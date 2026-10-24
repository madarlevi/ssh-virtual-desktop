#!/bin/sh
# Execute the actual Rust-generated worker with native OS utilities, without a GUI or user account.
set -eu
umask 077
fixtures=$(cd "$1" && pwd)
base=$(mktemp -d /tmp/svd-compat.XXXXXXXX)
case "$base" in /tmp/svd-compat.*) ;; *) exit 1;; esac
cd "$base"
server=''
cleanup() { if [ -n "$server" ]; then kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :; fi; rm -rf "$base"; }
trap cleanup EXIT INT TERM
filename=$(cat "$fixtures/filename.txt")
part=.svd-00000000-0000-0000-0000-000000000000.part
port=$((20000 + $$ % 30000))
export SVD_DEST SVD_JOB SVD_URL
SVD_URL="http://127.0.0.1:$port/attachment-without-extension"
prepare() {
  SVD_DEST="$base/-résumé ' folder-$1"
  SVD_JOB="$base/job-$1"
  mkdir "$SVD_DEST" "$SVD_JOB"
  printf '%s\n' "$SVD_URL" > "$SVD_JOB/request.urls"
  printf 'url = "%s"\noutput = "%s/%s"\nsilent\nwrite-out = "%%{http_code}"\n' "$SVD_URL" "$SVD_DEST" "$part" > "$SVD_JOB/request.conf"
  printf '%s\n' '#!/bin/sh' 'exec fetch -q -T 30 -N /dev/null -o "$SVD_DEST/.svd-00000000-0000-0000-0000-000000000000.part" "$SVD_URL"' > "$SVD_JOB/fetch.sh"
}
http() {
  mode=$1
  if [ "$mode" = slow ]; then
    (printf 'HTTP/1.1 200 OK\r\nContent-Length: 65536\r\nConnection: close\r\n\r\n'; i=0; while [ "$i" -lt 16 ]; do dd if=/dev/zero bs=4096 count=1 2>/dev/null; sleep 1; i=$((i+1)); done) | nc -N -l 127.0.0.1 "$port" > /dev/null &
  else
    printf 'HTTP/1.1 200 OK\r\nContent-Length: 16\r\nConnection: close\r\n\r\ncompatibility OK' | nc -N -l 127.0.0.1 "$port" > /dev/null &
  fi
  server=$!
  sleep 1
}
wait_status() {
  desired=$1
  n=0
  while [ "$n" -lt 20 ]; do
    status=$(cat "$SVD_JOB/status" 2>/dev/null || :)
    [ "$status" = "$desired" ] && return 0
    if [ "$status" = Failed ]; then cat "$SVD_JOB/error"; return 1; fi
    sleep 1; n=$((n+1))
  done
  printf 'Timeout waiting for %s\n' "$desired" >&2
  return 1
}
count=0
for backend in fetch curl wget aria2c; do
  command -v "$backend" >/dev/null 2>&1 || continue
  count=$((count+1)); printf 'Testing %s on %s\n' "$backend" "$(uname -s)"
  prepare "$backend-success"
  http fast
  /bin/sh "$fixtures/worker-$backend.sh"
  wait_status Completed
  [ "$(cat "$SVD_DEST/$filename")" = 'compatibility OK' ]
  [ ! -e "$SVD_JOB/request.conf" ] && [ ! -e "$SVD_JOB/request.urls" ] && [ ! -e "$SVD_JOB/fetch.sh" ]
  case "$(uname -s)" in FreeBSD) [ "$(stat -f %Lp "$SVD_DEST/$filename")" = 600 ];; Linux) [ "$(stat -c %a "$SVD_DEST/$filename")" = 600 ];; esac
  wait "$server" || :; server=''
  # Existing file must survive a second invocation.
  /bin/sh "$fixtures/worker-$backend.sh" || :
  [ "$(cat "$SVD_JOB/status")" = Failed ]
  [ "$(cat "$SVD_DEST/$filename")" = 'compatibility OK' ]
  prepare "$backend-cancel-before-start"
  : > "$SVD_JOB/cancel"
  /bin/sh "$fixtures/worker-$backend.sh"
  wait_status Cancelled
  [ ! -e "$SVD_DEST/$filename" ]
  prepare "$backend-detached-cancel"
  http slow
  case "$(uname -s)" in
    FreeBSD) /usr/sbin/daemon -f /bin/sh "$fixtures/worker-$backend.sh";;
    Linux) /bin/sh -c 'nohup setsid /bin/sh "$1" </dev/null >/dev/null 2>&1 &' sh "$fixtures/worker-$backend.sh";;
    *) printf 'Unsupported test OS\n' >&2; exit 1;;
  esac
  wait_status Downloading
  sleep 2
  [ -s "$SVD_DEST/$part" ]
  : > "$SVD_JOB/cancel"
  wait_status Cancelled
  [ ! -e "$SVD_DEST/$part" ] && [ ! -e "$SVD_DEST/$filename" ]
  kill "$server" 2>/dev/null || :; wait "$server" 2>/dev/null || :; server=''
  port=$((port+1)); SVD_URL="http://127.0.0.1:$port/file"
  printf 'PASS: %s success, collision, restrictive mode, cleanup, detached progress and cancellation\n' "$backend"
done
[ "$count" -gt 0 ]
[ ! -e "$base/nope" ]

# Session-only Bash integration. This file is removed as soon as Bash reads it.
if [[ -n ${SVD_TERMINAL_RC-} ]]; then
  command rm -f "$SVD_TERMINAL_RC"
  unset SVD_TERMINAL_RC
fi
if [[ ${SVD_TERMINAL_TEST-} != 1 && -r ~/.bashrc ]]; then
  source ~/.bashrc
fi

__svd_prompt() {
  # Separate prompt/input/command boundaries; never guess from echoed keystrokes.
  printf '\033]133;D\007'
  printf '\033]777;svd-cwd;%s\007' "$PWD"
  PS1='\[\033]133;A\007\]\[\033[38;2;113;221;162m\]\w\[\033[0m\] \[\033[38;2;113;221;162m\]❯\[\033[97m\] \[\033]133;B\007\]'
  PS2='\[\033[97m\]… '
}
# Flatten existing hooks into a scalar: Bash before 5.1 cannot execute an array.
# Bash evaluates the hooks itself; no eval or DEBUG trap is installed.
__svd_prompt_hooks=("${PROMPT_COMMAND[@]}")
unset PROMPT_COMMAND
printf -v PROMPT_COMMAND '%s\n' "${__svd_prompt_hooks[@]}" __svd_prompt
unset __svd_prompt_hooks
PS0=$'\033]133;C\007\033[38;2;132;190;255m'
printf '\033]777;svd-ready;bash\007'

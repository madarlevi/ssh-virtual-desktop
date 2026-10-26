//! Test-only HTTP/1 fixtures. All nonblocking socket I/O waits for OS readiness.
use std::{
    io::{self, ErrorKind},
    net::SocketAddr,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::{
    io::Interest,
    net::{TcpListener, TcpStream},
    sync::oneshot,
    time::{timeout_at, Instant},
};

const ACCEPT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_REQUEST: usize = 65536;

pub(super) enum Response {
    Complete(Vec<u8>),
    // Keep an incomplete response open until the downloader cancels. No timing sleeps.
    UntilPeerCloses(Vec<u8>),
}

pub(super) struct Fixture {
    pub address: SocketAddr,
    requests: mpsc::Receiver<io::Result<String>>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<io::Result<()>>>,
}

impl Fixture {
    pub fn start(responses: Vec<Response>) -> io::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (requests_tx, requests) = mpsc::channel();
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let thread = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()?;
            runtime.block_on(async move {
                let listener = TcpListener::from_std(listener)?;
                ready_tx
                    .send(())
                    .map_err(|_| closed("Fixture owner closed"))?;
                for response in responses {
                    let request = tokio::select! {
                        _ = &mut stop_rx => return Ok(()),
                        result = serve(&listener, response) => result,
                    };
                    let failed = request.is_err();
                    requests_tx
                        .send(request)
                        .map_err(|_| closed("Fixture owner closed"))?;
                    if failed {
                        return Err(io::Error::other("HTTP fixture failed; see request error"));
                    }
                }
                Ok(())
            })
        });
        let fixture = Self {
            address,
            requests,
            stop: Some(stop_tx),
            thread: Some(thread),
        };
        ready_rx.recv_timeout(IO_TIMEOUT).map_err(|_| {
            io::Error::new(ErrorKind::TimedOut, "HTTP fixture did not become ready")
        })?;
        Ok(fixture)
    }

    pub fn next_request(&self) -> io::Result<String> {
        self.requests
            .recv_timeout(ACCEPT_TIMEOUT + IO_TIMEOUT)
            .map_err(|_| {
                io::Error::new(
                    ErrorKind::TimedOut,
                    "HTTP fixture did not report a complete request",
                )
            })?
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.thread
            .take()
            .ok_or_else(|| io::Error::other("Fixture already joined"))?
            .join()
            .map_err(|_| io::Error::other("HTTP fixture thread panicked"))?
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        // Stop also interrupts an accept/read if the test unwinds before its next request.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn closed(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::BrokenPipe, message)
}
fn expired() -> io::Error {
    io::Error::new(ErrorKind::TimedOut, "HTTP fixture I/O deadline expired")
}

async fn ready_io<T>(
    stream: &TcpStream,
    interest: Interest,
    deadline: Instant,
    mut operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    loop {
        if Instant::now() >= deadline {
            return Err(expired());
        }
        timeout_at(deadline, stream.ready(interest))
            .await
            .map_err(|_| expired())??;
        if Instant::now() >= deadline {
            return Err(expired());
        }
        match operation() {
            // Readiness may be spurious. WouldBlock clears it; await a new notification.
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {
                continue
            }
            result => return result,
        }
    }
}

async fn read_some(stream: &TcpStream, buffer: &mut [u8], deadline: Instant) -> io::Result<usize> {
    ready_io(stream, Interest::READABLE, deadline, || {
        stream.try_read(buffer)
    })
    .await
}

async fn write_all(stream: &TcpStream, mut data: &[u8], deadline: Instant) -> io::Result<()> {
    while !data.is_empty() {
        match ready_io(stream, Interest::WRITABLE, deadline, || {
            stream.try_write(data)
        })
        .await?
        {
            0 => {
                return Err(io::Error::new(
                    ErrorKind::WriteZero,
                    "HTTP response write stopped",
                ))
            }
            n => data = &data[n..],
        }
    }
    Ok(())
}

#[derive(Default)]
struct RequestBuffer {
    data: Vec<u8>,
    length: Option<usize>,
}
impl RequestBuffer {
    fn push(&mut self, bytes: &[u8]) -> io::Result<bool> {
        if self.data.len() + bytes.len() > MAX_REQUEST {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Fixture request exceeds 64 KiB",
            ));
        }
        self.data.extend_from_slice(bytes);
        if self.length.is_none() {
            if let Some(end) = self.data.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&self.data[..end]).map_err(|_| {
                    io::Error::new(ErrorKind::InvalidData, "Fixture headers are not UTF-8")
                })?;
                let mut length = None;
                for line in headers.lines().skip(1) {
                    let Some((name, value)) = line.split_once(':') else {
                        return Err(io::Error::new(
                            ErrorKind::InvalidData,
                            "Malformed fixture header",
                        ));
                    };
                    if name.eq_ignore_ascii_case("transfer-encoding") {
                        return Err(io::Error::new(
                            ErrorKind::InvalidData,
                            "Fixture expects Content-Length framing",
                        ));
                    }
                    if name.eq_ignore_ascii_case("content-length") {
                        if length.is_some() {
                            return Err(io::Error::new(
                                ErrorKind::InvalidData,
                                "Duplicate Content-Length",
                            ));
                        }
                        length = Some(value.trim().parse::<usize>().map_err(|_| {
                            io::Error::new(ErrorKind::InvalidData, "Invalid Content-Length")
                        })?);
                    }
                }
                self.length = Some(
                    (end + 4)
                        .checked_add(length.unwrap_or(0))
                        .filter(|n| *n <= MAX_REQUEST)
                        .ok_or_else(|| {
                            io::Error::new(ErrorKind::InvalidData, "Fixture body exceeds limit")
                        })?,
                );
            }
        }
        if self.length.is_some_and(|n| self.data.len() > n) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Unexpected bytes after fixture request",
            ));
        }
        Ok(self.length == Some(self.data.len()))
    }
}

async fn read_request(stream: &TcpStream, deadline: Instant) -> io::Result<String> {
    let mut request = RequestBuffer::default();
    loop {
        let mut buffer = [0; 4096];
        let n = read_some(stream, &mut buffer, deadline).await?;
        if n == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "Peer closed before complete HTTP request",
            ));
        }
        if request.push(&buffer[..n])? {
            return String::from_utf8(request.data).map_err(|_| {
                io::Error::new(ErrorKind::InvalidData, "Fixture request is not UTF-8")
            });
        }
    }
}

async fn serve(listener: &TcpListener, response: Response) -> io::Result<String> {
    let accept_deadline = Instant::now() + ACCEPT_TIMEOUT;
    let (stream, _) = loop {
        if Instant::now() >= accept_deadline {
            return Err(expired());
        }
        match timeout_at(accept_deadline, listener.accept())
            .await
            .map_err(|_| expired())?
        {
            Err(e) if matches!(e.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) => {
                continue
            }
            result => break result?,
        }
    };
    let deadline = Instant::now() + IO_TIMEOUT;
    let request = read_request(&stream, deadline).await?;
    let data = match &response {
        Response::Complete(data) | Response::UntilPeerCloses(data) => data,
    };
    write_all(&stream, data, deadline).await?;
    if matches!(response, Response::UntilPeerCloses(_)) {
        // The incomplete slow-response fixture can finish only when the worker closes it.
        let deadline = Instant::now() + ACCEPT_TIMEOUT;
        match read_some(&stream, &mut [0; 128], deadline).await {
            Ok(0) => (),
            Ok(_) => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Unexpected second request",
                ))
            }
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                ) => {}
            Err(e) => return Err(e),
        }
    }
    // Dropping the fully consumed connection closes it without leaving unread POST bytes.
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{future::Future, io::Read, task::Poll};
    use tokio::io::AsyncWriteExt;

    async fn sockets() -> io::Result<(TcpStream, TcpStream)> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let client = TcpStream::connect(listener.local_addr()?).await?;
        let (server, _) = listener.accept().await?;
        Ok((client, server))
    }

    #[tokio::test]
    async fn would_block_before_request_arrives_waits_for_readiness() -> io::Result<()> {
        let (mut client, server) = sockets().await?;
        // Use the OS socket directly; Tokio may synthesize WouldBlock before an OS call.
        let mut raw = server.into_std()?;
        let initial = raw
            .read(&mut [0; 1])
            .expect_err("Client has deliberately sent no bytes");
        assert_eq!(initial.kind(), ErrorKind::WouldBlock);
        #[cfg(windows)]
        assert_eq!(initial.raw_os_error(), Some(10035));
        let server = TcpStream::from_std(raw)?;
        let request = read_request(&server, Instant::now() + IO_TIMEOUT);
        tokio::pin!(request);
        // Poll the fixture before sending anything: this deterministically reaches the old race.
        std::future::poll_fn(|cx| {
            assert!(request.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        let data = b"POST /fixture HTTP/1.1\r\nContent-Length: 4\r\n\r\nbody";
        client.write_all(data).await?;
        assert_eq!(request.await?.as_bytes(), data);
        Ok(())
    }

    #[test]
    fn partial_headers_and_body_are_not_accepted_early() -> io::Result<()> {
        let input = b"POST /fixture HTTP/1.1\r\ncontent-length: 8\r\n\r\na\r\n\r\nbcd";
        let mut request = RequestBuffer::default();
        for (index, byte) in input.iter().enumerate() {
            assert_eq!(request.push(&[*byte])?, index + 1 == input.len());
        }
        assert_eq!(request.data, input);
        for invalid in [
            "Content-Length: no",
            "Content-Length: 1\r\nContent-Length: 1",
            "Content-Length: 99999999",
            "Transfer-Encoding: chunked",
        ] {
            assert!(RequestBuffer::default()
                .push(format!("POST / HTTP/1.1\r\n{invalid}\r\n\r\n").as_bytes())
                .is_err());
        }
        Ok(())
    }

    #[tokio::test]
    async fn premature_eof_is_reported_instead_of_replaying_a_partial_post() -> io::Result<()> {
        let (mut client, server) = sockets().await?;
        client
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nx")
            .await?;
        client.shutdown().await?;
        assert_eq!(
            read_request(&server, Instant::now() + IO_TIMEOUT)
                .await
                .expect_err("Truncated POST")
                .kind(),
            ErrorKind::UnexpectedEof
        );
        Ok(())
    }

    #[tokio::test]
    async fn transient_errors_retry_but_real_errors_and_deadlines_fail() -> io::Result<()> {
        let (_client, server) = sockets().await?;
        let mut errors = [ErrorKind::Interrupted, ErrorKind::WouldBlock].into_iter();
        let value = ready_io(
            &server,
            Interest::WRITABLE,
            Instant::now() + IO_TIMEOUT,
            || match errors.next() {
                Some(kind) => Err(io::Error::from(kind)),
                None => Ok(42),
            },
        )
        .await?;
        assert_eq!(value, 42);
        let error = ready_io::<()>(
            &server,
            Interest::WRITABLE,
            Instant::now() + IO_TIMEOUT,
            || Err(io::Error::from(ErrorKind::PermissionDenied)),
        )
        .await
        .expect_err("Fatal error must surface");
        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
        let deadline = Instant::now() - Duration::from_nanos(1);
        let error = ready_io::<()>(&server, Interest::WRITABLE, deadline, || {
            panic!("No I/O after the deadline")
        })
        .await
        .expect_err("Expired deadline");
        assert_eq!(error.kind(), ErrorKind::TimedOut);
        Ok(())
    }

    #[tokio::test]
    async fn large_response_preserves_all_bytes_under_tcp_backpressure() -> io::Result<()> {
        let (client, server) = sockets().await?;
        let data: Vec<u8> = (0..2 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        let deadline = Instant::now() + IO_TIMEOUT;
        let sending = write_all(&server, &data, deadline);
        let receiving = async {
            let mut received = Vec::new();
            let mut buffer = [0; 137];
            while received.len() < data.len() {
                let n = read_some(&client, &mut buffer, deadline).await?;
                if n == 0 {
                    return Err(closed("Truncated response"));
                }
                received.extend_from_slice(&buffer[..n]);
            }
            Ok::<_, io::Error>(received)
        };
        let (sent, received) = tokio::join!(sending, receiving);
        sent?;
        assert_eq!(received?, data);
        Ok(())
    }
}

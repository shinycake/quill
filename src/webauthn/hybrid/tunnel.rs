//! The websocket to the tunnel server (tdesktop `cable_tunnel.cpp`
//! `TunnelSocket`): `wss://<domain>/cable/connect/<routing id>/<tunnel
//! id>` with the `fido.cable` subprotocol, binary messages both ways.
//!
//! TLS is rustls with the ring provider and the webpki roots (what ureq
//! already uses in Quill); the websocket is tungstenite over that stream.
//! The socket reads with a short timeout so the worker can notice Cancel
//! and the ceremony's deadline between reads.
#![deny(clippy::indexing_slicing)]

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::client::ClientRequestBuilder;
use tungstenite::handshake::HandshakeError;
use tungstenite::http::Uri;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

/// The websocket subprotocol of caBLE tunnels.
pub const SUBPROTOCOL: &str = "fido.cable";
/// The largest message Quill takes (tdesktop `kMaxFramePayload`).
const MAX_MESSAGE: usize = 4 * 1024 * 1024;
/// How long one read waits before the worker looks at Cancel again.
const READ_SLICE: Duration = Duration::from_millis(150);
/// How long a TCP connect may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a write may block.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// Why the tunnel ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelError {
    /// The user cancelled.
    Cancelled,
    /// The ceremony's deadline passed.
    Timeout,
    /// The server or phone closed the tunnel.
    Closed,
    /// Anything else; the text is for the log.
    Failed(String),
}

/// When to give up waiting: the ceremony's deadline, and Cancel.
pub struct Stop<'a> {
    pub deadline: Instant,
    pub cancelled: &'a dyn Fn() -> bool,
}

impl Stop<'_> {
    fn check(&self) -> Result<(), TunnelError> {
        if (self.cancelled)() {
            return Err(TunnelError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(TunnelError::Timeout);
        }
        Ok(())
    }
}

/// One open tunnel: binary messages to and from the phone.
pub trait Tunnel: Send {
    fn send(&mut self, data: &[u8], stop: &Stop<'_>) -> Result<(), TunnelError>;
    /// The next non-empty binary message.
    fn recv(&mut self, stop: &Stop<'_>) -> Result<Vec<u8>, TunnelError>;
    fn close(&mut self);
}

/// Opens tunnels.
pub trait Connector: Send {
    fn connect(
        &mut self,
        domain: &str,
        path: &str,
        stop: &Stop<'_>,
    ) -> Result<Box<dyn Tunnel>, TunnelError>;
}

/// A stream whose read timeouts read as "try again" on every OS (Windows
/// reports `TimedOut`, the others `WouldBlock`).
pub struct Patient<S>(pub S);

impl<S: Read> Read for Patient<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf).map_err(|error| match error.kind() {
            io::ErrorKind::TimedOut => io::Error::from(io::ErrorKind::WouldBlock),
            _ => error,
        })
    }
}

impl<S: Write> Write for Patient<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn would_block(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(io) if io.kind() == io::ErrorKind::WouldBlock)
}

fn failed(error: impl std::fmt::Display) -> TunnelError {
    TunnelError::Failed(error.to_string())
}

/// A tunnel over any byte stream (TLS in a ceremony, plain TCP in tests).
pub struct WsTunnel<S: Read + Write> {
    socket: WebSocket<S>,
}

impl<S: Read + Write + Send> WsTunnel<S> {
    /// The websocket upgrade over `stream`, for `wss://<host><path>`
    /// (`ws://` when `tls` is false; tests).
    pub fn open(
        stream: S,
        host: &str,
        path: &str,
        tls: bool,
        stop: &Stop<'_>,
    ) -> Result<Self, TunnelError> {
        let scheme = if tls { "wss" } else { "ws" };
        let uri: Uri = format!("{scheme}://{host}{path}").parse().map_err(failed)?;
        let request = ClientRequestBuilder::new(uri).with_sub_protocol(SUBPROTOCOL);
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE))
            .max_frame_size(Some(MAX_MESSAGE));
        let mut attempt = tungstenite::client::client_with_config(request, stream, Some(config));
        loop {
            match attempt {
                Ok((socket, _)) => return Ok(Self { socket }),
                Err(HandshakeError::Interrupted(mid)) => {
                    stop.check()?;
                    attempt = mid.handshake();
                }
                Err(HandshakeError::Failure(error)) => return Err(failed(error)),
            }
        }
    }

    /// Push queued frames out (pongs, the message just written).
    fn flush(&mut self, stop: &Stop<'_>) -> Result<(), TunnelError> {
        loop {
            match self.socket.flush() {
                Ok(()) => return Ok(()),
                Err(error) if would_block(&error) => stop.check()?,
                Err(error) => return Err(failed(error)),
            }
        }
    }
}

impl<S: Read + Write + Send> Tunnel for WsTunnel<S> {
    fn send(&mut self, data: &[u8], stop: &Stop<'_>) -> Result<(), TunnelError> {
        match self.socket.write(Message::Binary(data.to_vec().into())) {
            Ok(()) => {}
            Err(error) if would_block(&error) => {}
            Err(error) => return Err(failed(error)),
        }
        self.flush(stop)
    }

    fn recv(&mut self, stop: &Stop<'_>) -> Result<Vec<u8>, TunnelError> {
        loop {
            stop.check()?;
            match self.socket.read() {
                Ok(Message::Binary(data)) if !data.is_empty() => return Ok(data.to_vec()),
                // tdesktop ignores text, pongs and empty messages; a ping's
                // pong is queued by tungstenite and goes out here.
                Ok(Message::Ping(_)) => {
                    let _ = self.socket.flush();
                }
                Ok(Message::Close(_)) => return Err(TunnelError::Closed),
                Ok(_) => {}
                Err(error) if would_block(&error) => {}
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return Err(TunnelError::Closed);
                }
                Err(error) => return Err(failed(error)),
            }
        }
    }

    fn close(&mut self) {
        let _ = self.socket.close(None);
        let _ = self.socket.flush();
    }
}

/// A TCP connection to `host:port` that reads in short slices.
fn tcp(host: &str, port: u16, stop: &Stop<'_>) -> Result<TcpStream, TunnelError> {
    let addrs = (host, port).to_socket_addrs().map_err(failed)?;
    let mut last = TunnelError::Failed("no address".into());
    for addr in addrs {
        stop.check()?;
        let left = stop.deadline.saturating_duration_since(Instant::now());
        match TcpStream::connect_timeout(
            &addr,
            left.min(CONNECT_TIMEOUT).max(Duration::from_millis(1)),
        ) {
            Ok(stream) => {
                stream.set_read_timeout(Some(READ_SLICE)).map_err(failed)?;
                stream
                    .set_write_timeout(Some(WRITE_TIMEOUT))
                    .map_err(failed)?;
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => last = failed(error),
        }
    }
    Err(last)
}

/// The rustls client setup, made once.
fn tls_config() -> Option<Arc<rustls::ClientConfig>> {
    static CONFIG: std::sync::OnceLock<Option<Arc<rustls::ClientConfig>>> =
        std::sync::OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .ok()
                .map(|builder| {
                    Arc::new(builder.with_root_certificates(roots).with_no_client_auth())
                })
        })
        .clone()
}

/// The real tunnel servers: TLS on port 443.
pub struct TlsConnector;

type TlsStream = Patient<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>;

impl Connector for TlsConnector {
    fn connect(
        &mut self,
        domain: &str,
        path: &str,
        stop: &Stop<'_>,
    ) -> Result<Box<dyn Tunnel>, TunnelError> {
        let config = tls_config().ok_or_else(|| TunnelError::Failed("tls setup".into()))?;
        let name = rustls::pki_types::ServerName::try_from(domain.to_string()).map_err(failed)?;
        let connection = rustls::ClientConnection::new(config, name).map_err(failed)?;
        let socket = tcp(domain, 443, stop)?;
        let stream: TlsStream = Patient(rustls::StreamOwned::new(connection, socket));
        Ok(Box::new(WsTunnel::open(stream, domain, path, true, stop)?))
    }
}

/// A tunnel server on plain TCP at a fixed address (tests: the fake
/// phone's server on localhost).
#[cfg(test)]
pub struct PlainConnector {
    pub addr: std::net::SocketAddr,
    /// The paths asked for, to check the URL.
    pub paths: Arc<std::sync::Mutex<Vec<String>>>,
}

#[cfg(test)]
impl Connector for PlainConnector {
    fn connect(
        &mut self,
        domain: &str,
        path: &str,
        stop: &Stop<'_>,
    ) -> Result<Box<dyn Tunnel>, TunnelError> {
        if let Ok(mut paths) = self.paths.lock() {
            paths.push(format!("{domain}{path}"));
        }
        let socket = tcp(&self.addr.ip().to_string(), self.addr.port(), stop)?;
        Ok(Box::new(WsTunnel::open(
            Patient(socket),
            domain,
            path,
            false,
            stop,
        )?))
    }
}

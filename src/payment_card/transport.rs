//! The one HTTP call a tokenization makes, behind a trait so tests answer
//! it from recorded responses. The real transport is `ureq` over rustls.
#![deny(clippy::indexing_slicing)]

use zeroize::Zeroizing;

/// A tokenize `POST`. The body holds card data and is wiped when dropped.
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Zeroizing<Vec<u8>>,
}

/// The provider's answer: HTTP status and body bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Why no answer came back, as Qt's `QNetworkReply::NetworkError` numbers
/// tdesktop prints in "RequestError<n>".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    ConnectionRefused,
    HostNotFound,
    Timeout,
    Tls,
    Other,
}

impl TransportError {
    pub fn qt_code(self) -> u16 {
        match self {
            Self::ConnectionRefused => 1,
            Self::HostNotFound => 3,
            Self::Timeout => 4,
            Self::Tls => 6,
            Self::Other => 99,
        }
    }
}

/// Sends one request. Implementations block; callers run them off the UI
/// thread.
pub trait Transport: Send + Sync {
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// Largest provider answer read; a token answer is a few hundred bytes.
#[cfg(feature = "ui")]
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// HTTPS only, no redirects, answers of any status read as data (Stripe
/// reports card errors with a 402 and a JSON body).
#[cfg(feature = "ui")]
pub struct UreqTransport {
    agent: ureq::Agent,
}

#[cfg(feature = "ui")]
impl Default for UreqTransport {
    fn default() -> Self {
        let agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .build()
            .new_agent();
        Self { agent }
    }
}

#[cfg(feature = "ui")]
fn transport_error(error: &ureq::Error) -> TransportError {
    match error {
        ureq::Error::HostNotFound => TransportError::HostNotFound,
        ureq::Error::ConnectionFailed => TransportError::ConnectionRefused,
        ureq::Error::Timeout(_) => TransportError::Timeout,
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => TransportError::Tls,
        _ => TransportError::Other,
    }
}

#[cfg(feature = "ui")]
impl Transport for UreqTransport {
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let mut builder = self.agent.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(*name, value);
        }
        let mut response = builder
            .send(request.body.as_slice())
            .map_err(|error| transport_error(&error))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|error| transport_error(&error))?;
        Ok(HttpResponse { status, body })
    }
}

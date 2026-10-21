//! Wire types and rules of the web proxy carrier mode
//! (`quill-webview --carrier`, `parity:data-proxy-web`).
//!
//! TDLib 1.8.69 carries a web proxy's MTProto traffic through an HTTPS
//! relay page that the app loads in a hidden web view
//! (`updateWebProxyCarrierOpened`). The helper process hosts one carrier
//! per process: Quill sends [`CarrierCommand::Open`] as the first line,
//! then the bytes TDLib wants delivered; the helper answers with what the
//! page posted to the bridge object and with failures. Binary data stays
//! base64 end to end (TDLib's JSON `bytes`), so neither side decodes it.
//!
//! [`CarrierScope`] holds the rules both sides check: the origin and path
//! prefix the page may navigate within, the bridge object's name, and the
//! sources a bridge message may come from.

use serde::{Deserialize, Serialize};

/// Longest binary message the page may post, decoded. TDLib coalesces at
/// most 1 MiB per update (`MAX_CARRIER_DATA_SIZE`); the page's frames are
/// bounded the same way, so twice that leaves room without letting a page
/// make Quill hold arbitrary amounts.
pub const MAX_PAGE_BINARY_BYTES: usize = 2 * 1024 * 1024;

/// Longest text message the page may post (TDLib `MAX_CARRIER_TEXT_SIZE`).
pub const MAX_PAGE_TEXT_BYTES: usize = 64 * 1024;

/// Longest delivery Quill passes to the page, decoded (TDLib sends at most
/// 1 MiB per `updateWebProxyCarrierData`; a little slack for headers).
pub const MAX_DELIVERY_BYTES: usize = 1024 * 1024 + 4096;

/// Bytes waiting for the page before the carrier counts as hung: TDLib's
/// own flow control keeps far less than this in flight.
pub const MAX_PENDING_BYTES: usize = 16 * 1024 * 1024;

/// Deliveries waiting for the page before the carrier counts as hung.
pub const MAX_PENDING_ITEMS: usize = 1024;

/// Base64 length of `bytes` decoded bytes (padded).
pub const fn base64_len(bytes: usize) -> usize {
    bytes.div_ceil(3).saturating_mul(4)
}

/// Longest protocol line either side reads: a page binary message in
/// base64 plus its JSON envelope.
pub const MAX_LINE_BYTES: usize = base64_len(MAX_PAGE_BINARY_BYTES) + 1024;

/// Why a carrier failed, in the shape of TDLib's `WebProxyCarrierError`
/// (`Paused` and `Refused` are Quill's own decisions, never the helper's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CarrierFailure {
    /// No hidden web view on this system (no WebKitGTK, no WebView2
    /// runtime): TDLib stops asking until the proxy is enabled again.
    Unsupported,
    /// The page could not load, or tried to leave its origin. `message`
    /// is for TDLib's log and never contains the URL.
    LoadFailed { message: String },
    /// The web view or its renderer died or stopped answering, or the
    /// relay between Quill and the page broke.
    Terminated,
}

/// Quill → helper, one JSON line each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CarrierCommand {
    /// Always the first line. Sent over the pipe rather than the command
    /// line, so the capability in the URL never shows up in `ps`.
    Open {
        url: String,
        origin: String,
        path_prefix: String,
        bridge_object_name: String,
    },
    /// Data for the page's `onmessage`, as an `ArrayBuffer`. Base64.
    Deliver { data: String },
    /// TDLib closed the carrier. With `notify_page` the page first gets
    /// the string `{"t":"close"}` and the view goes about 200 ms later.
    Close { notify_page: bool },
}

/// Helper → Quill, one JSON line each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CarrierEvent {
    /// The hidden window and its web view exist; the page is loading.
    Ready,
    /// The helper's loop and the page are alive (every few seconds).
    Alive,
    /// The page posted an `ArrayBuffer` or a typed array. Base64.
    Binary { data: String },
    /// The page posted a string.
    Text { text: String },
    /// The carrier cannot go on; the helper exits after this.
    Failed { failure: CarrierFailure },
}

/// The carrier's origin, path prefix and bridge object name, validated so
/// they can be compared against URLs and written into a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierScope {
    origin: String,
    host: String,
    path_prefix: String,
    bridge_object_name: String,
}

/// Longest bridge object name accepted (TDLib always sends
/// `TelegramWebProxy`).
const MAX_BRIDGE_NAME: usize = 64;

impl CarrierScope {
    /// `None` unless `origin` is `https://<host>` (a lower-case DNS name,
    /// no port or path), `path_prefix` is `/` or `/seg/.../` and the name is
    /// a plain JavaScript identifier. With `allow_loopback_http`, also
    /// `http://127.0.0.1:<port>` (the helper's own tests; never in a
    /// release build).
    pub fn new(
        origin: &str,
        path_prefix: &str,
        bridge_object_name: &str,
        allow_loopback_http: bool,
    ) -> Option<Self> {
        let host = if let Some(host) = origin.strip_prefix("https://") {
            valid_host(host).then(|| host.to_string())?
        } else if allow_loopback_http {
            let rest = origin.strip_prefix("http://127.0.0.1:")?;
            let port: u16 = rest.parse().ok()?;
            (port != 0 && rest == port.to_string()).then(|| format!("127.0.0.1:{port}"))?
        } else {
            return None;
        };
        if !valid_path_prefix(path_prefix) || !valid_identifier(bridge_object_name) {
            return None;
        }
        Some(Self {
            origin: origin.to_string(),
            host,
            path_prefix: path_prefix.to_string(),
            bridge_object_name: bridge_object_name.to_string(),
        })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn path_prefix(&self) -> &str {
        &self.path_prefix
    }

    pub fn bridge_object_name(&self) -> &str {
        &self.bridge_object_name
    }

    /// Whether the scope's origin is plain `http` (loopback tests only).
    pub fn is_loopback_http(&self) -> bool {
        self.origin.starts_with("http://")
    }

    /// The WebSocket form of the scope (`wss://host/prefix/`), for the
    /// page's Content-Security-Policy.
    pub fn socket_source(&self) -> String {
        let scheme = if self.is_loopback_http() { "ws" } else { "wss" };
        format!("{scheme}://{}{}", self.host, self.path_prefix)
    }

    /// The HTTP form of the scope (`https://host/prefix/`).
    pub fn page_source(&self) -> String {
        format!("{}{}", self.origin, self.path_prefix)
    }

    /// Whether the web view may load `url` as a document: the exact
    /// origin (default port only), no user info, a path under the prefix.
    /// Everything else, `about:blank` included, is refused.
    pub fn allows_navigation(&self, url: &str) -> bool {
        self.same_origin_path(url)
            .is_some_and(|path| path.starts_with(&self.path_prefix))
    }

    /// Whether a bridge message whose sending frame reports `url` may be
    /// passed on: a document of the carrier's origin. An empty URL
    /// (platforms that do not report the frame) is left to the bridge's
    /// per-carrier token.
    pub fn allows_source(&self, url: &str) -> bool {
        url.is_empty() || self.same_origin_path(url).is_some()
    }

    /// The path of `url` when it is on this origin, else `None`.
    fn same_origin_path<'a>(&self, url: &'a str) -> Option<&'a str> {
        let rest = url.strip_prefix(&self.origin)?;
        // `https://host` must not match `https://host.evil.example`, a
        // port, or user info, so the origin has to end right here.
        if !(rest.is_empty() || rest.starts_with(['/', '?', '#'])) {
            return None;
        }
        let end = rest.find(['?', '#']).unwrap_or(rest.len());
        let path = rest.get(..end)?;
        let path = if path.is_empty() { "/" } else { path };
        // No dot segments or encoded slashes that could step out of the
        // prefix once the engine normalizes the path.
        let lower = path.to_ascii_lowercase();
        if path.split('/').any(|segment| segment == "." || segment == "..")
            || lower.contains("%2f")
            || lower.contains("%5c")
            || lower.contains("%2e")
            || path.contains('\\')
        {
            return None;
        }
        Some(path)
    }
}

/// A lower-case DNS name with at least one dot: what TDLib builds the
/// origin from (`Proxy::server()`, already punycoded).
fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// `/`, or `/`-separated segments of `[A-Za-z0-9_-]` with a slash at both
/// ends (TDLib: `'/' + proxy.path() + '/'`).
fn valid_path_prefix(prefix: &str) -> bool {
    if prefix == "/" {
        return true;
    }
    let Some(inner) = prefix
        .strip_prefix('/')
        .and_then(|rest| rest.strip_suffix('/'))
    else {
        return false;
    };
    prefix.len() <= 256
        && inner.split('/').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= MAX_BRIDGE_NAME
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Whether `text` is standard padded or unpadded base64 (what TDLib's JSON
/// `bytes` and the bridge's `btoa` produce), without decoding it.
pub fn is_base64(text: &str) -> bool {
    let body = text.trim_end_matches('=');
    text.len() - body.len() <= 2
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode};

    fn scope(prefix: &str) -> CarrierScope {
        CarrierScope::new(
            "https://proxy.example.com",
            prefix,
            "TelegramWebProxy",
            false,
        )
        .unwrap()
    }

    #[test]
    fn scopes_take_only_what_tdlib_builds() {
        assert!(CarrierScope::new("https://a.example", "/", "TelegramWebProxy", false).is_some());
        assert!(
            CarrierScope::new("https://a.example", "/relay/v1/", "TelegramWebProxy", false)
                .is_some()
        );
        for (origin, prefix, name) in [
            ("http://a.example", "/", "TelegramWebProxy"),
            ("https://a.example/", "/", "TelegramWebProxy"),
            ("https://a.example:8443", "/", "TelegramWebProxy"),
            ("https://A.example", "/", "TelegramWebProxy"),
            ("https://localhost", "/", "TelegramWebProxy"),
            ("https://a.example", "relay/", "TelegramWebProxy"),
            ("https://a.example", "/relay", "TelegramWebProxy"),
            ("https://a.example", "//", "TelegramWebProxy"),
            ("https://a.example", "/a b/", "TelegramWebProxy"),
            ("https://a.example", "/'x/", "TelegramWebProxy"),
            ("https://a.example", "/", "Telegram.WebProxy"),
            ("https://a.example", "/", "1Bridge"),
            ("https://a.example", "/", ""),
            ("https://a.example", "/", "x'];alert(1);//"),
        ] {
            assert!(
                CarrierScope::new(origin, prefix, name, false).is_none(),
                "{origin} {prefix} {name}"
            );
        }
    }

    #[test]
    fn loopback_http_needs_the_test_switch() {
        assert!(CarrierScope::new("http://127.0.0.1:8080", "/", "B", false).is_none());
        let local = CarrierScope::new("http://127.0.0.1:8080", "/", "B", true).unwrap();
        assert!(local.allows_navigation("http://127.0.0.1:8080/?bridge=x#android=y"));
        assert_eq!(local.socket_source(), "ws://127.0.0.1:8080/");
        assert!(CarrierScope::new("http://127.0.0.1:0", "/", "B", true).is_none());
        assert!(CarrierScope::new("http://127.0.0.1:080", "/", "B", true).is_none());
        assert!(CarrierScope::new("http://10.0.0.1:80", "/", "B", true).is_none());
    }

    #[test]
    fn navigation_stays_on_the_origin_and_under_the_prefix() {
        let root = scope("/");
        assert!(root.allows_navigation("https://proxy.example.com"));
        assert!(root.allows_navigation("https://proxy.example.com/?bridge=abc#android=n"));
        assert!(root.allows_navigation("https://proxy.example.com/any/where"));
        let relay = scope("/relay/");
        assert!(relay.allows_navigation("https://proxy.example.com/relay/?bridge=abc"));
        assert!(relay.allows_navigation("https://proxy.example.com/relay/page#x"));
        for url in [
            "https://proxy.example.com/",
            "https://proxy.example.com/relay",
            "https://proxy.example.com/relayx/",
            "https://proxy.example.com/other/",
            "https://proxy.example.com/relay/../admin/",
            "https://proxy.example.com/relay/%2e%2e/admin/",
            "https://proxy.example.com/relay/..%2fadmin",
            "https://proxy.example.com/relay/a\\b",
            "https://proxy.example.com.evil.example/relay/",
            "https://proxy.example.com:444/relay/",
            "https://proxy.example.com@evil.example/relay/",
            "http://proxy.example.com/relay/",
            "about:blank",
            "javascript:alert(1)",
            "data:text/html,x",
            "",
        ] {
            assert!(!relay.allows_navigation(url), "{url}");
        }
    }

    #[test]
    fn bridge_messages_come_only_from_the_carrier_origin() {
        let relay = scope("/relay/");
        assert!(relay.allows_source("https://proxy.example.com/relay/?bridge=abc#android=n"));
        // Same origin, other path: still the carrier's origin (the schema
        // scopes the bridge object by origin).
        assert!(relay.allows_source("https://proxy.example.com/"));
        // Linux reports no frame URL; the token decides there.
        assert!(relay.allows_source(""));
        for url in [
            "https://evil.example/",
            "https://proxy.example.com.evil.example/",
            "https://proxy.example.com:8443/",
            "http://proxy.example.com/",
            "about:blank",
            "about:srcdoc",
            "blob:https://proxy.example.com/1234",
            "null",
        ] {
            assert!(!relay.allows_source(url), "{url}");
        }
    }

    #[test]
    fn base64_check_takes_tdlib_and_btoa_output() {
        assert!(is_base64("AAECAw=="));
        assert!(is_base64("AAECAw"));
        assert!(is_base64(""));
        assert!(is_base64("+/+/"));
        assert!(!is_base64("AA=A"));
        assert!(!is_base64("AA==="));
        assert!(!is_base64("AA\"}"));
        assert!(!is_base64("AA\nBB"));
    }

    #[test]
    fn carrier_lines_round_trip() {
        let open = CarrierCommand::Open {
            url: "https://proxy.example.com/?bridge=x#android=y".into(),
            origin: "https://proxy.example.com".into(),
            path_prefix: "/".into(),
            bridge_object_name: "TelegramWebProxy".into(),
        };
        let line = encode(&open);
        assert_eq!(line.matches('\n').count(), 1);
        assert_eq!(decode::<CarrierCommand>(&line).unwrap(), open);
        let failed = CarrierEvent::Failed {
            failure: CarrierFailure::LoadFailed {
                message: "forbidden navigation".into(),
            },
        };
        assert_eq!(
            encode(&failed).trim(),
            r#"{"kind":"failed","failure":{"kind":"load_failed","message":"forbidden navigation"}}"#
        );
        assert_eq!(decode::<CarrierEvent>(&encode(&failed)).unwrap(), failed);
        assert!(decode::<CarrierEvent>(r#"{"kind":"binary"}"#).is_err());
    }

    #[test]
    fn line_cap_fits_the_largest_page_message() {
        let data = "A".repeat(base64_len(MAX_PAGE_BINARY_BYTES));
        let line = encode(&CarrierEvent::Binary { data });
        assert!(line.len() <= MAX_LINE_BYTES);
    }
}

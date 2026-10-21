//! Web proxies (`parity:data-proxy-web`): TDLib 1.8.69 `proxyTypeWeb`.
//! A web proxy is a host name with an optional base path, always on port
//! 443, plus an MTProto secret. Address rules, the link secret encoding and
//! the row status follow tdesktop's `mtproto/mtproto_proxy_data.cpp`
//! (`NormalizeWebProxyHost`, `setWebAddress`, `EncodeWebProxyLinkSecret`,
//! `DecodeWebProxyLinkSecret`) and `boxes/connection_box.cpp`.
#![deny(clippy::indexing_slicing)]

use base64::Engine as _;
use std::net::IpAddr;

/// A web proxy always talks HTTPS on this port.
pub const WEB_PROXY_PORT: u16 = 443;

/// First byte of a link secret that carries a base path: tdesktop's marker,
/// chosen so an older client reports the link as unsupported instead of
/// using the host root.
const LINK_SECRET_MARKER: u8 = 0x70;

const MAX_BASE_PATH: usize = 128;

fn last_label_is_numeric(host: &str) -> bool {
    let label = host.rsplit('.').next().unwrap_or("");
    if label.is_empty() {
        return false;
    }
    let (hex, digits) = match label.get(..2) {
        Some(prefix) if prefix.eq_ignore_ascii_case("0x") => (true, label.get(2..).unwrap_or("")),
        _ => (false, label),
    };
    digits
        .chars()
        .all(|c| c.is_ascii_digit() || (hex && c.is_ascii_hexdigit()))
}

/// tdesktop `NormalizeWebProxyHost`: a lower-case ASCII (punycode) domain
/// name with at least one dot; no port, path, login, IP address or
/// all-numeric last label.
pub fn normalize_host(value: &str) -> Option<String> {
    let input = value.trim();
    if input.is_empty() || input.contains([':', '/', '?', '#', '@']) || input.ends_with('.') {
        return None;
    }
    let host = idna::domain_to_ascii(input).ok()?.to_lowercase();
    if host.is_empty() || host.len() > 253 || !host.contains('.') {
        return None;
    }
    for label in host.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return None;
        }
    }
    if last_label_is_numeric(&host) || host.parse::<IpAddr>().is_ok() {
        return None;
    }
    Some(host)
}

fn trim_base_path(value: &str) -> &str {
    let value = value.trim();
    let value = value.strip_prefix('/').unwrap_or(value);
    value.strip_suffix('/').unwrap_or(value)
}

/// tdesktop `NormalizeWebProxyBasePath`: one or more `/`-separated
/// segments of `[A-Za-z0-9][A-Za-z0-9_-]*`, at most 128 characters.
pub fn normalize_base_path(value: &str) -> Option<String> {
    let path = trim_base_path(value);
    if path.is_empty() || path.len() > MAX_BASE_PATH {
        return None;
    }
    let mut segment_start = true;
    for c in path.chars() {
        if c == '/' {
            if segment_start {
                return None;
            }
            segment_start = true;
            continue;
        }
        let extra = c == '-' || c == '_';
        if !c.is_ascii_alphanumeric() && !(extra && !segment_start) {
            return None;
        }
        segment_start = false;
    }
    (!segment_start).then(|| path.to_string())
}

/// tdesktop `setWebAddress`: an address field value (a pasted `https://`
/// URL is fine) into the host and the base path (empty for the root).
pub fn parse_address(value: &str) -> Option<(String, String)> {
    let trimmed = value.trim();
    let trimmed = match trimmed.get(..8) {
        Some(scheme) if scheme.eq_ignore_ascii_case("https://") => trimmed.get(8..)?,
        _ => trimmed,
    };
    let (host_part, path_part) = match trimmed.split_once('/') {
        Some((host, path)) => (host, path),
        None => (trimmed, ""),
    };
    let host = normalize_host(host_part)?;
    let path = if trim_base_path(path_part).is_empty() {
        String::new()
    } else {
        normalize_base_path(path_part)?
    };
    Some((host, path))
}

/// `host` or `host/path`, what the form, rows and links show.
pub fn address(host: &str, path: &str) -> String {
    if path.is_empty() {
        host.to_string()
    } else {
        format!("{host}/{path}")
    }
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len() / 2)
        .map(|i| {
            hex.get(2 * i..2 * i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// tdesktop `EncodeWebProxyLinkSecret`: with a base path the link carries
/// `base64url(0x70 + secret)`, otherwise the plain secret.
pub fn encode_link_secret(path: &str, secret_hex: &str) -> String {
    let Some(bytes) = hex_to_bytes(secret_hex).filter(|b| !b.is_empty()) else {
        return secret_hex.to_string();
    };
    if path.is_empty() {
        return secret_hex.to_string();
    }
    let mut marked = vec![LINK_SECRET_MARKER];
    marked.extend(bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(marked)
}

/// tdesktop `DecodeWebProxyLinkSecret`: the marked form gives the hex
/// secret; an unmarked secret is only accepted on a link without a base
/// path (`None` otherwise).
pub fn decode_link_secret(value: &str, has_base_path: bool) -> Option<String> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value.trim_end_matches('='))
        .ok();
    if let Some(bytes) = decoded
        && bytes.len() >= 17
        && bytes.first() == Some(&LINK_SECRET_MARKER)
    {
        return Some(bytes_to_hex(bytes.get(1..)?));
    }
    (!has_base_path).then(|| value.to_string())
}

/// The state the row of a web proxy shows (tdesktop
/// `ProxiesBoxController::updateView` maps the transport state the same
/// way). Web proxies are never pinged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRowState {
    NotTested,
    Connecting,
    Online,
    /// The app could not open the proxy's relay page.
    Unavailable,
}

impl WebRowState {
    pub fn for_entry(enabled: bool, connected: bool, carrier_failed: bool) -> Self {
        if !enabled {
            Self::NotTested
        } else if carrier_failed {
            Self::Unavailable
        } else if connected {
            Self::Online
        } else {
            Self::Connecting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = "8561944064fc730cbfa4473562d8ec59";
    const MARKED: &str = "cIVhlEBk_HMMv6RHNWLY7Fk";

    #[test]
    fn host_rules_match_tdesktop() {
        assert_eq!(
            normalize_host(" Proxy.Example.COM ").as_deref(),
            Some("proxy.example.com")
        );
        assert_eq!(
            normalize_host("bücher.example").as_deref(),
            Some("xn--bcher-kva.example")
        );
        assert_eq!(
            normalize_host("xn--strae-oqa.example").as_deref(),
            Some("xn--strae-oqa.example")
        );
        for bad in [
            "localhost",
            "127.0.0.1",
            "127.1",
            "0x7f.1",
            "0177.0.0.1",
            "1.2.3",
            "site.example:443",
            "site..example",
            "site.example.",
            "a@site.example",
            "",
        ] {
            assert_eq!(normalize_host(bad), None, "{bad}");
        }
    }

    #[test]
    fn base_path_rules_match_tdesktop() {
        assert_eq!(
            normalize_base_path(" /two/segments/ ").as_deref(),
            Some("two/segments")
        );
        assert_eq!(
            normalize_base_path("MixedCase").as_deref(),
            Some("MixedCase")
        );
        for bad in [
            "",
            "-lead",
            "with space",
            "empty//segment",
            "a/-lead",
            "dot.ted",
            "..",
            "per%20cent",
        ] {
            assert_eq!(normalize_base_path(bad), None, "{bad}");
        }
        assert_eq!(normalize_base_path(&"a".repeat(129)), None);
        assert!(normalize_base_path(&"a".repeat(128)).is_some());
    }

    #[test]
    fn address_parses_urls_and_paths() {
        assert_eq!(
            parse_address(" HTTPS://Proxy.Example.COM/App/ "),
            Some(("proxy.example.com".into(), "App".into()))
        );
        assert_eq!(
            parse_address("proxy.example.com"),
            Some(("proxy.example.com".into(), String::new()))
        );
        assert_eq!(
            parse_address("proxy.example.com/"),
            Some(("proxy.example.com".into(), String::new()))
        );
        assert_eq!(parse_address("http://proxy.example.com"), None);
        assert_eq!(parse_address("proxy.example.com/a//b"), None);
        assert_eq!(address("h.example", "a/b"), "h.example/a/b");
    }

    #[test]
    fn link_secret_round_trips() {
        assert_eq!(decode_link_secret(MARKED, true).as_deref(), Some(PLAIN));
        assert_eq!(decode_link_secret(MARKED, false).as_deref(), Some(PLAIN));
        assert_eq!(decode_link_secret(PLAIN, false).as_deref(), Some(PLAIN));
        assert_eq!(decode_link_secret(PLAIN, true), None);
        assert_eq!(encode_link_secret("app", PLAIN), MARKED);
        assert_eq!(encode_link_secret("", PLAIN), PLAIN);
    }

    #[test]
    fn row_state_follows_the_enabled_proxy() {
        assert_eq!(
            WebRowState::for_entry(false, true, true),
            WebRowState::NotTested
        );
        assert_eq!(
            WebRowState::for_entry(true, true, false),
            WebRowState::Online
        );
        assert_eq!(
            WebRowState::for_entry(true, false, false),
            WebRowState::Connecting
        );
        assert_eq!(
            WebRowState::for_entry(true, true, true),
            WebRowState::Unavailable
        );
    }

    use crate::calls::proxy::ProxyKind;
    use crate::proxy::{
        ProxyDraft, ProxyIssue, parse_added_proxy, parse_proxy_link, proxy_link_from_text,
    };

    fn web(address: &str, secret: &str) -> ProxyDraft {
        ProxyDraft::web(address, secret)
    }

    #[test]
    fn the_form_validates_like_tdesktop() {
        let ok = web("Proxy.Example.com/App", PLAIN).validated().unwrap();
        assert_eq!(ok.kind, ProxyKind::Web);
        assert_eq!(
            (ok.server.as_str(), ok.path.as_str()),
            ("proxy.example.com", "App")
        );
        assert_eq!(ok.port, 443);
        assert_eq!(ok.display_address(), "proxy.example.com/App");
        // Bad address, empty secret, short secret, fake-TLS secret.
        assert_eq!(
            web("127.0.0.1", PLAIN).validated(),
            Err(ProxyIssue::Invalid)
        );
        assert_eq!(
            web("a.example/x//y", PLAIN).validated(),
            Err(ProxyIssue::Invalid)
        );
        assert_eq!(web("a.example", "").validated(), Err(ProxyIssue::Invalid));
        assert_eq!(
            web("a.example", "abcd").validated(),
            Err(ProxyIssue::Invalid)
        );
        let fake_tls = crate::proxy::synthetic_fake_tls_secret();
        assert_eq!(
            web("a.example", &fake_tls).validated(),
            Err(ProxyIssue::Unsupported)
        );
        // A padded (dd) secret is fine and the port is not editable.
        let padded = format!("dd{PLAIN}");
        assert!(web("a.example", &padded).validated().is_ok());
        let mut wrong_port = web("a.example", PLAIN);
        wrong_port.port = 8443;
        assert_eq!(wrong_port.validated(), Err(ProxyIssue::Invalid));
    }

    #[test]
    fn proxy_type_web_serialises_and_parses() {
        let draft = web("a.example/app", PLAIN).validated().unwrap();
        let json = draft.to_json();
        assert_eq!(json["server"], "a.example");
        assert_eq!(json["port"], 443);
        assert_eq!(json["type"]["@type"], "proxyTypeWeb");
        assert_eq!(json["type"]["path"], "app");
        assert_eq!(json["type"]["secret"], PLAIN);
        let added = serde_json::json!({"@type": "addedProxy", "id": 5, "is_enabled": true,
            "last_used_date": 0, "comment": "", "proxy": json});
        let entry = parse_added_proxy(&added).unwrap();
        assert_eq!(entry.proxy, draft);
        assert_eq!(entry.proxy.kind_label(), "WEB");
    }

    #[test]
    fn web_links_parse_and_round_trip() {
        let root = web("a.example", PLAIN).validated().unwrap();
        let link = root.share_link(true).unwrap();
        assert_eq!(
            link,
            format!("tg://webproxy?server=a.example&secret={PLAIN}")
        );
        assert_eq!(parse_proxy_link(&link).unwrap().unwrap(), root);
        let public = root.share_link(false).unwrap();
        assert!(public.starts_with("https://t.me/webproxy?"));
        assert_eq!(parse_proxy_link(&public).unwrap().unwrap(), root);

        let nested = web("a.example/my-app", PLAIN).validated().unwrap();
        let link = nested.share_link(true).unwrap();
        assert!(link.starts_with("tg://webproxy?server=a.example%2Fmy-app&secret="));
        assert_eq!(parse_proxy_link(&link).unwrap().unwrap(), nested);
        assert!(proxy_link_from_text(&format!("proxy {link}")).is_some());
    }

    #[test]
    fn broken_web_links_are_rejected() {
        // `host` works as an alias of `server` (tdesktop).
        let host = format!("tg://webproxy?host=a.example&secret={PLAIN}");
        assert!(parse_proxy_link(&host).unwrap().is_ok());
        // An unmarked secret on a link with a base path is refused.
        let unmarked = format!("tg://webproxy?server=a.example%2Fapp&secret={PLAIN}");
        assert_eq!(
            parse_proxy_link(&unmarked).unwrap(),
            Err(ProxyIssue::Invalid)
        );
        for bad in [
            format!("tg://webproxy?server=1.2.3.4&secret={PLAIN}"),
            "tg://webproxy?server=a.example".to_string(),
            format!("tg://webproxy?server=http%3A%2F%2Fa.example&secret={PLAIN}"),
        ] {
            assert_eq!(
                parse_proxy_link(&bad).unwrap(),
                Err(ProxyIssue::Invalid),
                "{bad}"
            );
        }
    }

    #[test]
    fn web_proxies_are_told_apart_by_path_and_secret() {
        let a = web("a.example/x", PLAIN).validated().unwrap();
        let mut b = a.clone();
        assert!(a.same_endpoint(&b));
        b.path = "y".into();
        assert!(!a.same_endpoint(&b));
    }
}

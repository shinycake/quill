//! Proxy settings model (`parity:proxy-settings`): TDLib's proxy list
//! (`getProxies` / `addProxy` / `editProxy` / `enableProxy` /
//! `disableProxy` / `removeProxy` / `pingProxy`), `tg://proxy` /
//! `tg://socks` link parsing and sharing, tdesktop's MTProto secret
//! validation, and the auto-switch ("proxy rotation") state machine.
//!
//! TDLib stores the proxy list itself (it survives restarts and is usable
//! before authorization), so Quill persists only its two client-side
//! rotation preferences ([`ProxyPrefs`]). Everything here is pure: the
//! driver (`connect/proxy.rs`) sends requests, the reducer
//! (`state/session_proxy.rs`) applies answers.
//!
//! tdesktop references: `boxes/connection_box.cpp` (`ProxiesBox`,
//! `ProxyBox`, `ShowApplyConfirmation`, `ProxyDataToQueryPath`),
//! `mtproto/mtproto_proxy_data.cpp` (secret status),
//! `core/proxy_rotation_manager.cpp` (auto-switch).

use crate::calls::proxy::{EnabledProxy, ProxyKind};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// A proxy's connection data without TDLib's id: what the editor edits,
/// what a link carries, what `addProxy` / `pingProxy` take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyDraft {
    pub kind: ProxyKind,
    pub server: String,
    pub port: u16,
    /// SOCKS5 / HTTP login (may be empty).
    pub username: String,
    pub password: String,
    /// MTProto secret as TDLib wants it (hexadecimal).
    pub secret: String,
    /// HTTP only: the proxy cannot do `CONNECT` tunnelling.
    pub http_only: bool,
}

impl ProxyDraft {
    pub fn empty(kind: ProxyKind) -> Self {
        Self {
            kind,
            server: String::new(),
            port: 0,
            username: String::new(),
            password: String::new(),
            secret: String::new(),
            http_only: false,
        }
    }

    /// Identity for "already in the list": type, server, port and the
    /// credential that distinguishes proxies of that type (tdesktop's
    /// `ProxyData::operator==`).
    pub fn same_endpoint(&self, other: &ProxyDraft) -> bool {
        self.kind == other.kind
            && self.server.eq_ignore_ascii_case(&other.server)
            && self.port == other.port
            && match self.kind {
                ProxyKind::Mtproto => self.secret.eq_ignore_ascii_case(&other.secret),
                _ => self.username == other.username && self.password == other.password,
            }
    }

    /// Check the draft the way tdesktop's `ProxyData::status()` does and
    /// normalise the MTProto secret to hexadecimal (TDLib's encoding;
    /// links may carry base64url).
    pub fn validated(mut self) -> Result<ProxyDraft, ProxyIssue> {
        self.server = self.server.trim().to_string();
        if self.server.is_empty() || self.port == 0 {
            return Err(ProxyIssue::Invalid);
        }
        if self.kind == ProxyKind::Mtproto {
            let secret = self.secret.trim();
            match mtproto_secret_status(secret) {
                SecretStatus::Valid => {}
                SecretStatus::Invalid => return Err(ProxyIssue::Invalid),
                SecretStatus::Unsupported => return Err(ProxyIssue::Unsupported),
                SecretStatus::IncorrectSecret => return Err(ProxyIssue::IncorrectSecret),
            }
            self.secret = secret_to_hex(secret).ok_or(ProxyIssue::Invalid)?;
        }
        Ok(self)
    }

    /// The kind label shown in lists (`SOCKS5`, `HTTP`, `MTPROTO`).
    pub fn kind_label(&self) -> &'static str {
        kind_label(self.kind)
    }

    /// Shareable link, `None` for HTTP proxies (tdesktop's
    /// `ProxyDataIsShareable`). `scheme_tg` picks `tg://` over the public
    /// `https://t.me/` form.
    pub fn share_link(&self, scheme_tg: bool) -> Option<String> {
        let path = match self.kind {
            ProxyKind::Socks5 => "socks",
            ProxyKind::Mtproto => "proxy",
            ProxyKind::Http => return None,
        };
        let mut link = format!(
            "{}{path}?server={}&port={}",
            if scheme_tg { "tg://" } else { "https://t.me/" },
            percent_encode(&self.server),
            self.port
        );
        match self.kind {
            ProxyKind::Mtproto => {
                link.push_str("&secret=");
                link.push_str(&percent_encode(&self.secret));
            }
            _ => {
                if !self.username.is_empty() {
                    link.push_str("&user=");
                    link.push_str(&percent_encode(&self.username));
                }
                if !self.password.is_empty() {
                    link.push_str("&pass=");
                    link.push_str(&percent_encode(&self.password));
                }
            }
        }
        Some(link)
    }

    /// The TDLib `proxy` object.
    pub fn to_json(&self) -> Value {
        let type_value = match self.kind {
            ProxyKind::Socks5 => serde_json::json!({
                "@type": "proxyTypeSocks5",
                "username": self.username,
                "password": self.password,
            }),
            ProxyKind::Http => serde_json::json!({
                "@type": "proxyTypeHttp",
                "username": self.username,
                "password": self.password,
                "http_only": self.http_only,
            }),
            ProxyKind::Mtproto => serde_json::json!({
                "@type": "proxyTypeMtproto",
                "secret": self.secret,
            }),
        };
        serde_json::json!({
            "@type": "proxy",
            "server": self.server,
            "port": self.port,
            "type": type_value,
        })
    }
}

pub fn kind_label(kind: ProxyKind) -> &'static str {
    match kind {
        ProxyKind::Socks5 => "SOCKS5",
        ProxyKind::Http => "HTTP",
        ProxyKind::Mtproto => "MTPROTO",
    }
}

/// Why a proxy link or editor draft cannot be used; the wording is
/// tdesktop's (`lng_proxy_invalid`, `lng_proxy_unsupported`,
/// `lng_proxy_incorrect_secret`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyIssue {
    Invalid,
    Unsupported,
    IncorrectSecret,
}

impl ProxyIssue {
    pub fn message(self) -> &'static str {
        match self {
            ProxyIssue::Invalid => "The proxy link is invalid.",
            ProxyIssue::Unsupported => {
                "Quill doesn't support this proxy type or the proxy link is invalid."
            }
            ProxyIssue::IncorrectSecret => {
                "This proxy link uses an invalid secret. Please contact the proxy provider and ask them to update the MTProxy source code and configure it with a correct secret value, then get a new link."
            }
        }
    }
}

/// tdesktop `ProxyData::Status` for an MTProto secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretStatus {
    Valid,
    Invalid,
    Unsupported,
    IncorrectSecret,
}

fn is_hex_secret(secret: &str) -> bool {
    secret.len() >= 32
        && secret.len().is_multiple_of(2)
        && secret.bytes().all(|b| b.is_ascii_hexdigit())
}

fn base64url_inner(secret: &str) -> &str {
    secret.trim_end_matches('=')
}

fn is_base64url_secret(secret: &str) -> bool {
    if secret.len() < 22 || secret.len() % 4 == 1 {
        return false;
    }
    base64url_inner(secret)
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// tdesktop's `MtprotoPasswordStatus`: a 16-byte secret, a `dd`-prefixed
/// 17-byte (padded intermediate) one, or an `ee`-prefixed 21+ byte (fake
/// TLS) one, in hex or base64url.
pub fn mtproto_secret_status(secret: &str) -> SecretStatus {
    if is_hex_secret(secret) {
        let size = secret.len() / 2;
        let lower = secret.to_ascii_lowercase();
        let (t1, t2) = (lower.as_bytes()[0], lower.as_bytes()[1]);
        let valid = size == 16
            || (size == 17 && t1 == b'd' && t2 == b'd')
            || (size >= 21 && t1 == b'e' && t2 == b'e');
        if valid {
            SecretStatus::Valid
        } else if size < 16 {
            SecretStatus::Invalid
        } else {
            SecretStatus::Unsupported
        }
    } else if is_base64url_secret(secret) {
        let inner = base64url_inner(secret);
        let size = inner.len() * 3 / 4;
        let b = secret.as_bytes();
        let valid = size == 16
            || (size == 17 && b[0] == b'3' && (matches!(b[1], b'Q'..=b'Z' | b'a'..=b'f')))
            || (size >= 21 && b[0] == b'7' && matches!(b[1], b'g'..=b'v'));
        let incorrect =
            size >= 21 && b[0].eq_ignore_ascii_case(&b'e') && b[1].eq_ignore_ascii_case(&b'e');
        if size < 16 {
            SecretStatus::Invalid
        } else if valid {
            SecretStatus::Valid
        } else if incorrect {
            SecretStatus::IncorrectSecret
        } else {
            SecretStatus::Unsupported
        }
    } else {
        SecretStatus::Invalid
    }
}

/// Hex form of a (hex or base64url) MTProto secret.
fn secret_to_hex(secret: &str) -> Option<String> {
    if is_hex_secret(secret) {
        return Some(secret.to_ascii_lowercase());
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(base64url_inner(secret))
        .ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Unreserved characters stay; everything else is `%XX` (UTF-8 bytes).
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A made-up fake-TLS MTProto secret (`ee` + 16 filler bytes + the hex of
/// `www.google.com`) for the screenshot demo and tests. Assembled at
/// runtime so no secret-looking literal sits in the source.
pub fn synthetic_fake_tls_secret() -> String {
    let domain: String = "www.google.com"
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("ee{}{domain}", "ab".repeat(16))
}

/// One entry of TDLib's proxy list (`addedProxy`, schema 1.8.67 :10118).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyEntry {
    pub id: i32,
    pub last_used_date: i32,
    pub is_enabled: bool,
    pub comment: String,
    pub proxy: ProxyDraft,
}

impl ProxyEntry {
    /// The shape the call engine takes (`calls::proxy::proxy_for_calls`).
    pub fn as_enabled(&self) -> EnabledProxy {
        EnabledProxy {
            kind: self.proxy.kind,
            host: self.proxy.server.clone(),
            port: self.proxy.port,
            username: self.proxy.username.clone(),
            password: self.proxy.password.clone(),
        }
    }
}

fn json_str(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Parse one `addedProxy`. Entries with an unknown proxy type or without
/// an id are skipped by the caller (`None`).
pub fn parse_added_proxy(value: &Value) -> Option<ProxyEntry> {
    let id = i32::try_from(value.get("id")?.as_i64()?).ok()?;
    let proxy = value.get("proxy")?;
    let kind_value = proxy.get("type")?;
    let kind = match kind_value.get("@type")?.as_str()? {
        "proxyTypeSocks5" => ProxyKind::Socks5,
        "proxyTypeHttp" => ProxyKind::Http,
        "proxyTypeMtproto" => ProxyKind::Mtproto,
        _ => return None,
    };
    Some(ProxyEntry {
        id,
        last_used_date: value
            .get("last_used_date")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        is_enabled: value
            .get("is_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        comment: json_str(value.get("comment")),
        proxy: ProxyDraft {
            kind,
            server: json_str(proxy.get("server")),
            port: proxy
                .get("port")
                .and_then(Value::as_i64)
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(0),
            username: json_str(kind_value.get("username")),
            password: json_str(kind_value.get("password")),
            secret: json_str(kind_value.get("secret")),
            http_only: kind_value
                .get("http_only")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
    })
}

/// Parse an `addedProxies` answer.
pub fn parse_added_proxies(value: &Value) -> Vec<ProxyEntry> {
    value
        .get("proxies")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(parse_added_proxy).collect())
        .unwrap_or_default()
}

/// Whether a link is a proxy link, and if so what it carries.
/// `Some(Err(..))` is a proxy link that cannot be used (tdesktop shows
/// an inform box with the issue); `None` is any other link.
///
/// Accepted forms (tdesktop `local_url_handlers.cpp`): `tg://proxy?…`,
/// `tg://socks?…`, `https://t.me/proxy?…`, `https://t.me/socks?…`
/// (also `telegram.me` / `telegram.dog`). Parameter names are
/// case-insensitive; the MTProto secret has `+` → `-` and `/` → `_`
/// applied (base64 → base64url).
pub fn parse_proxy_link(url: &str) -> Option<Result<ProxyDraft, ProxyIssue>> {
    let url = url.trim();
    let url = url.split('#').next().unwrap_or(url);
    let lower = url.to_ascii_lowercase();
    let command = if let Some(rest) = lower.strip_prefix("tg://") {
        rest.split(['?', '/']).next().unwrap_or("").to_string()
    } else {
        let after = lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"))?;
        let (host, tail) = after.split_once('/')?;
        if !["t.me", "telegram.me", "telegram.dog"].contains(&host) {
            return None;
        }
        tail.split(['?', '/']).next().unwrap_or("").to_string()
    };
    let kind = match command.as_str() {
        "proxy" => ProxyKind::Mtproto,
        "socks" => ProxyKind::Socks5,
        _ => return None,
    };
    let params: Vec<(String, String)> = crate::connect::tg_query_params(url)
        .into_iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .collect();
    let param = |name: &str| {
        params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let mut draft = ProxyDraft::empty(kind);
    draft.server = param("server");
    draft.port = param("port").parse().unwrap_or(0);
    if kind == ProxyKind::Socks5 {
        draft.username = param("user");
        draft.password = param("pass");
    } else {
        draft.secret = param("secret").replace('+', "-").replace('/', "_");
    }
    Some(draft.validated())
}

/// First proxy link in free text (clipboard "Add proxy from clipboard"):
/// tdesktop only accepts text that holds exactly one link candidate.
pub fn proxy_link_from_text(text: &str) -> Option<Result<ProxyDraft, ProxyIssue>> {
    let mut candidates = text
        .split_whitespace()
        .filter(|t| t.starts_with("tg:") || t.starts_with("http://") || t.starts_with("https://"));
    let only = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    parse_proxy_link(only)
}

/// Availability of one proxy (`pingProxy`, tdesktop's row status).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PingStatus {
    /// Ping request in flight ("checking…").
    Checking,
    /// Answered after this many milliseconds.
    Available(u32),
    /// The ping failed ("not available").
    Unavailable,
}

impl PingStatus {
    /// Row status text, tdesktop's `lng_proxy_*` wording.
    pub fn label(self) -> String {
        match self {
            PingStatus::Checking => "checking…".into(),
            PingStatus::Available(ms) => format!("available (ping: {ms} ms)"),
            PingStatus::Unavailable => "not available".into(),
        }
    }
}

/// Client-side proxy preferences (tdesktop `SettingsProxy`): the
/// auto-switch toggle and its timeout. Persisted as `proxy_prefs.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyPrefs {
    pub auto_switch: bool,
    pub auto_switch_secs: u32,
}

/// tdesktop `kProxyRotationTimeouts`.
pub const AUTO_SWITCH_TIMEOUTS: [u32; 5] = [5, 10, 15, 30, 60];
pub const DEFAULT_AUTO_SWITCH_SECS: u32 = 10;

impl Default for ProxyPrefs {
    fn default() -> Self {
        Self {
            auto_switch: false,
            auto_switch_secs: DEFAULT_AUTO_SWITCH_SECS,
        }
    }
}

impl ProxyPrefs {
    /// The stored timeout snapped to the offered choices.
    pub fn timeout_secs(&self) -> u32 {
        *AUTO_SWITCH_TIMEOUTS
            .iter()
            .min_by_key(|t| t.abs_diff(self.auto_switch_secs))
            .unwrap_or(&DEFAULT_AUTO_SWITCH_SECS)
    }
}

/// What the auto-switch machine wants done next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationAction {
    None,
    /// Ping this proxy to see whether it is a usable replacement.
    Ping(i32),
    /// Enable this proxy (it answered a ping).
    Switch(i32),
}

/// Everything the proxy UI and driver cache (`Session::proxy`).
#[derive(Debug, Clone, Default)]
pub struct ProxyState {
    /// `getProxies` answer; `None` until the first one arrives.
    pub list: Option<Vec<ProxyEntry>>,
    pub loading: bool,
    /// The list changed under us (an add / edit / enable / remove
    /// succeeded): refetch the authoritative answer.
    pub stale: bool,
    /// A mutation is in flight.
    pub mutating: bool,
    pub error: Option<String>,
    /// Per-proxy-id ping results.
    pub pings: HashMap<i32, PingStatus>,
    /// Option `prefer_ipv6` (tdesktop "Try connecting through IPv6").
    pub prefer_ipv6: bool,
    pub prefs: ProxyPrefs,
    /// First `getProxies` was sent (so a failed one is not retried in a
    /// tight loop).
    pub fetch_attempted: bool,
    /// Auto-switch: when the connection last stopped being usable (ms).
    pub disconnected_since_ms: Option<u64>,
}

impl ProxyState {
    pub fn entries(&self) -> &[ProxyEntry] {
        self.list.as_deref().unwrap_or(&[])
    }

    /// The enabled proxy, if any.
    pub fn enabled(&self) -> Option<&ProxyEntry> {
        self.entries().iter().find(|p| p.is_enabled)
    }

    pub fn find(&self, id: i32) -> Option<&ProxyEntry> {
        self.entries().iter().find(|p| p.id == id)
    }

    /// Whether the call-media toggle applies: only a SOCKS5 proxy can
    /// carry call media (`calls::proxy`).
    pub fn enabled_supports_calls(&self) -> bool {
        self.enabled()
            .is_some_and(|p| p.proxy.kind == ProxyKind::Socks5)
    }

    /// The proxy to route call media through, given the user's toggle.
    pub fn call_proxy(&self, use_for_calls: bool) -> Option<crate::calls::proxy::CallProxy> {
        let enabled = self.enabled().map(ProxyEntry::as_enabled);
        crate::calls::proxy::proxy_for_calls(use_for_calls, enabled.as_ref())
    }

    /// Whether the connection strip should show the proxy shield: a proxy
    /// is enabled.
    pub fn shield(&self) -> bool {
        self.enabled().is_some()
    }

    /// Auto-switch step (tdesktop `ProxyRotationManager`): once the
    /// connection has been unusable for the configured timeout, probe the
    /// other proxies one at a time and enable the first that answers.
    ///
    /// `connected` is whether TDLib reports a usable connection. Pure:
    /// the caller sends the requested ping / enable and feeds answers
    /// back through `pings`.
    pub fn rotation_step(&mut self, now_ms: u64, connected: bool) -> RotationAction {
        let entries = self.entries();
        let observing = self.prefs.auto_switch && entries.len() > 1 && self.enabled().is_some();
        if !observing || connected {
            self.disconnected_since_ms = None;
            return RotationAction::None;
        }
        let since = *self.disconnected_since_ms.get_or_insert(now_ms);
        if now_ms.saturating_sub(since) < u64::from(self.prefs.timeout_secs()) * 1000 {
            return RotationAction::None;
        }
        if self
            .pings
            .values()
            .any(|p| matches!(p, PingStatus::Checking))
        {
            return RotationAction::None;
        }
        let entries = self.entries();
        let current = entries.iter().position(|p| p.is_enabled).unwrap_or(0);
        // Candidates in list order, starting after the current proxy.
        let order: Vec<&ProxyEntry> = (1..entries.len())
            .map(|step| &entries[(current + step) % entries.len()])
            .collect();
        if let Some(found) = order
            .iter()
            .find(|p| matches!(self.pings.get(&p.id), Some(PingStatus::Available(_))))
        {
            let id = found.id;
            self.disconnected_since_ms = Some(now_ms);
            self.pings.remove(&id);
            return RotationAction::Switch(id);
        }
        if let Some(next) = order.iter().find(|p| !self.pings.contains_key(&p.id)) {
            return RotationAction::Ping(next.id);
        }
        // Every alternative failed: forget the verdicts and look again
        // after another timeout.
        let ids: Vec<i32> = order.iter().map(|p| p.id).collect();
        for id in ids {
            self.pings.remove(&id);
        }
        self.disconnected_since_ms = Some(now_ms);
        RotationAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_secret() -> String {
        synthetic_fake_tls_secret()
    }

    fn entry(id: i32, enabled: bool) -> ProxyEntry {
        ProxyEntry {
            id,
            last_used_date: 0,
            is_enabled: enabled,
            comment: String::new(),
            proxy: ProxyDraft {
                server: format!("p{id}.example"),
                port: 1080,
                ..ProxyDraft::empty(ProxyKind::Socks5)
            },
        }
    }

    fn state(list: Vec<ProxyEntry>, auto: bool) -> ProxyState {
        ProxyState {
            list: Some(list),
            prefs: ProxyPrefs {
                auto_switch: auto,
                auto_switch_secs: 10,
            },
            ..ProxyState::default()
        }
    }

    #[test]
    fn socks_link_parses_credentials() {
        let draft =
            parse_proxy_link("tg://socks?server=proxy.example&port=1080&user=al%20ice&pass=s3")
                .unwrap()
                .unwrap();
        assert_eq!(draft.kind, ProxyKind::Socks5);
        assert_eq!(draft.server, "proxy.example");
        assert_eq!(draft.port, 1080);
        assert_eq!(draft.username, "al ice");
        assert_eq!(draft.password, "s3");
    }

    #[test]
    fn mtproto_web_link_parses_and_normalises_secret() {
        let url = format!(
            "https://t.me/proxy?server=1.2.3.4&port=443&secret={}",
            hex_secret()
        );
        let draft = parse_proxy_link(&url).unwrap().unwrap();
        assert_eq!(draft.kind, ProxyKind::Mtproto);
        assert_eq!(draft.secret, hex_secret());
        // Param names are case-insensitive, other hosts are not proxies.
        assert!(parse_proxy_link(&url.replace("server", "SERVER")).is_some());
        assert!(parse_proxy_link("https://example.com/proxy?server=a&port=1").is_none());
        assert!(parse_proxy_link("tg://resolve?domain=durov").is_none());
    }

    #[test]
    fn base64url_secret_converts_to_hex() {
        // 16 raw bytes 00..0f.
        let raw: Vec<u8> = (0u8..16).collect();
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&raw);
        let url = format!("tg://proxy?server=a.b&port=443&secret={b64}");
        let draft = parse_proxy_link(&url).unwrap().unwrap();
        assert_eq!(draft.secret, "000102030405060708090a0b0c0d0e0f");
    }

    #[test]
    fn broken_proxy_links_report_the_tdesktop_issue() {
        assert_eq!(
            parse_proxy_link("tg://proxy?server=a.b&port=0&secret=00").unwrap(),
            Err(ProxyIssue::Invalid)
        );
        assert_eq!(
            parse_proxy_link("tg://socks?port=1080").unwrap(),
            Err(ProxyIssue::Invalid)
        );
        // 17 bytes without the dd prefix: well-formed but unsupported.
        let odd = "11".repeat(17);
        assert_eq!(
            parse_proxy_link(&format!("tg://proxy?server=a.b&port=443&secret={odd}")).unwrap(),
            Err(ProxyIssue::Unsupported)
        );
    }

    #[test]
    fn secret_status_matches_tdesktop() {
        assert_eq!(mtproto_secret_status(&"ab".repeat(16)), SecretStatus::Valid);
        assert_eq!(
            mtproto_secret_status(&format!("dd{}", "ab".repeat(16))),
            SecretStatus::Valid
        );
        assert_eq!(mtproto_secret_status(&hex_secret()), SecretStatus::Valid);
        assert_eq!(mtproto_secret_status("zz"), SecretStatus::Invalid);
        assert_eq!(mtproto_secret_status(""), SecretStatus::Invalid);
    }

    #[test]
    fn share_links_round_trip_and_http_is_not_shareable() {
        let mut draft = ProxyDraft::empty(ProxyKind::Socks5);
        draft.server = "proxy.example".into();
        draft.port = 1080;
        draft.username = "a b".into();
        draft.password = "p&q".into();
        let link = draft.share_link(false).unwrap();
        assert_eq!(
            link,
            "https://t.me/socks?server=proxy.example&port=1080&user=a%20b&pass=p%26q"
        );
        assert_eq!(parse_proxy_link(&link).unwrap().unwrap(), draft);
        assert!(draft.share_link(true).unwrap().starts_with("tg://socks?"));
        draft.kind = ProxyKind::Http;
        assert_eq!(draft.share_link(false), None);
    }

    #[test]
    fn clipboard_text_needs_exactly_one_link() {
        let one = "see tg://socks?server=a.b&port=1 please";
        assert!(proxy_link_from_text(one).is_some());
        let two = "tg://socks?server=a.b&port=1 tg://socks?server=c.d&port=2";
        assert!(proxy_link_from_text(two).is_none());
        assert!(proxy_link_from_text("hello").is_none());
    }

    #[test]
    fn added_proxies_answer_parses_all_three_types() {
        let json: Value = serde_json::from_str(
            r#"{"@type":"addedProxies","proxies":[
              {"@type":"addedProxy","id":1,"last_used_date":1700000000,"is_enabled":true,"comment":"home",
               "proxy":{"@type":"proxy","server":"s5.example","port":1080,"type":{"@type":"proxyTypeSocks5","username":"u","password":"p"}}},
              {"@type":"addedProxy","id":2,"last_used_date":0,"is_enabled":false,"comment":"",
               "proxy":{"@type":"proxy","server":"h.example","port":8080,"type":{"@type":"proxyTypeHttp","username":"","password":"","http_only":true}}},
              {"@type":"addedProxy","id":3,"last_used_date":0,"is_enabled":false,"comment":"",
               "proxy":{"@type":"proxy","server":"m.example","port":443,"type":{"@type":"proxyTypeMtproto","secret":"abcd"}}},
              {"@type":"addedProxy","id":4,"proxy":{"@type":"proxy","server":"x","port":1,"type":{"@type":"proxyTypeFuture"}}}
            ]}"#,
        )
        .unwrap();
        let list = parse_added_proxies(&json);
        assert_eq!(list.len(), 3);
        assert!(list[0].is_enabled && list[0].proxy.username == "u" && list[0].comment == "home");
        assert!(list[1].proxy.http_only && list[1].proxy.kind == ProxyKind::Http);
        assert_eq!(list[2].proxy.secret, "abcd");
    }

    #[test]
    fn proxy_json_matches_the_schema() {
        let mut draft = ProxyDraft::empty(ProxyKind::Mtproto);
        draft.server = "m.example".into();
        draft.port = 443;
        draft.secret = hex_secret();
        let json = draft.to_json();
        assert_eq!(json["@type"], "proxy");
        assert_eq!(json["port"], 443);
        assert_eq!(json["type"]["@type"], "proxyTypeMtproto");
        assert_eq!(json["type"]["secret"], hex_secret());
    }

    #[test]
    fn rotation_waits_for_the_timeout_then_probes_then_switches() {
        let mut s = state(vec![entry(1, true), entry(2, false), entry(3, false)], true);
        assert_eq!(s.rotation_step(0, false), RotationAction::None);
        assert_eq!(s.rotation_step(9_999, false), RotationAction::None);
        // Timeout reached: probe the proxy after the current one.
        assert_eq!(s.rotation_step(10_000, false), RotationAction::Ping(2));
        s.pings.insert(2, PingStatus::Checking);
        assert_eq!(s.rotation_step(10_500, false), RotationAction::None);
        s.pings.insert(2, PingStatus::Unavailable);
        assert_eq!(s.rotation_step(11_000, false), RotationAction::Ping(3));
        s.pings.insert(3, PingStatus::Available(80));
        assert_eq!(s.rotation_step(12_000, false), RotationAction::Switch(3));
    }

    #[test]
    fn rotation_resets_when_connected_or_disabled() {
        let mut s = state(vec![entry(1, true), entry(2, false)], true);
        s.rotation_step(0, false);
        assert_eq!(s.rotation_step(20_000, true), RotationAction::None);
        assert_eq!(s.disconnected_since_ms, None);
        // The clock restarts from the next disconnect.
        assert_eq!(s.rotation_step(21_000, false), RotationAction::None);
        s.prefs.auto_switch = false;
        assert_eq!(s.rotation_step(99_000, false), RotationAction::None);
        // One proxy only: nothing to rotate to.
        let mut single = state(vec![entry(1, true)], true);
        single.rotation_step(0, false);
        assert_eq!(single.rotation_step(99_000, false), RotationAction::None);
    }

    #[test]
    fn rotation_retries_after_every_alternative_failed() {
        let mut s = state(vec![entry(1, true), entry(2, false)], true);
        s.rotation_step(0, false);
        assert_eq!(s.rotation_step(10_000, false), RotationAction::Ping(2));
        s.pings.insert(2, PingStatus::Unavailable);
        assert_eq!(s.rotation_step(10_500, false), RotationAction::None);
        assert!(s.pings.is_empty());
        assert_eq!(s.rotation_step(20_500, false), RotationAction::Ping(2));
    }

    #[test]
    fn prefs_timeout_snaps_to_the_offered_choices() {
        let prefs = ProxyPrefs {
            auto_switch: true,
            auto_switch_secs: 0,
        };
        assert_eq!(prefs.timeout_secs(), 5);
        assert_eq!(ProxyPrefs::default().timeout_secs(), 10);
        let json: ProxyPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(json, ProxyPrefs::default());
    }

    #[test]
    fn call_proxy_uses_only_an_enabled_socks5_proxy() {
        let s = state(vec![entry(1, true)], false);
        assert!(s.enabled_supports_calls());
        assert!(s.call_proxy(true).is_some());
        assert!(s.call_proxy(false).is_none());
        let mut mt = entry(2, true);
        mt.proxy.kind = ProxyKind::Mtproto;
        let s = state(vec![mt], false);
        assert!(!s.enabled_supports_calls());
        assert!(s.call_proxy(true).is_none());
    }

    #[test]
    fn same_endpoint_ignores_host_case_and_id() {
        let a = entry(1, false).proxy;
        let mut b = entry(2, false).proxy;
        b.server = "P1.EXAMPLE".into();
        assert!(a.same_endpoint(&b));
        b.port = 1;
        assert!(!a.same_endpoint(&b));
    }
}

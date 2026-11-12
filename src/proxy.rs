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
#![deny(clippy::indexing_slicing)]

use crate::calls::proxy::{EnabledProxy, ProxyKind};
use crate::ids::RequestId;
pub use crate::proxy_ops::{FAILED_RETRY_AFTER, FAILED_RETRY_MAX, ProxyFetch, ProxyListOps};
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
    /// Web proxies only: the base path on the server (empty for the
    /// root). `server` holds the host and `port` is always 443.
    pub path: String,
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
            path: String::new(),
            http_only: false,
        }
    }

    /// A web proxy from the form's address field (host with an optional
    /// base path, tdesktop `setWebAddress`) and secret. An address that
    /// does not parse leaves `server` empty, which `validated` rejects.
    pub fn web(address: &str, secret: &str) -> Self {
        let (server, path) = crate::proxy_web::parse_address(address).unwrap_or_default();
        Self {
            server,
            path,
            port: crate::proxy_web::WEB_PROXY_PORT,
            secret: secret.to_string(),
            ..Self::empty(ProxyKind::Web)
        }
    }

    /// `host` or `host/path` for a web proxy, the host otherwise.
    pub fn display_address(&self) -> String {
        match self.kind {
            ProxyKind::Web => crate::proxy_web::address(&self.server, &self.path),
            _ => self.server.clone(),
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
                ProxyKind::Web => {
                    self.path == other.path && self.secret.eq_ignore_ascii_case(&other.secret)
                }
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
        let web = self.kind == ProxyKind::Web;
        if web
            && (self.port != crate::proxy_web::WEB_PROXY_PORT
                || crate::proxy_web::normalize_host(&self.server).as_deref()
                    != Some(self.server.as_str())
                || (!self.path.is_empty()
                    && crate::proxy_web::normalize_base_path(&self.path).as_deref()
                        != Some(self.path.as_str())))
        {
            return Err(ProxyIssue::Invalid);
        }
        if self.kind == ProxyKind::Mtproto || web {
            let secret = self.secret.trim();
            match mtproto_secret_status(secret) {
                SecretStatus::Valid => {}
                SecretStatus::Invalid => return Err(ProxyIssue::Invalid),
                SecretStatus::Unsupported => return Err(ProxyIssue::Unsupported),
                SecretStatus::IncorrectSecret => return Err(ProxyIssue::IncorrectSecret),
            }
            self.secret = secret_to_hex(secret).ok_or(ProxyIssue::Invalid)?;
            // A web proxy has no fake-TLS mode (tdesktop `status()`).
            if web && self.secret.starts_with("ee") && self.secret.len() >= 42 {
                return Err(ProxyIssue::Unsupported);
            }
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
            ProxyKind::Web => "webproxy",
            ProxyKind::Http => return None,
        };
        let scheme = if scheme_tg { "tg://" } else { "https://t.me/" };
        if self.kind == ProxyKind::Web {
            return Some(format!(
                "{scheme}{path}?server={}&secret={}",
                percent_encode(&self.display_address()),
                percent_encode(&crate::proxy_web::encode_link_secret(
                    &self.path,
                    &self.secret
                ))
            ));
        }
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
            ProxyKind::Web => serde_json::json!({
                "@type": "proxyTypeWeb",
                "path": self.path,
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
        ProxyKind::Web => "WEB",
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
        let [t1, t2, ..] = lower.as_bytes() else {
            return SecretStatus::Invalid;
        };
        let (t1, t2) = (*t1, *t2);
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
        let [b0, b1, ..] = secret.as_bytes() else {
            return SecretStatus::Invalid;
        };
        let valid = size == 16
            || (size == 17 && *b0 == b'3' && (matches!(*b1, b'Q'..=b'Z' | b'a'..=b'f')))
            || (size >= 21 && *b0 == b'7' && matches!(*b1, b'g'..=b'v'));
        let incorrect =
            size >= 21 && b0.eq_ignore_ascii_case(&b'e') && b1.eq_ignore_ascii_case(&b'e');
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
        "proxyTypeWeb" => ProxyKind::Web,
        _ => return None,
    };
    Some(ProxyEntry {
        id,
        last_used_date: value
            .get("last_used_date")
            .and_then(Value::as_i64)
            .and_then(|date| i32::try_from(date).ok())
            .unwrap_or(0),
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
            path: json_str(kind_value.get("path")),
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
        "webproxy" => ProxyKind::Web,
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
    if kind == ProxyKind::Web {
        // tdesktop `ProxyDataFromFields`: `server` (or `host`) is the
        // address; the secret may carry the base-path marker.
        let address = match param("server") {
            server if server.is_empty() => param("host"),
            server => server,
        };
        let mut draft = ProxyDraft::web(&address, "");
        let secret = param("secret").replace('+', "-").replace('/', "_");
        draft.secret = crate::proxy_web::decode_link_secret(&secret, !draft.path.is_empty())
            .unwrap_or_default();
        return Some(draft.validated());
    }
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
    /// This ping request is in flight ("checking…"); only its answer
    /// settles the proxy.
    Checking(RequestId),
    /// Answered after this many milliseconds.
    Available(u32),
    /// The ping failed ("not available").
    Unavailable,
}

impl PingStatus {
    /// Row status text, tdesktop's `lng_proxy_*` wording.
    pub fn label(self) -> String {
        match self {
            PingStatus::Checking(_) => "checking…".into(),
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
    /// "Use system proxy settings" while no custom proxy is enabled
    /// (`crate::system_proxy`). tdesktop's default mode, so on unless
    /// "Disable proxy" was picked.
    pub use_system: bool,
}

/// tdesktop `kProxyRotationTimeouts`.
pub const AUTO_SWITCH_TIMEOUTS: [u32; 5] = [5, 10, 15, 30, 60];
pub const DEFAULT_AUTO_SWITCH_SECS: u32 = 10;

impl Default for ProxyPrefs {
    fn default() -> Self {
        Self {
            auto_switch: false,
            auto_switch_secs: DEFAULT_AUTO_SWITCH_SECS,
            use_system: true,
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

/// tdesktop's `ProxyData::Settings`, in the order of the box's radios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    Disabled,
    System,
    Custom,
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

/// Where the "Use system proxy settings" step sent last stands. Its answer
/// is matched by request id, so it is never read off [`ProxyState::error`],
/// which belongs to what the user did in the proxy box.
#[derive(Debug, Clone, Default)]
pub enum SystemStepState {
    /// No step has gone out (or the OS now wants nothing).
    #[default]
    Idle,
    /// Sent as this request, not answered.
    InFlight {
        step: crate::system_proxy::SystemProxyStep,
        at: std::time::Instant,
        request: crate::ids::RequestId,
    },
    /// TDLib accepted it.
    Accepted {
        step: crate::system_proxy::SystemProxyStep,
        at: std::time::Instant,
    },
    /// TDLib answered with an error, or the step never left: it waits
    /// `system_proxy::RETRY` before it is sent again.
    Refused {
        step: crate::system_proxy::SystemProxyStep,
        at: std::time::Instant,
    },
}

impl SystemStepState {
    /// The answer for `request`, when it is the step in flight. Returns
    /// whether it was; any other request (the user's own change) leaves
    /// the stage alone.
    pub fn answered(&mut self, request: crate::ids::RequestId, refused: bool) -> bool {
        match std::mem::take(self) {
            SystemStepState::InFlight {
                step,
                at,
                request: sent,
            } if sent == request => {
                *self = if refused {
                    SystemStepState::Refused { step, at }
                } else {
                    SystemStepState::Accepted { step, at }
                };
                true
            }
            other => {
                *self = other;
                false
            }
        }
    }
}

/// The proxy-link box: closed, or open on one proxy's draft at one of its
/// stages. The draft and the check are one value, so closing the box, a new
/// link or Connect is a single transition, and a check still in flight no
/// longer matches anything (only its own request settles `Checking`).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum LinkBox {
    /// No box.
    #[default]
    Closed,
    /// The proxy's details, with Check Status and Connect Proxy.
    Review(ProxyDraft),
    /// The one-time "this exposes your IP" warning before a check.
    Warn(ProxyDraft),
    /// `pingProxy` sent; this request's answer settles it.
    Checking {
        draft: ProxyDraft,
        request: crate::ids::RequestId,
    },
    /// The ping answered after this many milliseconds.
    Available { draft: ProxyDraft, ms: u32 },
    /// The ping failed.
    Unavailable(ProxyDraft),
}

impl LinkBox {
    pub fn is_open(&self) -> bool {
        *self != Self::Closed
    }

    /// The proxy the open box is about.
    pub fn draft(&self) -> Option<&ProxyDraft> {
        match self {
            Self::Closed => None,
            Self::Review(draft)
            | Self::Warn(draft)
            | Self::Checking { draft, .. }
            | Self::Available { draft, .. }
            | Self::Unavailable(draft) => Some(draft),
        }
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Warn(_))
    }

    /// The proxy status the box shows; `None` before a check.
    pub fn status(&self) -> Option<PingStatus> {
        match self {
            Self::Closed | Self::Review(_) | Self::Warn(_) => None,
            Self::Checking { request, .. } => Some(PingStatus::Checking(*request)),
            Self::Available { ms, .. } => Some(PingStatus::Available(*ms)),
            Self::Unavailable(_) => Some(PingStatus::Unavailable),
        }
    }
}

/// Where the auto-switch machine is (tdesktop `ProxyRotationManager`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rotation {
    /// The connection is usable (or nothing is rotated): no clock runs.
    #[default]
    Watching,
    /// The connection has been unusable since `since_ms`.
    Disconnected { since_ms: u64 },
    /// The timeout passed and the ping of proxy `id`, sent as `request` at
    /// `asked_ms`, is the probe being waited for. Only that request holds the
    /// rotation (the edit box's link test, or any other ping, does not), and
    /// only until [`PROBE_TIMEOUT_MS`]: a ping whose answer never comes
    /// counts as failed. The rotation leaves `Disconnected` for this stage
    /// when the send went out ([`ProxyState::probe_sent`]), never before.
    Probing {
        since_ms: u64,
        id: i32,
        request: RequestId,
        asked_ms: u64,
    },
}

/// How long the rotation waits for a probe's answer.
pub const PROBE_TIMEOUT_MS: u64 = 15_000;

/// Everything the proxy UI and driver cache (`Session::proxy`).
#[derive(Debug, Clone, Default)]
pub struct ProxyState {
    /// `getProxies` answer without the system proxy's entries; `None`
    /// until the first one arrives.
    pub list: Option<Vec<ProxyEntry>>,
    /// The system proxy's entries (`crate::system_proxy`), oldest first:
    /// never listed, shared or rotated.
    pub system: Vec<ProxyEntry>,
    /// The `getProxies` the list waits for and the change in flight (one
    /// value: [`ProxyListOps`]). Only the fetch's own answer lands; a
    /// change's success drops it (its answer may predate the change).
    pub ops: ProxyListOps,
    /// The box's error line: what the user's own action (a fetch, a change,
    /// the IPv6 choice) last failed with.
    pub error: Option<String>,
    /// The system proxy step's own outcome (not `error`).
    pub system_step: SystemStepState,
    /// Per-proxy-id ping results.
    pub pings: HashMap<i32, PingStatus>,
    /// The proxy-link box, with the draft it is about (its proxy is not in
    /// the list).
    pub link: LinkBox,
    /// Option `prefer_ipv6` (tdesktop "Try connecting through IPv6").
    pub prefer_ipv6: bool,
    pub prefs: ProxyPrefs,
    /// Auto-switch.
    pub rotation: Rotation,
    /// The web proxy carriers TDLib opened and what the driver still has
    /// to do about them (`crate::proxy_carrier`).
    pub carriers: crate::proxy_carrier::Carriers,
}

impl ProxyState {
    /// A link box opens on `draft`: a box or a check before it is gone, and
    /// its late answer matches nothing.
    pub fn open_link(&mut self, draft: ProxyDraft) {
        self.link = LinkBox::Review(draft);
    }

    /// The box closes (cancel, or the owner going away).
    pub fn close_link(&mut self) {
        self.link = LinkBox::Closed;
    }

    /// Connect: the box closes and hands over the proxy it showed.
    pub fn take_link(&mut self) -> Option<ProxyDraft> {
        match std::mem::take(&mut self.link) {
            LinkBox::Closed => None,
            LinkBox::Review(draft)
            | LinkBox::Warn(draft)
            | LinkBox::Checking { draft, .. }
            | LinkBox::Available { draft, .. }
            | LinkBox::Unavailable(draft) => Some(draft),
        }
    }

    /// Check Status asks for the one-time "this exposes your IP" warning
    /// first.
    pub fn warn_link(&mut self) {
        if let Some(draft) = self.link.draft().cloned() {
            self.link = LinkBox::Warn(draft);
        }
    }

    /// `pingProxy` for the open box went out under `request`.
    pub fn link_checking(&mut self, request: crate::ids::RequestId) {
        if let Some(draft) = self.link.draft().cloned() {
            self.link = LinkBox::Checking { draft, request };
        }
    }

    /// The ping `request` answered (or failed): only the box that is
    /// `Checking` under that request settles.
    pub fn settle_link(&mut self, request: RequestId, status: PingStatus) {
        self.link = match std::mem::take(&mut self.link) {
            LinkBox::Checking {
                draft,
                request: open,
            } if open == request => match status {
                PingStatus::Available(ms) => LinkBox::Available { draft, ms },
                PingStatus::Checking(_) | PingStatus::Unavailable => LinkBox::Unavailable(draft),
            },
            other => other,
        };
    }

    /// The auto-switch settings changed: the rotation restarts with them.
    /// The one place that resets it on a prefs change.
    pub fn set_auto_switch(&mut self, on: Option<bool>, secs: Option<u32>) {
        if let Some(on) = on {
            self.prefs.auto_switch = on;
        }
        if let Some(secs) = secs {
            self.prefs.auto_switch_secs = secs;
        }
        self.rotation = Rotation::Watching;
    }

    pub fn entries(&self) -> &[ProxyEntry] {
        self.list.as_deref().unwrap_or(&[])
    }

    /// The list is changing or about to: a mutation is out, or a fetch is
    /// owed or on its way.
    pub fn busy(&self) -> bool {
        self.ops.busy()
    }

    /// Whether a proxy of `kind` may be switched to: a web proxy only when
    /// this client can host its carrier (`crate::proxy_carrier`).
    pub fn can_enable(&self, kind: ProxyKind) -> bool {
        kind != ProxyKind::Web || self.carriers.hosting
    }

    /// The enabled proxy, if any.
    pub fn enabled(&self) -> Option<&ProxyEntry> {
        self.entries().iter().find(|p| p.is_enabled)
    }

    /// A list entry or a system proxy entry by its TDLib id.
    pub fn find(&self, id: i32) -> Option<&ProxyEntry> {
        self.entries()
            .iter()
            .chain(&self.system)
            .find(|p| p.id == id)
    }

    /// TDLib's whole list, the system proxy's entries included: what the
    /// background accounts mirror (`crate::proxy_sync`). `None` until the
    /// first `getProxies` answer.
    pub fn all_entries(&self) -> Option<Vec<ProxyEntry>> {
        let mut all = self.list.clone()?;
        all.extend(self.system.iter().cloned());
        Some(all)
    }

    /// The radio choice of the proxy box (tdesktop `ProxyData::Settings`).
    pub fn mode(&self) -> ProxyMode {
        if self.enabled().is_some() {
            ProxyMode::Custom
        } else if self.prefs.use_system {
            ProxyMode::System
        } else {
            ProxyMode::Disabled
        }
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

    /// The ping `RotationAction::Ping(id)` asked for went out as `request`
    /// at `now_ms`: the rotation waits for that request.
    pub fn probe_sent(&mut self, id: i32, request: RequestId, now_ms: u64) {
        if let Rotation::Disconnected { since_ms } = self.rotation {
            self.rotation = Rotation::Probing {
                since_ms,
                id,
                request,
                asked_ms: now_ms,
            };
        }
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
            self.rotation = Rotation::Watching;
            return RotationAction::None;
        }
        let (since, probing) = match self.rotation {
            Rotation::Watching => (now_ms, None),
            Rotation::Disconnected { since_ms } => (since_ms, None),
            Rotation::Probing {
                since_ms,
                id,
                request,
                asked_ms,
            } => (since_ms, Some((id, request, asked_ms))),
        };
        if self.rotation == Rotation::Watching {
            self.rotation = Rotation::Disconnected { since_ms: since };
        }
        if now_ms.saturating_sub(since) < u64::from(self.prefs.timeout_secs()) * 1000 {
            return RotationAction::None;
        }
        // The probe asked for is still out: wait for its answer, but not
        // for ever: a ping whose answer never comes is a failed one (its
        // late answer then matches nothing and is dropped).
        if let Some((id, request, asked_ms)) = probing
            && self.pings.get(&id) == Some(&PingStatus::Checking(request))
        {
            if now_ms.saturating_sub(asked_ms) < PROBE_TIMEOUT_MS {
                return RotationAction::None;
            }
            self.pings.insert(id, PingStatus::Unavailable);
        }
        let entries = self.entries();
        let current = entries.iter().position(|p| p.is_enabled).unwrap_or(0);
        // Candidates in list order, starting after the current proxy.
        // Web proxies are skipped: they can't be pinged.
        let order: Vec<&ProxyEntry> = (1..entries.len())
            .filter_map(|step| entries.get((current + step) % entries.len()))
            .filter(|p| p.proxy.kind.can_be_rotated_to())
            .collect();
        if let Some(found) = order
            .iter()
            .find(|p| matches!(self.pings.get(&p.id), Some(PingStatus::Available(_))))
        {
            let id = found.id;
            self.rotation = Rotation::Disconnected { since_ms: now_ms };
            self.pings.remove(&id);
            return RotationAction::Switch(id);
        }
        if let Some(next) = order
            .iter()
            .find(|p| !self.pings.contains_key(&p.id))
            .map(|p| p.id)
        {
            // The probe is Probing once its send went out (`probe_sent`).
            self.rotation = Rotation::Disconnected { since_ms: since };
            return RotationAction::Ping(next);
        }
        // Every alternative failed: forget the failed verdicts and look again
        // after another timeout. A check the user has running is not one of
        // them: its answer still lands.
        let ids: Vec<i32> = order.iter().map(|p| p.id).collect();
        for id in ids {
            if self.pings.get(&id) == Some(&PingStatus::Unavailable) {
                self.pings.remove(&id);
            }
        }
        self.rotation = Rotation::Disconnected { since_ms: now_ms };
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
                ..ProxyPrefs::default()
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
    fn changing_the_auto_switch_settings_restarts_the_rotation_itself() {
        let mut state = ProxyState {
            rotation: Rotation::Disconnected { since_ms: 5 },
            ..ProxyState::default()
        };
        state.set_auto_switch(Some(true), Some(30));
        assert!(state.prefs.auto_switch);
        assert_eq!(state.prefs.auto_switch_secs, 30);
        assert_eq!(state.rotation, Rotation::Watching);
        state.rotation = Rotation::Probing {
            since_ms: 1,
            id: 2,
            request: RequestId(3),
            asked_ms: 1,
        };
        state.set_auto_switch(None, None);
        assert_eq!(state.rotation, Rotation::Watching, "any change restarts it");
    }

    #[test]
    fn the_link_box_is_one_stage_that_holds_its_draft() {
        let draft = |server: &str| ProxyDraft {
            server: server.into(),
            ..ProxyDraft::empty(ProxyKind::Socks5)
        };
        let mut state = ProxyState::default();
        state.warn_link();
        assert_eq!(state.link, LinkBox::Closed, "no box, no warning");
        state.open_link(draft("a"));
        state.warn_link();
        assert_eq!(state.link, LinkBox::Warn(draft("a")));
        state.link_checking(RequestId(4));
        // Closed, then another link opened: the first check's answer is late.
        state.close_link();
        state.open_link(draft("b"));
        state.settle_link(RequestId(4), PingStatus::Available(9));
        assert_eq!(state.link, LinkBox::Review(draft("b")));
        state.link_checking(RequestId(5));
        state.settle_link(RequestId(4), PingStatus::Unavailable);
        assert_eq!(
            state.link.status(),
            Some(PingStatus::Checking(RequestId(5)))
        );
        state.settle_link(RequestId(5), PingStatus::Available(30));
        assert_eq!(
            state.link,
            LinkBox::Available {
                draft: draft("b"),
                ms: 30
            }
        );
        assert_eq!(state.take_link(), Some(draft("b")));
        assert_eq!(state.link, LinkBox::Closed);
        assert_eq!(state.take_link(), None);
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
        s.pings.insert(2, PingStatus::Checking(RequestId(7)));
        s.probe_sent(2, RequestId(7), 10_000);
        assert_eq!(s.rotation_step(10_500, false), RotationAction::None);
        s.pings.insert(2, PingStatus::Unavailable);
        assert_eq!(s.rotation_step(11_000, false), RotationAction::Ping(3));
        s.pings.insert(3, PingStatus::Available(80));
        assert_eq!(s.rotation_step(12_000, false), RotationAction::Switch(3));
    }

    /// Sweep r4-04 fix 3: looking again after every alternative failed keeps
    /// a check the user has running.
    #[test]
    fn a_rotation_round_that_failed_keeps_the_users_running_check() {
        let mut s = state(vec![entry(1, true), entry(2, false), entry(3, false)], true);
        s.rotation_step(0, false);
        s.pings.insert(2, PingStatus::Unavailable);
        s.pings.insert(3, PingStatus::Checking(RequestId(5)));
        assert_eq!(s.rotation_step(10_000, false), RotationAction::None);
        assert_eq!(s.pings.get(&2), None, "the failed verdict is forgotten");
        assert_eq!(s.pings.get(&3), Some(&PingStatus::Checking(RequestId(5))));
    }

    #[test]
    fn rotation_resets_when_connected_or_disabled() {
        let mut s = state(vec![entry(1, true), entry(2, false)], true);
        s.rotation_step(0, false);
        assert_eq!(s.rotation_step(20_000, true), RotationAction::None);
        assert_eq!(s.rotation, Rotation::Watching);
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
    fn rotation_skips_web_proxies() {
        let mut web = entry(2, false);
        web.proxy.kind = ProxyKind::Web;
        let mut s = state(vec![entry(1, true), web, entry(3, false)], true);
        s.rotation_step(0, false);
        // The web proxy is neither pinged nor switched to.
        assert_eq!(s.rotation_step(10_000, false), RotationAction::Ping(3));
        s.pings.insert(3, PingStatus::Unavailable);
        assert_eq!(s.rotation_step(10_500, false), RotationAction::None);
        assert!(!s.pings.contains_key(&2));
    }

    #[test]
    fn a_ping_of_the_edit_box_does_not_hold_the_rotation() {
        let mut s = state(vec![entry(1, true), entry(2, false)], true);
        s.rotation_step(0, false);
        // The link test of the proxy box is out, not the rotation's probe.
        s.open_link(ProxyDraft::empty(ProxyKind::Socks5));
        s.link_checking(crate::ids::RequestId(5));
        assert_eq!(s.rotation_step(10_000, false), RotationAction::Ping(2));
        // The probe is out only once its send went out.
        assert_eq!(s.rotation, Rotation::Disconnected { since_ms: 0 });
        s.pings.insert(2, PingStatus::Checking(RequestId(7)));
        s.probe_sent(2, RequestId(7), 10_000);
        assert_eq!(
            s.rotation,
            Rotation::Probing {
                since_ms: 0,
                id: 2,
                request: RequestId(7),
                asked_ms: 10_000
            }
        );
        // Its own probe does hold it.
        assert_eq!(s.rotation_step(10_500, false), RotationAction::None);
        // Another request on the same slot (a newer ping) is not the probe:
        // the rotation no longer waits on it, and with no other proxy to
        // try it starts over.
        s.pings.insert(2, PingStatus::Checking(RequestId(8)));
        assert_eq!(s.rotation_step(10_600, false), RotationAction::None);
        assert_eq!(s.rotation, Rotation::Disconnected { since_ms: 10_600 });
    }

    #[test]
    fn a_probe_whose_answer_never_comes_counts_as_failed() {
        let mut s = state(vec![entry(1, true), entry(2, false), entry(3, false)], true);
        s.rotation_step(0, false);
        assert_eq!(s.rotation_step(10_000, false), RotationAction::Ping(2));
        s.pings.insert(2, PingStatus::Checking(RequestId(7)));
        s.probe_sent(2, RequestId(7), 10_000);
        assert_eq!(
            s.rotation_step(10_000 + PROBE_TIMEOUT_MS - 1, false),
            RotationAction::None
        );
        // The probe is given up: the next proxy is probed, and the lost
        // ping's late answer matches nothing.
        assert_eq!(
            s.rotation_step(10_000 + PROBE_TIMEOUT_MS, false),
            RotationAction::Ping(3)
        );
        assert_eq!(s.pings.get(&2), Some(&PingStatus::Unavailable));
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
            ..ProxyPrefs::default()
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

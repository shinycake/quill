//! "Use system proxy settings" (`parity:data-proxy-extras`).
//!
//! Telegram Desktop's default proxy mode is `ProxyData::Settings::System`:
//! its MTProto sockets take `QNetworkProxy::DefaultProxy`, which Qt fills
//! from the operating system's proxy configuration
//! (`Sandbox::refreshGlobalProxy`, `QNetworkProxyFactory::
//! setUseSystemConfiguration(true)`). TDLib has no such hook, so Quill
//! reads the OS configuration itself ([`detect`]) and hands the proxy it
//! finds to TDLib as one extra entry of the proxy list, tagged with
//! [`SYSTEM_PROXY_COMMENT`]. That entry never shows in the proxy list
//! (`ProxyState::entries` leaves it out), is not shared, is not part of the
//! auto-switch, and does not carry call media, as in tdesktop, where the
//! system proxy is not a list entry either.
//!
//! Where the configuration comes from:
//! - macOS: `scutil --proxy` (the `SCDynamicStoreCopyProxies` dictionary).
//! - Windows: the WinINet settings under `HKCU\Software\Microsoft\Windows\
//!   CurrentVersion\Internet Settings` (`ProxyEnable`, `ProxyServer`), the
//!   ones Qt's `systemProxyForQuery` reads through WinHTTP.
//! - Linux: `all_proxy`, `https_proxy`, `http_proxy` (either case), as Qt's
//!   generic Unix factory does, then GNOME's `org.gnome.system.proxy`
//!   through `gsettings` when its mode is `manual`.
//!
//! A SOCKS5 proxy wins over an HTTPS one, which wins over an HTTP one;
//! TDLib reaches the servers through an HTTP proxy with `CONNECT`.
//! Proxy auto-config scripts (PAC, WPAD) are not evaluated: with only a
//! PAC URL configured Quill connects directly.

mod platform;
#[cfg(test)]
mod tests;

use crate::calls::proxy::ProxyKind;
use crate::proxy::{ProxyDraft, ProxyEntry};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The comment that marks the system proxy's entry in TDLib's list.
/// How long a step TDLib refused waits before it is sent again.
pub const RETRY: std::time::Duration = std::time::Duration::from_secs(300);

/// A step the connection driver sent, until its answer is known.
#[derive(Debug, Clone)]
pub struct SentStep {
    pub step: SystemProxyStep,
    pub at: std::time::Instant,
    /// TDLib's answer came back as an error.
    pub refused: bool,
    /// The answer came back.
    pub settled: bool,
}

pub const SYSTEM_PROXY_COMMENT: &str = "Quill: system proxy";

/// How often the OS configuration is read again while Quill runs.
const REFRESH: Duration = Duration::from_secs(15);

/// Whether `entry` is the system proxy's entry.
pub fn is_system_entry(entry: &ProxyEntry) -> bool {
    entry.comment == SYSTEM_PROXY_COMMENT
}

/// What the OS says, as last read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detected {
    /// Not read yet: do nothing until the first answer.
    Unknown,
    /// Direct connection (no proxy configured, or only a PAC script).
    Direct,
    Proxy(ProxyDraft),
}

/// Read the OS proxy configuration now. Blocking (it may run `scutil` or
/// `gsettings`), so call it off the UI thread; [`latest`] does.
pub fn detect() -> Option<ProxyDraft> {
    platform::detect()
}

/// Whether the background reader should read (the mode is "Use system
/// proxy settings" and no custom proxy is on).
static WANTED: AtomicBool = AtomicBool::new(false);

/// The OS proxy configuration while `wanted`, as last read by the
/// background reader (it starts on the first wanted call and reads every
/// [`REFRESH`] while wanted). Not wanted: `Direct`, which drops the hidden
/// entry, and nothing is read. Never blocks.
pub fn latest(wanted: bool) -> Detected {
    WANTED.store(wanted, Ordering::Relaxed);
    if !wanted {
        return Detected::Direct;
    }
    static SLOT: OnceLock<Arc<Mutex<Detected>>> = OnceLock::new();
    let slot = SLOT.get_or_init(|| {
        let slot = Arc::new(Mutex::new(Detected::Unknown));
        let writer = slot.clone();
        // The reader lives as long as the process; a failed spawn leaves
        // the slot at `Unknown`, which keeps the system mode inert.
        let _ = crate::thread_guard::spawn_guarded("quill-system-proxy", move || {
            loop {
                if WANTED.load(Ordering::Relaxed) {
                    let found = match detect() {
                        Some(proxy) => Detected::Proxy(proxy),
                        None => Detected::Direct,
                    };
                    if let Ok(mut guard) = writer.lock() {
                        *guard = found;
                    }
                }
                std::thread::sleep(REFRESH);
            }
        });
        slot
    });
    match slot.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// One change that brings TDLib's list in line with the proxy mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemProxyStep {
    /// Add the system proxy, enabled.
    Add(ProxyDraft),
    /// The OS proxy changed: rewrite the entry, enabled.
    Edit { id: i32, proxy: ProxyDraft },
    /// The entry is there but not in use.
    Enable { id: i32 },
    /// The entry is not wanted (another mode, or the OS went direct), or
    /// is a duplicate.
    Remove { id: i32 },
}

/// The next step toward the wanted state, `None` once there. `use_system`
/// is the saved mode ("Use system proxy settings" picked, or no choice
/// made yet); a custom proxy that is enabled overrides it, as tdesktop's
/// `Settings::Enabled` does. `system` holds the tagged entries, oldest
/// first. One step at a time: TDLib takes one proxy change at a time in
/// Quill and every change refetches the list.
pub fn plan_system_proxy(
    use_system: bool,
    custom_enabled: bool,
    detected: &Detected,
    system: &[ProxyEntry],
) -> Option<SystemProxyStep> {
    let mut entries = system.iter();
    let first = entries.next();
    if let Some(extra) = entries.next() {
        return Some(SystemProxyStep::Remove { id: extra.id });
    }
    let wanted = match detected {
        Detected::Unknown => return None,
        Detected::Proxy(proxy) if use_system && !custom_enabled => Some(proxy),
        _ => None,
    };
    match (wanted, first) {
        (None, Some(entry)) => Some(SystemProxyStep::Remove { id: entry.id }),
        (None, None) => None,
        (Some(proxy), None) => Some(SystemProxyStep::Add(proxy.clone())),
        (Some(proxy), Some(entry)) if !entry.proxy.same_endpoint(proxy) => {
            Some(SystemProxyStep::Edit {
                id: entry.id,
                proxy: proxy.clone(),
            })
        }
        (Some(_), Some(entry)) if !entry.is_enabled => {
            Some(SystemProxyStep::Enable { id: entry.id })
        }
        (Some(_), Some(_)) => None,
    }
}

fn draft(kind: ProxyKind, server: &str, port: u16) -> Option<ProxyDraft> {
    let server = server.trim();
    if server.is_empty() || port == 0 {
        return None;
    }
    let mut proxy = ProxyDraft::empty(kind);
    proxy.server = server.to_string();
    proxy.port = port;
    Some(proxy)
}

/// macOS: the `scutil --proxy` dictionary. Keys are the
/// `kSCPropNetProxies*` names; credentials live in the keychain and are
/// not part of it.
pub fn parse_scutil_proxy(text: &str) -> Option<ProxyDraft> {
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once(" : ")?;
            (name.trim() == key).then(|| value.trim().to_string())
        })
    };
    let pick = |prefix: &str, kind: ProxyKind| {
        if value(&format!("{prefix}Enable")).as_deref() != Some("1") {
            return None;
        }
        let server = value(&format!("{prefix}Proxy"))?;
        let port = value(&format!("{prefix}Port"))?.parse().ok()?;
        draft(kind, &server, port)
    };
    pick("SOCKS", ProxyKind::Socks5)
        .or_else(|| pick("HTTPS", ProxyKind::Http))
        .or_else(|| pick("HTTP", ProxyKind::Http))
}

/// Windows: `ProxyServer` of the WinINet settings, either one
/// `host:port` for every protocol or `scheme=host:port` pairs split by
/// `;` (`socks=`, `https=`, `http=`).
pub fn parse_windows_proxy_server(server: &str) -> Option<ProxyDraft> {
    let entries: Vec<&str> = server
        .split([';', ' '])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect();
    let scheme = |name: &str| {
        entries.iter().find_map(|entry| {
            let (scheme, address) = entry.split_once('=')?;
            scheme.trim().eq_ignore_ascii_case(name).then_some(address)
        })
    };
    if let Some(proxy) = scheme("socks").and_then(|a| parse_proxy_url(&format!("socks5://{a}"))) {
        return Some(proxy);
    }
    let any = entries.iter().find(|entry| !entry.contains('=')).copied();
    scheme("https")
        .or_else(|| scheme("http"))
        .or(any)
        .and_then(|address| parse_proxy_url(&format!("http://{}", strip_scheme(address))))
}

fn strip_scheme(address: &str) -> &str {
    address.split_once("://").map_or(address, |(_, rest)| rest)
}

/// A proxy URL as the `*_proxy` variables carry it:
/// `[scheme://][user[:password]@]host[:port][/]`. `socks5`, `socks5h` and
/// `socks` mean SOCKS5 (default port 1080); `http`, `https` or no scheme an
/// HTTP proxy (default port 8080, Qt's). Other schemes (`socks4`) are
/// skipped.
pub fn parse_proxy_url(url: &str) -> Option<ProxyDraft> {
    let url = url.trim();
    let (scheme, rest) = match url.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => ("http".to_string(), url),
    };
    let (kind, default_port) = match scheme.as_str() {
        "socks5" | "socks5h" | "socks" => (ProxyKind::Socks5, 1080),
        "http" | "https" => (ProxyKind::Http, 8080),
        _ => return None,
    };
    let rest = rest.split('/').next().unwrap_or_default();
    let (login, address) = match rest.rsplit_once('@') {
        Some((login, address)) => (Some(login), address),
        None => (None, rest),
    };
    let (host, port) = split_host_port(address)?;
    let port = match port {
        Some(port) => port.parse().ok()?,
        None => default_port,
    };
    let mut proxy = draft(kind, host, port)?;
    if let Some(login) = login {
        let (user, password) = login.split_once(':').unwrap_or((login, ""));
        proxy.username = percent_decode(user);
        proxy.password = percent_decode(password);
    }
    Some(proxy)
}

/// `host`, `host:port`, `[v6]` or `[v6]:port`.
fn split_host_port(address: &str) -> Option<(&str, Option<&str>)> {
    if let Some(rest) = address.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = after.strip_prefix(':');
        return Some((host, port));
    }
    match address.rsplit_once(':') {
        // A bare IPv6 address has more than one colon and no port.
        Some((host, port)) if !host.contains(':') => Some((host, Some(port))),
        _ => Some((address, None)),
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (byte, hex) {
            (b'%', Some(decoded)) => {
                out.push(decoded);
                i += 3;
            }
            _ => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Linux: the first set variable of `all_proxy`, `https_proxy`,
/// `http_proxy` (lower case first, as curl and Qt read them). `var` looks a
/// name up.
pub fn proxy_from_env(var: impl Fn(&str) -> Option<String>) -> Option<ProxyDraft> {
    ["all_proxy", "https_proxy", "http_proxy"]
        .iter()
        .find_map(|name| {
            var(name)
                .or_else(|| var(&name.to_ascii_uppercase()))
                .filter(|value| !value.trim().is_empty())
        })
        .and_then(|value| parse_proxy_url(&value))
}

/// GNOME's `org.gnome.system.proxy`: `get` reads a key of a schema
/// (`gsettings get` output, quoted strings and plain numbers). Only the
/// `manual` mode names hosts; `auto` is a PAC URL.
pub fn proxy_from_gsettings(get: impl Fn(&str, &str) -> Option<String>) -> Option<ProxyDraft> {
    let unquote = |value: String| value.trim().trim_matches('\'').to_string();
    if get("org.gnome.system.proxy", "mode")
        .map(unquote)
        .as_deref()
        != Some("manual")
    {
        return None;
    }
    let pick = |child: &str, kind: ProxyKind| {
        let schema = format!("org.gnome.system.proxy.{child}");
        let host = get(&schema, "host").map(unquote)?;
        let port = get(&schema, "port")?.trim().parse().ok()?;
        draft(kind, &host, port)
    };
    pick("socks", ProxyKind::Socks5)
        .or_else(|| pick("https", ProxyKind::Http))
        .or_else(|| pick("http", ProxyKind::Http))
}

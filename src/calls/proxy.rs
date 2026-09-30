//! "Use proxy for calls" — client-side proxy selection for call media.
//!
//! The TDLib schema (pinned 1.8.67) has no "use proxy for calls" option:
//! proxy management is `addProxy`/`enableProxy`/… (`schema/td_api.tl`
//! :16202–16304) and the only `use-for-calls` mention is the
//! `proxy/use-for-calls` settings deep-link subsection (:9276). TDLib
//! also transports no call media (signaling only: `createCall`,
//! `sendCallSignalingData`), so routing calls through the proxy is the
//! client's job: when the toggle is on, the client hands the enabled
//! proxy to its VoIP engine.
//!
//! Official rule (Telegram Desktop / iOS): only SOCKS5 proxies are
//! offered for calls — MTProto and HTTP proxies cannot carry call
//! (UDP) media ("Calls remain SOCKS5-only").
//!
//! Quill has no proxy management yet, so `proxy_for_calls` currently
//! only ever sees `None` from the driver; the future proxy-management
//! slice will feed it the enabled proxy from TDLib `getProxies`
//! (`addedProxy` with `is_enabled`, schema :10112).

/// Proxy type as reported by TDLib `getProxies` (`proxyTypeSocks5`,
/// `proxyTypeHttp`, `proxyTypeMtproto`, schema :10100–10109).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    Socks5,
    Http,
    Mtproto,
}

/// The proxy TDLib reports as enabled, in the shape the future
/// `getProxies` plumbing will hand over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnabledProxy {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

/// Normalized SOCKS5 proxy for the call media engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallProxy {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

/// Select the proxy to route call media through.
///
/// Returns `Some` only when the toggle is on AND the enabled proxy is
/// SOCKS5. Everything else (toggle off, no proxy, MTProto/HTTP proxy)
/// yields `None` — the call goes direct.
pub fn proxy_for_calls(
    use_proxy_for_calls: bool,
    enabled: Option<&EnabledProxy>,
) -> Option<CallProxy> {
    if !use_proxy_for_calls {
        return None;
    }
    let proxy = enabled.filter(|p| p.kind == ProxyKind::Socks5)?;
    Some(CallProxy {
        host: proxy.host.clone(),
        port: proxy.port,
        username: proxy.username.clone(),
        password: proxy.password.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socks5() -> EnabledProxy {
        EnabledProxy {
            kind: ProxyKind::Socks5,
            host: "proxy.example".into(),
            port: 1080,
            username: "user".into(),
            password: "pass".into(),
        }
    }

    #[test]
    fn toggle_off_never_proxies() {
        assert_eq!(proxy_for_calls(false, Some(&socks5())), None);
        assert_eq!(proxy_for_calls(false, None), None);
    }

    #[test]
    fn no_enabled_proxy_goes_direct() {
        assert_eq!(proxy_for_calls(true, None), None);
    }

    #[test]
    fn socks5_proxy_is_used() {
        assert_eq!(
            proxy_for_calls(true, Some(&socks5())),
            Some(CallProxy {
                host: "proxy.example".into(),
                port: 1080,
                username: "user".into(),
                password: "pass".into(),
            })
        );
    }

    #[test]
    fn mtproto_and_http_proxies_are_not_offered_for_calls() {
        // The official rule under test: "Calls remain SOCKS5-only" — an
        // enabled MTProto/HTTP proxy must not reroute call media.
        for kind in [ProxyKind::Mtproto, ProxyKind::Http] {
            let proxy = EnabledProxy { kind, ..socks5() };
            assert_eq!(proxy_for_calls(true, Some(&proxy)), None);
        }
    }
}

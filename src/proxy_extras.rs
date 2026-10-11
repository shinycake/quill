//! Pure helpers for the proxy extras (`parity:data-proxy-extras`): the
//! "Connection type" label and the link a share QR code encodes. The
//! proxy list, auto-switch and persistence live in `proxy.rs`.
//!
//! tdesktop reference: `settings/sections/settings_advanced.cpp`
//! (`connectionType`) and `boxes/connection_box.cpp` (`ShareProxy`).
use crate::proxy::{ProxyEntry, ProxyState};

/// The value shown on the "Connection type" row. tdesktop shows
/// "Default (…used)" / "{transport} with proxy"; TDLib does not expose the
/// transport, so Quill shows the mode and, with a proxy, the endpoint.
pub fn connection_type_label(state: &ProxyState, connected: bool) -> String {
    match state.enabled() {
        None if connected => "Default".into(),
        None => "Default (connecting…)".into(),
        Some(entry) if connected => format!(
            "{} proxy {}:{}",
            entry.proxy.kind_label(),
            entry.proxy.server,
            entry.proxy.port
        ),
        Some(_) => "Connecting through proxy…".into(),
    }
}

/// The link the share QR code encodes. tdesktop encodes the `tg://` form
/// (`ProxyDataToLocalLink`) so the QR works offline in any client; HTTP
/// proxies have no link.
pub fn qr_share_link(entry: &ProxyEntry) -> Option<String> {
    entry.proxy.share_link(true)
}

#[cfg(test)]
mod tests {
    use super::{connection_type_label, qr_share_link};
    use crate::calls::proxy::ProxyKind;
    use crate::proxy::{ProxyDraft, ProxyEntry, ProxyPrefs, ProxyState};

    fn entry(id: i32, kind: ProxyKind, enabled: bool) -> ProxyEntry {
        let mut proxy = ProxyDraft::empty(kind);
        proxy.server = "p.example".into();
        proxy.port = 1080;
        ProxyEntry {
            id,
            last_used_date: 0,
            is_enabled: enabled,
            comment: String::new(),
            proxy,
        }
    }

    fn state(list: Vec<ProxyEntry>) -> ProxyState {
        ProxyState {
            list: Some(list),
            ..ProxyState::default()
        }
    }

    #[test]
    fn label_follows_proxy_and_connection() {
        let none = state(vec![entry(1, ProxyKind::Socks5, false)]);
        assert_eq!(connection_type_label(&none, true), "Default");
        assert_eq!(connection_type_label(&none, false), "Default (connecting…)");
        let on = state(vec![entry(1, ProxyKind::Socks5, true)]);
        assert_eq!(
            connection_type_label(&on, true),
            "SOCKS5 proxy p.example:1080"
        );
        assert_eq!(
            connection_type_label(&on, false),
            "Connecting through proxy…"
        );
    }

    #[test]
    fn qr_link_is_the_tg_form_and_http_has_none() {
        let socks = entry(1, ProxyKind::Socks5, false);
        assert_eq!(
            qr_share_link(&socks).as_deref(),
            Some("tg://socks?server=p.example&port=1080")
        );
        assert!(qr_share_link(&entry(2, ProxyKind::Http, false)).is_none());
    }

    #[test]
    fn auto_switch_prefs_survive_a_json_round_trip() {
        let prefs = ProxyPrefs {
            auto_switch: true,
            auto_switch_secs: 30,
        };
        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<ProxyPrefs>(&json).unwrap(), prefs);
        // A file from before the timeout existed still loads.
        let old: ProxyPrefs = serde_json::from_str(r#"{"auto_switch":true}"#).unwrap();
        assert!(old.auto_switch);
        assert_eq!(old.auto_switch_secs, 10);
    }
}

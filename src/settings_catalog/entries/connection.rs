//! Connection page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Connection;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new("connection.mode", P, keys::Q_PREFS_CONNECTION_TYPE_2)
        .keywords(&["proxy", "vpn", "tunnel"]),
    Entry::new("connection.proxy_disable", P, keys::PROXY_DISABLE).section(keys::PROXY_SETTINGS),
    Entry::new(
        "connection.proxy_system",
        P,
        keys::PROXY_USE_SYSTEM_SETTINGS,
    )
    .section(keys::PROXY_SETTINGS),
    Entry::new("connection.proxy_custom", P, keys::PROXY_USE_CUSTOM)
        .section(keys::PROXY_SETTINGS)
        .keywords(&["socks5", "mtproto", "http"]),
    Entry::new(
        "connection.proxy_list",
        P,
        keys::Q_SETTINGS_CONNECTION_PROXY_LIST,
    )
    .section(keys::PROXY_SETTINGS)
    .keywords(&["servers", "edit proxy", "delete proxy", "qr code"]),
    Entry::new("connection.proxy_add", P, keys::PROXY_ADD)
        .section(keys::PROXY_SETTINGS)
        .keywords(&["new proxy", "server"]),
    Entry::new(
        "connection.proxy_add_clipboard",
        P,
        keys::PROXY_ADD_FROM_CLIPBOARD,
    )
    .section(keys::PROXY_SETTINGS)
    .keywords(&["paste"]),
    Entry::new(
        "connection.proxy_share_list",
        P,
        keys::PROXY_EDIT_SHARE_LIST_BUTTON,
    )
    .section(keys::PROXY_SETTINGS)
    .keywords(&["copy proxies", "export"]),
    Entry::new("connection.auto_switch", P, keys::PROXY_AUTO_SWITCH)
        .keywords(&["failover", "fastest proxy"]),
    Entry::new("connection.ipv6", P, keys::CONNECTION_TRY_IPV6).keywords(&["ip version"]),
    Entry::new("connection.proxy_for_calls", P, keys::PROXY_USE_FOR_CALLS)
        .keywords(&["voice", "socks5"]),
];

use super::{
    Detected, REREAD_EVERY, SYSTEM_PROXY_COMMENT, SystemProxyStep, is_system_entry,
    parse_proxy_url, parse_windows_proxy_server, plan_system_proxy, proxy_from_dictionary,
    proxy_from_env, proxy_from_gsettings, reread_due, serve,
};
use crate::calls::proxy::ProxyKind;
use crate::proxy::{ProxyDraft, ProxyEntry};
use crate::telegram::envelope::ConnectionState;
use std::sync::mpsc::sync_channel;
use std::time::Instant;

fn proxy(kind: ProxyKind, server: &str, port: u16) -> ProxyDraft {
    let mut proxy = ProxyDraft::empty(kind);
    proxy.server = server.into();
    proxy.port = port;
    proxy
}

fn entry(id: i32, proxy: ProxyDraft, enabled: bool) -> ProxyEntry {
    ProxyEntry {
        id,
        last_used_date: 0,
        is_enabled: enabled,
        comment: SYSTEM_PROXY_COMMENT.into(),
        proxy,
    }
}

fn dictionary(entries: &[(&str, &str)]) -> Option<ProxyDraft> {
    proxy_from_dictionary(|key| {
        let found = entries.iter().find(|(name, _)| *name == key)?;
        Some(found.1.to_string())
    })
}

#[test]
fn the_dictionary_prefers_socks_then_https_then_http() {
    let mut entries = vec![
        ("HTTPEnable", "1"),
        ("HTTPPort", "3128"),
        ("HTTPProxy", "web.lan"),
        ("HTTPSEnable", "1"),
        ("HTTPSPort", "3129"),
        ("HTTPSProxy", "secure.lan"),
        ("SOCKSEnable", "0"),
        ("SOCKSPort", "1080"),
        ("SOCKSProxy", "socks.lan"),
    ];
    assert_eq!(
        dictionary(&entries),
        Some(proxy(ProxyKind::Http, "secure.lan", 3129))
    );
    entries.push(("SOCKSEnable", "1"));
    entries.swap_remove(6);
    assert_eq!(
        dictionary(&entries),
        Some(proxy(ProxyKind::Socks5, "socks.lan", 1080))
    );
    assert_eq!(
        dictionary(&[("HTTPEnable", "0"), ("HTTPSEnable", "0")]),
        None
    );
}

fn reads_for(ask: impl Fn(&std::sync::mpsc::SyncSender<()>)) -> (usize, Vec<Detected>) {
    let (asks, requests) = sync_channel(1);
    ask(&asks);
    drop(asks);
    let mut reads = 0;
    let mut published = Vec::new();
    serve(
        &requests,
        || {
            reads += 1;
            Detected::Direct
        },
        |found| published.push(found),
    );
    (reads, published)
}

#[test]
fn with_no_request_nothing_is_read() {
    assert_eq!(reads_for(|_| {}), (0, vec![]));
}

#[test]
fn a_request_reads_once_and_publishes() {
    let (reads, published) = reads_for(|asks| {
        let _ = asks.try_send(());
    });
    assert_eq!(reads, 1);
    assert_eq!(published, [Detected::Direct]);
}

#[test]
fn a_burst_of_requests_reads_once() {
    let (reads, _) = reads_for(|asks| {
        for _ in 0..50 {
            let _ = asks.try_send(());
        }
    });
    assert_eq!(reads, 1);
}

#[test]
fn only_a_connection_being_opened_asks_again_and_not_too_often() {
    let start = Instant::now();
    for quiet in [
        ConnectionState::Ready,
        ConnectionState::Updating,
        ConnectionState::Initial,
    ] {
        assert!(!reread_due(quiet, None, start));
    }
    let connecting = ConnectionState::Connecting;
    assert!(reread_due(connecting, None, start));
    assert!(reread_due(ConnectionState::ConnectingToProxy, None, start));
    assert!(reread_due(ConnectionState::WaitingForNetwork, None, start));
    assert!(!reread_due(connecting, Some(start), start));
    assert!(!reread_due(
        connecting,
        Some(start),
        start + REREAD_EVERY / 2
    ));
    assert!(reread_due(connecting, Some(start), start + REREAD_EVERY));
}

#[test]
fn windows_server_string_forms() {
    assert_eq!(
        parse_windows_proxy_server("proxy.corp:8080"),
        Some(proxy(ProxyKind::Http, "proxy.corp", 8080))
    );
    assert_eq!(
        parse_windows_proxy_server("http=web:80;https=secure:443;socks=s:1080"),
        Some(proxy(ProxyKind::Socks5, "s", 1080))
    );
    assert_eq!(
        parse_windows_proxy_server("http=web:80;https=secure:443"),
        Some(proxy(ProxyKind::Http, "secure", 443))
    );
    assert_eq!(
        parse_windows_proxy_server("ftp=f:21"),
        None,
        "only an FTP proxy is no proxy for Quill"
    );
    assert_eq!(parse_windows_proxy_server(""), None);
}

#[test]
fn proxy_urls() {
    let mut with_login = proxy(ProxyKind::Socks5, "10.0.0.2", 9050);
    with_login.username = "me".into();
    with_login.password = "p@ss".into();
    assert_eq!(
        parse_proxy_url("socks5h://me:p%40ss@10.0.0.2:9050/"),
        Some(with_login)
    );
    assert_eq!(
        parse_proxy_url("http://cache"),
        Some(proxy(ProxyKind::Http, "cache", 8080))
    );
    assert_eq!(
        parse_proxy_url("[::1]:3128"),
        Some(proxy(ProxyKind::Http, "::1", 3128))
    );
    assert_eq!(parse_proxy_url("socks4://old:1080"), None);
    assert_eq!(parse_proxy_url("http://host:notaport"), None);
    assert_eq!(parse_proxy_url("http://:8080"), None);
}

#[test]
fn env_order_and_case() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    };
    assert_eq!(
        proxy_from_env(env(&[("HTTP_PROXY", "http://web:3128")])),
        Some(proxy(ProxyKind::Http, "web", 3128))
    );
    assert_eq!(
        proxy_from_env(env(&[
            ("http_proxy", "http://web:3128"),
            ("all_proxy", "socks5://s:1080"),
        ])),
        Some(proxy(ProxyKind::Socks5, "s", 1080))
    );
    assert_eq!(proxy_from_env(env(&[("http_proxy", " ")])), None);
    assert_eq!(proxy_from_env(env(&[])), None);
}

#[test]
fn gsettings_manual_mode_only() {
    let settings = |mode: &'static str| {
        move |schema: &str, key: &str| {
            Some(
                match (schema, key) {
                    ("org.gnome.system.proxy", "mode") => mode,
                    ("org.gnome.system.proxy.socks", "host") => "''\n",
                    ("org.gnome.system.proxy.socks", "port") => "0\n",
                    ("org.gnome.system.proxy.https", "host") => "'gate.lan'\n",
                    ("org.gnome.system.proxy.https", "port") => "8443\n",
                    _ => return None,
                }
                .to_string(),
            )
        }
    };
    assert_eq!(
        proxy_from_gsettings(settings("'manual'\n")),
        Some(proxy(ProxyKind::Http, "gate.lan", 8443))
    );
    assert_eq!(proxy_from_gsettings(settings("'auto'\n")), None);
    assert_eq!(proxy_from_gsettings(settings("'none'\n")), None);
}

#[test]
fn plan_adds_enables_edits_and_removes() {
    let a = proxy(ProxyKind::Socks5, "a", 1080);
    let b = proxy(ProxyKind::Http, "b", 8080);
    let found = Detected::Proxy(a.clone());
    assert_eq!(
        plan_system_proxy(true, false, &found, &[]),
        Some(SystemProxyStep::Add(a.clone()))
    );
    assert_eq!(
        plan_system_proxy(true, false, &found, &[entry(4, a.clone(), false)]),
        Some(SystemProxyStep::Enable { id: 4 })
    );
    assert_eq!(
        plan_system_proxy(true, false, &found, &[entry(4, a.clone(), true)]),
        None
    );
    assert_eq!(
        plan_system_proxy(true, false, &found, &[entry(4, b.clone(), true)]),
        Some(SystemProxyStep::Edit { id: 4, proxy: a })
    );
    assert_eq!(
        plan_system_proxy(true, false, &Detected::Direct, &[entry(4, b.clone(), true)]),
        Some(SystemProxyStep::Remove { id: 4 })
    );
}

#[test]
fn plan_leaves_custom_and_disabled_modes_alone() {
    let a = proxy(ProxyKind::Socks5, "a", 1080);
    let found = Detected::Proxy(a.clone());
    assert_eq!(plan_system_proxy(false, false, &found, &[]), None);
    assert_eq!(plan_system_proxy(true, true, &found, &[]), None);
    assert_eq!(
        plan_system_proxy(true, true, &found, &[entry(4, a.clone(), false)]),
        Some(SystemProxyStep::Remove { id: 4 }),
        "a custom proxy replaces the system one"
    );
    assert_eq!(
        plan_system_proxy(false, false, &found, &[entry(4, a, true)]),
        Some(SystemProxyStep::Remove { id: 4 })
    );
}

#[test]
fn plan_waits_for_the_first_read_and_drops_duplicates() {
    let a = proxy(ProxyKind::Socks5, "a", 1080);
    assert_eq!(
        plan_system_proxy(
            true,
            false,
            &Detected::Unknown,
            &[entry(4, a.clone(), true)]
        ),
        None
    );
    assert_eq!(
        plan_system_proxy(
            true,
            false,
            &Detected::Unknown,
            &[entry(4, a.clone(), true), entry(7, a.clone(), false)]
        ),
        Some(SystemProxyStep::Remove { id: 7 })
    );
    assert!(is_system_entry(&entry(1, a.clone(), false)));
    let mut custom = entry(1, a, false);
    custom.comment = "work".into();
    assert!(!is_system_entry(&custom));
}

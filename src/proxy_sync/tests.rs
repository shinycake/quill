use super::*;
use crate::calls::proxy::ProxyKind;

fn socks(server: &str, password: &str) -> ProxyDraft {
    ProxyDraft {
        kind: ProxyKind::Socks5,
        server: server.into(),
        port: 1080,
        username: "u".into(),
        password: password.into(),
        secret: String::new(),
        path: String::new(),
        http_only: false,
    }
}

fn entry(id: i32, proxy: ProxyDraft, enabled: bool) -> ProxyEntry {
    ProxyEntry {
        id,
        last_used_date: 0,
        is_enabled: enabled,
        comment: String::new(),
        proxy,
    }
}

#[test]
fn matching_lists_need_nothing_whatever_the_ids() {
    let target = [
        entry(1, socks("a", "p"), true),
        entry(2, socks("b", "p"), false),
    ];
    let current = [
        entry(7, socks("b", "p"), false),
        entry(9, socks("a", "p"), true),
    ];
    assert!(plan_proxy_sync(&target, &current).is_empty());
    assert!(plan_proxy_sync(&[], &[]).is_empty());
}

#[test]
fn missing_proxies_are_added_and_extra_ones_removed() {
    let target = [
        entry(1, socks("a", "p"), false),
        entry(2, socks("b", "p"), true),
    ];
    let current = [
        entry(5, socks("a", "p"), false),
        entry(6, socks("old", "p"), false),
    ];
    assert_eq!(
        plan_proxy_sync(&target, &current),
        vec![
            ProxySyncStep::Remove { id: 6 },
            ProxySyncStep::Add {
                proxy: socks("b", "p"),
                comment: String::new(),
                enable: true,
            },
        ]
    );
}

#[test]
fn a_changed_login_is_a_different_proxy() {
    let target = [entry(1, socks("a", "new"), false)];
    let current = [entry(4, socks("a", "old"), false)];
    assert_eq!(
        plan_proxy_sync(&target, &current),
        vec![
            ProxySyncStep::Remove { id: 4 },
            ProxySyncStep::Add {
                proxy: socks("a", "new"),
                comment: String::new(),
                enable: false,
            },
        ]
    );
}

#[test]
fn the_selection_follows_the_app() {
    let a = socks("a", "p");
    let b = socks("b", "p");
    // Another proxy selected here: select the app's.
    assert_eq!(
        plan_proxy_sync(
            &[entry(1, a.clone(), true), entry(2, b.clone(), false)],
            &[entry(5, a.clone(), false), entry(6, b.clone(), true)],
        ),
        vec![ProxySyncStep::Enable { id: 5 }]
    );
    // The app connects directly.
    assert_eq!(
        plan_proxy_sync(&[entry(1, a.clone(), false)], &[entry(5, a.clone(), true)]),
        vec![ProxySyncStep::Disable]
    );
    // Direct here too: nothing to do.
    assert!(plan_proxy_sync(&[entry(1, a.clone(), false)], &[entry(5, a, false)]).is_empty());
}

#[test]
fn removing_the_enabled_extra_needs_no_separate_disable() {
    let target = [entry(1, socks("a", "p"), false)];
    let current = [
        entry(5, socks("a", "p"), false),
        entry(6, socks("x", "p"), true),
    ];
    assert_eq!(
        plan_proxy_sync(&target, &current),
        vec![ProxySyncStep::Remove { id: 6 }]
    );
}

//! Proxy settings driver flow (`parity:proxy-settings`) with TDLib
//! answers recorded as JSON.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::proxy::{PingStatus, ProxyPrefs};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ConnectionState;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Driver = ConnectDriver<Arc<RecordingSender>>;

struct Rig {
    driver: Driver,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
}

impl Rig {
    fn new() -> Self {
        let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        // No authorization update: proxy requests must work before sign-in.
        let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        Self {
            driver,
            recorder,
            seq: AtomicU64::new(0),
            sink,
        }
    }

    fn feed(&mut self, value: Value) {
        self.driver
            .ingest(copy_and_parse(&value.to_string(), &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    /// Answer the most recent request of `type_name` with `body` (its
    /// `@extra` is echoed, like TDLib does).
    fn answer(&mut self, type_name: &str, mut body: Value) {
        body["@extra"] = sent_request(&self.recorder, type_name)["@extra"].clone();
        self.feed(body);
    }

    fn count(&self, type_name: &str) -> usize {
        self.recorder
            .snapshot()
            .iter()
            .filter(|json| serde_json::from_str::<Value>(json).unwrap()["@type"] == type_name)
            .count()
    }

    fn load_list(&mut self, ids: &[(i32, bool)]) {
        self.driver.fetch_proxies().unwrap();
        self.answer("getProxies", proxies_json(ids));
    }
}

fn proxies_json(ids: &[(i32, bool)]) -> Value {
    let proxies: Vec<Value> = ids
        .iter()
        .map(|(id, enabled)| {
            json!({"@type": "addedProxy", "id": id, "last_used_date": 0, "is_enabled": enabled,
                "comment": "",
                "proxy": {"@type": "proxy", "server": format!("p{id}.example"), "port": 1080,
                    "type": {"@type": "proxyTypeSocks5", "username": "", "password": ""}}})
        })
        .collect();
    json!({"@type": "addedProxies", "proxies": proxies})
}

#[test]
fn the_list_is_fetched_once_before_authorization() {
    let mut rig = Rig::new();
    assert!(rig.driver.proxy_tick(0));
    assert_eq!(rig.count("getProxies"), 1);
    assert!(
        !rig.driver.proxy_tick(100),
        "an in-flight fetch is not repeated"
    );
    rig.answer("getProxies", proxies_json(&[(1, true), (2, false)]));
    assert_eq!(rig.driver.session.settings.proxy.entries().len(), 2);
    assert!(rig.driver.session.settings.proxy.shield());
    assert!(!rig.driver.proxy_tick(200));
    assert_eq!(rig.count("getProxies"), 1);
}

#[test]
fn a_mutation_refetches_the_authoritative_list() {
    let mut rig = Rig::new();
    let mut draft = crate::proxy::ProxyDraft::empty(crate::calls::proxy::ProxyKind::Socks5);
    draft.server = "new.example".into();
    draft.port = 9050;
    rig.driver.add_proxy(&draft, true, "").unwrap();
    assert_eq!(sent_request(&rig.recorder, "addProxy")["enable"], true);
    // One mutation at a time.
    assert!(rig.driver.disable_proxy().is_err());
    rig.answer(
        "addProxy",
        json!({"@type": "addedProxy", "id": 7, "last_used_date": 0,
            "is_enabled": true, "comment": "",
            "proxy": {"@type": "proxy", "server": "new.example", "port": 9050,
                "type": {"@type": "proxyTypeSocks5", "username": "", "password": ""}}}),
    );
    // The ack marked the list stale and ingest refetched on the same turn.
    assert_eq!(rig.count("getProxies"), 1);
    rig.answer("getProxies", proxies_json(&[(7, true)]));
    assert_eq!(
        rig.driver.session.settings.proxy.enabled().map(|p| p.id),
        Some(7)
    );
    assert!(
        !rig.driver.session.settings.proxy.stale && !rig.driver.session.settings.proxy.mutating
    );
}

#[test]
fn ok_acks_refetch_and_errors_surface_without_touching_the_list() {
    let mut rig = Rig::new();
    rig.load_list(&[(1, true), (2, false)]);
    assert!(
        rig.driver.enable_proxy(99).is_err(),
        "unknown ids never leave"
    );
    rig.driver.enable_proxy(2).unwrap();
    assert_eq!(sent_request(&rig.recorder, "enableProxy")["proxy_id"], 2);
    rig.answer("enableProxy", json!({"@type": "ok"}));
    assert_eq!(rig.count("getProxies"), 2);
    rig.answer("getProxies", proxies_json(&[(1, false), (2, true)]));
    rig.driver.remove_proxy(1).unwrap();
    rig.answer(
        "removeProxy",
        json!({"@type": "error", "code": 400, "message": "PROXY_NOT_FOUND"}),
    );
    let proxy = &rig.driver.session.settings.proxy;
    assert!(!proxy.mutating);
    assert!(
        proxy
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("Could not change the proxy"))
    );
    assert_eq!(proxy.entries().len(), 2, "never optimistic");
}

#[test]
fn ping_answers_and_failures_set_the_row_status() {
    let mut rig = Rig::new();
    rig.load_list(&[(1, false), (2, false)]);
    rig.driver.ping_listed_proxy(1).unwrap();
    assert_eq!(
        rig.driver.session.settings.proxy.pings.get(&1),
        Some(&PingStatus::Checking)
    );
    rig.answer("pingProxy", json!({"@type": "seconds", "seconds": 0.0423}));
    assert_eq!(
        rig.driver.session.settings.proxy.pings.get(&1),
        Some(&PingStatus::Available(42))
    );
    rig.driver.ping_listed_proxy(2).unwrap();
    rig.answer(
        "pingProxy",
        json!({"@type": "error", "code": 400, "message": "Request timeout"}),
    );
    assert_eq!(
        rig.driver.session.settings.proxy.pings.get(&2),
        Some(&PingStatus::Unavailable)
    );
}

#[test]
fn prefer_ipv6_follows_the_ack_and_the_option_update() {
    let mut rig = Rig::new();
    rig.driver.set_prefer_ipv6(true).unwrap();
    assert_eq!(
        sent_request(&rig.recorder, "setOption")["name"],
        "prefer_ipv6"
    );
    assert!(
        !rig.driver.session.settings.proxy.prefer_ipv6,
        "never optimistic"
    );
    rig.answer("setOption", json!({"@type": "ok"}));
    assert!(rig.driver.session.settings.proxy.prefer_ipv6);
    rig.feed(json!({"@type": "updateOption", "name": "prefer_ipv6",
        "value": {"@type": "optionValueBoolean", "value": false}}));
    assert!(!rig.driver.session.settings.proxy.prefer_ipv6);
}

#[test]
fn auto_switch_probes_then_enables_a_working_proxy() {
    let mut rig = Rig::new();
    rig.load_list(&[(1, true), (2, false), (3, false)]);
    rig.driver.session.settings.proxy.prefs = ProxyPrefs {
        auto_switch: true,
        auto_switch_secs: 10,
        ..ProxyPrefs::default()
    };
    rig.driver.session.connection = ConnectionState::ConnectingToProxy;
    assert!(!rig.driver.proxy_tick(1_000));
    assert!(!rig.driver.proxy_tick(10_999));
    assert!(
        rig.driver.proxy_tick(11_000),
        "timeout reached: probe the next proxy"
    );
    assert_eq!(rig.count("pingProxy"), 1);
    rig.answer("pingProxy", json!({"@type": "seconds", "seconds": 0.1}));
    assert!(rig.driver.proxy_tick(12_000));
    assert_eq!(sent_request(&rig.recorder, "enableProxy")["proxy_id"], 2);
    // Back online: the machine stands down.
    rig.driver.session.connection = ConnectionState::Ready;
    assert!(!rig.driver.proxy_tick(30_000));
    assert_eq!(rig.count("pingProxy"), 1);
}

/// A background account converges on the app's list (the on-screen
/// account's) one change at a time: fetch, remove the extra, add the
/// missing one enabled, and mirror "prefer IPv6".
#[test]
fn a_background_account_takes_the_apps_proxy_list() {
    // The app's list: p2 enabled (TDLib id 20 on the account on screen).
    let mut screen = Rig::new();
    screen.load_list(&[(2, true)]);
    let mut target = screen.driver.session.settings.proxy.entries().to_vec();
    target[0].id = 20;

    let mut rig = Rig::new();
    assert!(
        rig.driver.sync_proxies_to(&target, true),
        "first: getProxies"
    );
    assert_eq!(rig.count("getProxies"), 1);
    assert!(
        !rig.driver.sync_proxies_to(&target, true),
        "waits for the list"
    );
    rig.answer("getProxies", proxies_json(&[(1, false)]));

    assert!(rig.driver.sync_proxies_to(&target, true));
    assert_eq!(
        sent_request(&rig.recorder, "setOption")["name"],
        "prefer_ipv6"
    );
    assert_eq!(rig.count("removeProxy"), 1);
    assert_eq!(sent_request(&rig.recorder, "removeProxy")["proxy_id"], 1);
    // One change at a time.
    assert!(!rig.driver.sync_proxies_to(&target, true));
    rig.answer("removeProxy", json!({"@type": "ok"}));
    assert_eq!(rig.count("getProxies"), 2, "the change refetched the list");
    rig.answer("getProxies", proxies_json(&[]));

    assert!(rig.driver.sync_proxies_to(&target, true));
    let add = sent_request(&rig.recorder, "addProxy");
    assert_eq!(add["proxy"]["server"], "p2.example");
    assert_eq!(add["enable"], true);
    rig.answer(
        "addProxy",
        json!({"@type": "addedProxy", "id": 3, "last_used_date": 0, "is_enabled": true,
            "comment": "",
            "proxy": {"@type": "proxy", "server": "p2.example", "port": 1080,
                "type": {"@type": "proxyTypeSocks5", "username": "", "password": ""}}}),
    );
    rig.answer("getProxies", proxies_json(&[(2, true)]));
    rig.answer("setOption", json!({"@type": "ok"}));
    assert!(
        !rig.driver.sync_proxies_to(&target, true),
        "in step: nothing more to send"
    );
    assert_eq!(rig.count("addProxy"), 1);
}

fn system_entry(id: i32, server: &str, enabled: bool) -> Value {
    json!({"@type": "addedProxy", "id": id, "last_used_date": 0, "is_enabled": enabled,
        "comment": crate::system_proxy::SYSTEM_PROXY_COMMENT,
        "proxy": {"@type": "proxy", "server": server, "port": 1080,
            "type": {"@type": "proxyTypeSocks5", "username": "", "password": ""}}})
}

fn detected(server: &str) -> crate::system_proxy::Detected {
    let mut draft = crate::proxy::ProxyDraft::empty(crate::calls::proxy::ProxyKind::Socks5);
    draft.server = server.into();
    draft.port = 1080;
    crate::system_proxy::Detected::Proxy(draft)
}

#[test]
fn the_system_proxy_is_added_hidden_and_followed() {
    let mut rig = Rig::new();
    assert!(
        !rig.driver.system_proxy_tick(&detected("sys.lan")),
        "nothing before the list is known"
    );
    rig.load_list(&[(1, false)]);
    assert!(rig.driver.system_proxy_tick(&detected("sys.lan")));
    let add = sent_request(&rig.recorder, "addProxy");
    assert_eq!(add["enable"], true);
    assert_eq!(add["comment"], crate::system_proxy::SYSTEM_PROXY_COMMENT);
    assert_eq!(add["proxy"]["server"], "sys.lan");
    rig.answer("addProxy", system_entry(5, "sys.lan", true));
    let mut list = proxies_json(&[(1, false)]);
    list["proxies"]
        .as_array_mut()
        .unwrap()
        .push(system_entry(5, "sys.lan", true));
    rig.answer("getProxies", list);
    let proxy = &rig.driver.session.settings.proxy;
    assert_eq!(proxy.entries().len(), 1, "never in the visible list");
    assert!(proxy.enabled().is_none() && !proxy.shield());
    assert_eq!(proxy.mode(), crate::proxy::ProxyMode::System);
    assert_eq!(proxy.all_entries().map(|all| all.len()), Some(2));
    assert!(!rig.driver.system_proxy_tick(&detected("sys.lan")));
    // The OS proxy changed: the entry follows.
    assert!(rig.driver.system_proxy_tick(&detected("other.lan")));
    let edit = sent_request(&rig.recorder, "editProxy");
    assert_eq!(edit["proxy_id"], 5);
    assert_eq!(edit["proxy"]["server"], "other.lan");
}

#[test]
fn disabling_drops_the_system_proxy() {
    let mut rig = Rig::new();
    let mut list = proxies_json(&[]);
    list["proxies"]
        .as_array_mut()
        .unwrap()
        .push(system_entry(5, "sys.lan", true));
    rig.driver.fetch_proxies().unwrap();
    rig.answer("getProxies", list);
    rig.driver.set_proxy_mode(false).unwrap();
    assert_eq!(rig.count("disableProxy"), 1);
    assert!(!rig.driver.session.settings.proxy.prefs.use_system);
    assert_eq!(
        rig.driver.session.settings.proxy.mode(),
        crate::proxy::ProxyMode::Disabled
    );
    rig.answer("disableProxy", json!({"@type": "ok"}));
    rig.answer("getProxies", {
        let mut list = proxies_json(&[]);
        list["proxies"]
            .as_array_mut()
            .unwrap()
            .push(system_entry(5, "sys.lan", false));
        list
    });
    assert!(rig.driver.system_proxy_tick(&detected("sys.lan")));
    assert_eq!(sent_request(&rig.recorder, "removeProxy")["proxy_id"], 5);
}

#[test]
fn a_refused_system_proxy_waits_and_stays_out_of_the_box() {
    let mut rig = Rig::new();
    rig.load_list(&[]);
    assert!(rig.driver.system_proxy_tick(&detected("bad.lan")));
    rig.answer(
        "addProxy",
        json!({"@type": "error", "code": 400, "message": "Wrong server name"}),
    );
    assert!(
        !rig.driver.system_proxy_tick(&detected("bad.lan")),
        "not sent again"
    );
    assert_eq!(rig.count("addProxy"), 1);
    assert!(rig.driver.session.settings.proxy.error.is_none());
    // A new OS setting is tried at once.
    assert!(rig.driver.system_proxy_tick(&detected("good.lan")));
    assert_eq!(rig.count("addProxy"), 2);
}

#[test]
fn a_web_proxy_carrier_is_refused_and_the_row_says_not_available() {
    let mut rig = Rig::new();
    rig.load_list(&[(1, true)]);
    rig.feed(json!({"@type": "updateWebProxyCarrierOpened", "carrier": {
        "@type": "webProxyCarrier", "id": 9, "proxy_id": 1, "url": "https://x.example/",
        "origin": "https://x.example", "path_prefix": "/",
        "bridge_object_name": "TelegramWebProxy", "is_temporary": false}}));
    let report = sent_request(&rig.recorder, "reportWebProxyCarrierError");
    assert_eq!(report["carrier_id"], 9);
    assert_eq!(report["error"]["@type"], "webProxyCarrierErrorUnsupported");
    assert_eq!(rig.driver.session.settings.proxy.carrier_failed, Some(1));

    // A temporary carrier (a ping or test) is refused without marking the proxy.
    rig.driver.session.settings.proxy.carrier_failed = None;
    rig.feed(json!({"@type": "updateWebProxyCarrierOpened", "carrier": {
        "@type": "webProxyCarrier", "id": 10, "proxy_id": 0, "url": "https://x.example/",
        "origin": "https://x.example", "path_prefix": "/",
        "bridge_object_name": "TelegramWebProxy", "is_temporary": true}}));
    let report = sent_request(&rig.recorder, "reportWebProxyCarrierError");
    assert_eq!(report["error"]["@type"], "webProxyCarrierErrorRefused");
    assert_eq!(rig.driver.session.settings.proxy.carrier_failed, None);
    assert_eq!(rig.count("reportWebProxyCarrierError"), 2);
}

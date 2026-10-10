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

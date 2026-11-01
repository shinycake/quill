use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::net_watch::NetworkState;
use crate::platform::MemorySecretStore;
use crate::proxy::{ProxyDraft, ProxyEntry};
use crate::state::Session;
use crate::telegram::envelope::ConnectionState;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

fn network(epoch: u64, available: bool) -> NetworkState {
    NetworkState { epoch, available }
}

type Driver = ConnectDriver<Arc<RecordingSender>>;

fn driver() -> (Driver, Arc<RecordingSender>) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), sink);
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    driver.kickoff().unwrap();
    (driver, recorder)
}

fn told(recorder: &RecordingSender) -> Vec<String> {
    recorder
        .snapshot()
        .iter()
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .filter(|request| request["@type"] == "setNetworkType")
        .map(|request| request["type"]["@type"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn nothing_is_said_until_something_changes_and_then_once() {
    let (mut driver, recorder) = driver();
    let now = Instant::now();
    let background = TickScope::Background;
    driver.poll_ticks(background, now, network(4, true));
    driver.poll_ticks(background, now, network(4, true));
    assert!(told(&recorder).is_empty(), "the start is not a change");

    assert!(
        driver
            .poll_ticks(background, now, network(5, true))
            .progressed
    );
    driver.poll_ticks(background, now, network(5, true));
    assert_eq!(told(&recorder), ["networkTypeOther"]);

    driver.poll_ticks(background, now, network(6, false));
    driver.poll_ticks(background, now, network(7, true));
    assert_eq!(
        told(&recorder),
        ["networkTypeOther", "networkTypeNone", "networkTypeOther"]
    );
}

#[test]
fn a_client_started_offline_is_told_at_once() {
    let (mut driver, recorder) = driver();
    let scope = TickScope::OnScreen {
        settings_open: false,
    };
    driver.poll_ticks(scope, Instant::now(), network(0, false));
    assert_eq!(told(&recorder), ["networkTypeNone"]);
}

#[test]
fn a_client_without_parameters_is_told_nothing_yet() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), sink);
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    driver.poll_ticks(TickScope::Background, Instant::now(), network(0, false));
    assert!(recorder.snapshot().is_empty());
}

#[test]
fn a_closing_client_is_told_nothing() {
    let (mut driver, recorder) = driver();
    let now = Instant::now();
    driver.poll_ticks(TickScope::Background, now, network(0, true));
    driver.request_close().unwrap();
    driver.poll_ticks(TickScope::Background, now, network(1, true));
    assert!(told(&recorder).is_empty());
}

fn enabled_proxy() -> ProxyEntry {
    ProxyEntry {
        id: 1,
        last_used_date: 0,
        is_enabled: true,
        comment: String::new(),
        proxy: ProxyDraft::empty(crate::calls::proxy::ProxyKind::Socks5),
    }
}

#[test]
fn a_proxy_keeps_tdlib_trying_with_no_route_out() {
    let (mut driver, recorder) = driver();
    let now = Instant::now();
    driver.session.settings.proxy.list = Some(vec![enabled_proxy()]);
    driver.poll_ticks(TickScope::Background, now, network(0, false));
    assert!(told(&recorder).is_empty(), "the start is not a change");
    driver.poll_ticks(TickScope::Background, now, network(1, false));
    assert_eq!(told(&recorder), ["networkTypeOther"]);
}

#[test]
fn a_proxy_known_after_the_first_poll_gets_tdlib_going_again() {
    let (mut driver, recorder) = driver();
    let now = Instant::now();
    let background = TickScope::Background;
    driver.poll_ticks(background, now, network(0, false));
    assert_eq!(told(&recorder), ["networkTypeNone"]);

    driver.session.settings.proxy.system = vec![enabled_proxy()];
    driver.poll_ticks(background, now, network(0, false));
    driver.poll_ticks(background, now, network(0, false));
    assert_eq!(told(&recorder), ["networkTypeNone", "networkTypeOther"]);

    driver.session.settings.proxy.system.clear();
    driver.poll_ticks(background, now, network(0, false));
    assert_eq!(
        told(&recorder),
        ["networkTypeNone", "networkTypeOther", "networkTypeNone"]
    );
}

#[test]
fn only_a_client_opening_a_connection_has_the_os_proxy_read_again() {
    let (mut driver, _recorder) = driver();
    let scope = TickScope::OnScreen {
        settings_open: false,
    };
    let start = Instant::now();
    driver.session.connection = ConnectionState::Ready;
    driver.poll_ticks(scope, start, network(0, true));
    assert_eq!(driver.system_proxy_asked, None);

    driver.session.connection = ConnectionState::Connecting;
    driver.poll_ticks(scope, start, network(0, true));
    assert_eq!(driver.system_proxy_asked, Some(start));

    driver.poll_ticks(scope, start + Duration::from_secs(1), network(0, true));
    assert_eq!(driver.system_proxy_asked, Some(start));

    let later = start + Duration::from_secs(15);
    driver.poll_ticks(scope, later, network(0, true));
    assert_eq!(driver.system_proxy_asked, Some(later));
}

#[test]
fn the_label_appears_once_tdlib_stops_for_the_network() {
    let (mut driver, _recorder) = driver();
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let waiting = crate::telegram::client::copy_and_parse(
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateWaitingForNetwork"}}"#,
        &AtomicU64::new(0),
        &sink,
    )
    .unwrap();
    driver.ingest(waiting).unwrap();
    let indicator = crate::state::connection_indicator(driver.session.connection);
    assert_eq!(indicator.unwrap().label(), "Waiting for network…");
}

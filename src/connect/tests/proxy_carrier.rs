//! Web proxy carriers through the driver (`parity:data-proxy-web`) with a
//! fake host: open, relay both ways, failure, close, quit while open.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::proxy_carrier::{
    CarrierError, CarrierHost, CarrierMessage, CarrierPhase, CarrierRequest, HostEvent,
};
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

type Driver = ConnectDriver<Arc<RecordingSender>>;

/// What the fake host was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Open(i32),
    Deliver(i32, String),
    Close(i32, bool),
    Shutdown,
}

#[derive(Default)]
struct Log {
    calls: Vec<Call>,
    events: Vec<(i32, HostEvent)>,
    refuse_open: Option<CarrierError>,
}

struct FakeHost(Arc<Mutex<Log>>);

impl CarrierHost for FakeHost {
    fn available(&self) -> bool {
        true
    }
    fn open(&mut self, request: &CarrierRequest) -> Result<(), CarrierError> {
        let mut log = self.0.lock().unwrap();
        log.calls.push(Call::Open(request.id));
        match log.refuse_open.clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn deliver(&mut self, carrier_id: i32, data: String) {
        self.0
            .lock()
            .unwrap()
            .calls
            .push(Call::Deliver(carrier_id, data));
    }
    fn close(&mut self, carrier_id: i32, notify_page: bool) {
        self.0
            .lock()
            .unwrap()
            .calls
            .push(Call::Close(carrier_id, notify_page));
    }
    fn poll(&mut self) -> Vec<(i32, HostEvent)> {
        std::mem::take(&mut self.0.lock().unwrap().events)
    }
    fn shutdown(&mut self) {
        self.0.lock().unwrap().calls.push(Call::Shutdown);
    }
}

struct Rig {
    driver: Driver,
    recorder: Arc<RecordingSender>,
    log: Arc<Mutex<Log>>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
}

impl Rig {
    fn new() -> Self {
        let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink.clone());
        let mut driver =
            ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let log = Arc::new(Mutex::new(Log::default()));
        driver.set_carrier_host(Box::new(FakeHost(log.clone())));
        Self {
            driver,
            recorder,
            log,
            seq: AtomicU64::new(0),
            sink,
        }
    }

    fn feed(&mut self, value: Value) {
        self.driver
            .ingest(copy_and_parse(&value.to_string(), &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn open(&mut self, id: i32, proxy_id: i32) {
        self.feed(json!({"@type": "updateWebProxyCarrierOpened", "carrier": {
            "@type": "webProxyCarrier", "id": id, "proxy_id": proxy_id,
            "url": "https://relay.example.com/app/?bridge=cap#android=nonce",
            "origin": "https://relay.example.com", "path_prefix": "/app/",
            "bridge_object_name": "TelegramWebProxy", "is_temporary": proxy_id == 0}}));
    }

    fn host_says(&mut self, carrier_id: i32, event: HostEvent) {
        self.log.lock().unwrap().events.push((carrier_id, event));
        self.driver.carrier_tick();
    }

    fn calls(&self) -> Vec<Call> {
        std::mem::take(&mut self.log.lock().unwrap().calls)
    }

    fn sent(&self, type_name: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .iter()
            .map(|json| serde_json::from_str::<Value>(json).unwrap())
            .filter(|value| value["@type"] == type_name)
            .collect()
    }

    fn phase(&self, id: i32) -> Option<CarrierPhase> {
        self.driver
            .session
            .settings
            .proxy
            .carriers
            .open
            .get(&id)
            .map(|carrier| carrier.phase)
    }
}

#[test]
fn a_carrier_opens_relays_both_ways_and_closes_with_the_page_told() {
    let mut rig = Rig::new();
    rig.open(7, 1);
    assert_eq!(rig.calls(), vec![Call::Open(7)]);
    assert_eq!(rig.phase(7), Some(CarrierPhase::Opening));
    assert!(rig.sent("reportWebProxyCarrierError").is_empty());

    // Page → TDLib: the init text, then binary frames, base64 as is.
    rig.host_says(
        7,
        HostEvent::Message(CarrierMessage::Text(
            r#"{"t":"tproxy-android-init","v":1,"nonce":"nonce"}"#.into(),
        )),
    );
    rig.host_says(
        7,
        HostEvent::Message(CarrierMessage::Binary("EQAAAA==".into())),
    );
    let sent = rig.sent("processWebProxyCarrierMessage");
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0]["carrier_id"], 7);
    assert_eq!(sent[0]["message"]["@type"], "webProxyCarrierMessageText");
    assert_eq!(
        sent[0]["message"]["text"],
        r#"{"t":"tproxy-android-init","v":1,"nonce":"nonce"}"#
    );
    assert_eq!(sent[1]["message"]["@type"], "webProxyCarrierMessageBinary");
    assert_eq!(sent[1]["message"]["data"], "EQAAAA==");
    assert_eq!(rig.phase(7), Some(CarrierPhase::Relaying));

    // TDLib → page, in order; data for unknown carriers goes nowhere.
    rig.feed(json!({"@type": "updateWebProxyCarrierData", "carrier_id": 7, "data": "EAAAAAE="}));
    rig.feed(json!({"@type": "updateWebProxyCarrierData", "carrier_id": 7, "data": "AgAAAA=="}));
    rig.feed(json!({"@type": "updateWebProxyCarrierData", "carrier_id": 99, "data": "AAAA"}));
    assert_eq!(
        rig.calls(),
        vec![
            Call::Deliver(7, "EAAAAAE=".into()),
            Call::Deliver(7, "AgAAAA==".into())
        ]
    );

    // The page has posted, so it hears {"t":"close"} first.
    rig.feed(json!({"@type": "updateWebProxyCarrierClosed", "carrier_id": 7}));
    assert_eq!(rig.calls(), vec![Call::Close(7, true)]);
    assert_eq!(rig.phase(7), None);

    // Messages after the close are ignored.
    rig.host_says(7, HostEvent::Message(CarrierMessage::Binary("AAAA".into())));
    assert_eq!(rig.sent("processWebProxyCarrierMessage").len(), 2);
}

#[test]
fn a_silent_page_is_closed_without_the_close_message() {
    let mut rig = Rig::new();
    rig.open(3, 1);
    rig.calls();
    rig.feed(json!({"@type": "updateWebProxyCarrierClosed", "carrier_id": 3}));
    assert_eq!(rig.calls(), vec![Call::Close(3, false)]);
}

#[test]
fn a_host_failure_is_reported_once_and_marks_the_row() {
    let mut rig = Rig::new();
    rig.open(5, 2);
    rig.calls();
    rig.host_says(5, HostEvent::Failed(CarrierError::Terminated));
    let reports = rig.sent("reportWebProxyCarrierError");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["carrier_id"], 5);
    assert_eq!(
        reports[0]["error"]["@type"],
        "webProxyCarrierErrorTerminated"
    );
    assert_eq!(rig.calls(), vec![Call::Close(5, false)]);
    assert_eq!(rig.driver.session.settings.proxy.carrier_failed, Some(2));
    assert_eq!(rig.phase(5), Some(CarrierPhase::Failed));

    // A second failure, late messages and data are all dropped.
    rig.host_says(
        5,
        HostEvent::Failed(CarrierError::LoadFailed("again".into())),
    );
    rig.host_says(5, HostEvent::Message(CarrierMessage::Text("x".into())));
    rig.feed(json!({"@type": "updateWebProxyCarrierData", "carrier_id": 5, "data": "AAAA"}));
    assert_eq!(rig.sent("reportWebProxyCarrierError").len(), 1);
    assert!(rig.sent("processWebProxyCarrierMessage").is_empty());
    assert!(rig.calls().is_empty());

    // TDLib closes it and retries with a new carrier: the row goes back
    // to "connecting".
    rig.feed(json!({"@type": "updateWebProxyCarrierClosed", "carrier_id": 5}));
    assert!(rig.calls().is_empty());
    rig.open(6, 2);
    assert_eq!(rig.calls(), vec![Call::Open(6)]);
    assert_eq!(rig.driver.session.settings.proxy.carrier_failed, None);
}

#[test]
fn a_load_failure_keeps_its_message() {
    let mut rig = Rig::new();
    rig.open(8, 1);
    rig.host_says(
        8,
        HostEvent::Failed(CarrierError::LoadFailed(
            "the page tried to leave its origin".into(),
        )),
    );
    let report = &rig.sent("reportWebProxyCarrierError")[0];
    assert_eq!(report["error"]["@type"], "webProxyCarrierErrorLoadFailed");
    assert_eq!(
        report["error"]["message"],
        "the page tried to leave its origin"
    );
}

#[test]
fn a_host_that_cannot_start_the_view_reports_unsupported() {
    let mut rig = Rig::new();
    rig.log.lock().unwrap().refuse_open = Some(CarrierError::Unsupported);
    rig.open(4, 1);
    let reports = rig.sent("reportWebProxyCarrierError");
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0]["error"]["@type"],
        "webProxyCarrierErrorUnsupported"
    );
    assert_eq!(rig.calls(), vec![Call::Open(4), Call::Close(4, false)]);
    assert_eq!(rig.driver.session.settings.proxy.carrier_failed, Some(1));
}

#[test]
fn temporary_carriers_are_refused_even_with_a_host() {
    let mut rig = Rig::new();
    rig.open(11, 0);
    let reports = rig.sent("reportWebProxyCarrierError");
    assert_eq!(reports[0]["error"]["@type"], "webProxyCarrierErrorRefused");
    assert!(rig.calls().is_empty());
    assert_eq!(rig.driver.session.settings.proxy.carrier_failed, None);
}

#[test]
fn quitting_while_open_shuts_every_view_down() {
    let mut rig = Rig::new();
    rig.open(1, 1);
    rig.open(2, 1);
    rig.calls();
    crate::connect::closing::begin_close(&mut rig.driver);
    assert_eq!(rig.calls(), vec![Call::Shutdown]);
    assert!(rig.driver.session.settings.proxy.carriers.open.is_empty());
    // Whatever the host still reports is dropped.
    rig.host_says(1, HostEvent::Message(CarrierMessage::Binary("AAAA".into())));
    rig.host_says(2, HostEvent::Failed(CarrierError::Terminated));
    assert!(rig.sent("processWebProxyCarrierMessage").is_empty());
    assert!(rig.sent("reportWebProxyCarrierError").is_empty());
}

#[test]
fn logging_out_shuts_every_view_down() {
    let mut rig = Rig::new();
    rig.open(1, 1);
    rig.calls();
    rig.feed(json!({"@type": "updateAuthorizationState",
        "authorization_state": {"@type": "authorizationStateLoggingOut"}}));
    rig.driver.carrier_tick();
    assert!(rig.calls().contains(&Call::Shutdown));
    assert!(rig.driver.session.settings.proxy.carriers.open.is_empty());
}

#[test]
fn with_a_host_a_web_proxy_can_be_enabled() {
    let mut rig = Rig::new();
    let web =
        crate::proxy::ProxyDraft::web("relay.example.com/app", "8561944064fc730cbfa4473562d8ec59")
            .validated()
            .unwrap();
    rig.driver.add_proxy(&web, true, "").unwrap();
    let add = &rig.sent("addProxy")[0];
    assert_eq!(add["enable"], true);
    assert!(rig.driver.session.settings.proxy.can_enable(web.kind));
}

#[test]
fn the_url_never_shows_in_debug_output() {
    let mut rig = Rig::new();
    rig.log.lock().unwrap().refuse_open = None;
    rig.open(12, 1);
    let state = format!("{:?}", rig.driver.session.settings.proxy);
    assert!(!state.contains("bridge=cap"));
    let request = crate::proxy_carrier::CarrierRequest {
        url: crate::proxy_carrier::CarrierUrl::new("https://x.example/?bridge=cap".into()),
        ..Default::default()
    };
    assert!(!format!("{request:?}").contains("bridge=cap"));
}

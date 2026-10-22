//! Web proxy carriers through the driver (`parity:data-proxy-web`) with a
//! fake host: open, data to the page, failure, close, hosting turning off,
//! quit while open.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::proxy_carrier::{
    CARRIER_RELAY_EXTRA, CarrierError, CarrierHost, CarrierPhase, CarrierRequest,
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

struct Log {
    calls: Vec<Call>,
    failures: Vec<(i32, CarrierError)>,
    refuse_open: Option<CarrierError>,
    available: bool,
}

impl Default for Log {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            failures: Vec::new(),
            refuse_open: None,
            available: true,
        }
    }
}

struct FakeHost(Arc<Mutex<Log>>);

impl CarrierHost for FakeHost {
    fn available(&self) -> bool {
        self.0.lock().unwrap().available
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
    fn poll(&mut self) -> Vec<(i32, CarrierError)> {
        std::mem::take(&mut self.0.lock().unwrap().failures)
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
        self.feed(Self::opened(id, proxy_id));
    }

    fn opened(id: i32, proxy_id: i32) -> Value {
        json!({"@type": "updateWebProxyCarrierOpened", "carrier": {
            "@type": "webProxyCarrier", "id": id, "proxy_id": proxy_id,
            "url": "https://relay.example.com/app/?bridge=cap#android=nonce",
            "origin": "https://relay.example.com", "path_prefix": "/app/",
            "bridge_object_name": "TelegramWebProxy", "is_temporary": proxy_id == 0}})
    }

    fn host_fails(&mut self, carrier_id: i32, error: CarrierError) {
        self.log.lock().unwrap().failures.push((carrier_id, error));
        self.driver.carrier_tick();
    }

    fn calls(&self) -> Vec<Call> {
        std::mem::take(&mut self.log.lock().unwrap().calls)
    }

    /// The calls once the host's shutdown has run (on the teardown thread).
    fn calls_after_shutdown(&self) -> Vec<Call> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !self.log.lock().unwrap().calls.contains(&Call::Shutdown)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        self.calls()
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
fn a_carrier_opens_takes_data_in_order_and_closes_with_the_page_told() {
    let mut rig = Rig::new();
    rig.open(7, 1);
    assert_eq!(rig.calls(), vec![Call::Open(7)]);
    assert_eq!(rig.phase(7), Some(CarrierPhase::Live));
    assert!(rig.sent("reportWebProxyCarrierError").is_empty());

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

    // Whether the page posted is the helper's to know, so the page is
    // always offered {"t":"close"}.
    rig.feed(json!({"@type": "updateWebProxyCarrierClosed", "carrier_id": 7}));
    assert_eq!(rig.calls(), vec![Call::Close(7, true)]);
    assert_eq!(rig.phase(7), None);
}

#[test]
fn data_right_behind_the_open_reaches_the_host_after_it() {
    let mut rig = Rig::new();
    // Both parsed before the driver gets to either.
    let opened = copy_and_parse(&Rig::opened(3, 1).to_string(), &rig.seq, &rig.sink).unwrap();
    let data = copy_and_parse(
        &json!({"@type": "updateWebProxyCarrierData", "carrier_id": 3, "data": "AQID"}).to_string(),
        &rig.seq,
        &rig.sink,
    )
    .unwrap();
    rig.driver.ingest(opened).unwrap();
    rig.driver.ingest(data).unwrap();
    assert_eq!(
        rig.calls(),
        vec![Call::Open(3), Call::Deliver(3, "AQID".into())]
    );
}

#[test]
fn tdlib_answers_to_relayed_page_messages_are_dropped() {
    let mut rig = Rig::new();
    rig.open(7, 1);
    rig.calls();
    let before = rig.recorder.snapshot().len();
    let relay = CARRIER_RELAY_EXTRA.as_extra();
    rig.feed(json!({"@type": "ok", "@extra": relay}));
    rig.feed(json!({"@type": "error", "@extra": relay, "code": 400,
        "message": "Carrier not found"}));
    assert_eq!(rig.recorder.snapshot().len(), before);
    assert!(rig.calls().is_empty());
    assert_eq!(rig.phase(7), Some(CarrierPhase::Live));
}

#[test]
fn a_host_failure_is_reported_once_and_marks_the_row() {
    let mut rig = Rig::new();
    rig.open(5, 2);
    rig.calls();
    rig.host_fails(5, CarrierError::Terminated);
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

    // A second failure and late data are dropped.
    rig.host_fails(5, CarrierError::LoadFailed("again".into()));
    rig.feed(json!({"@type": "updateWebProxyCarrierData", "carrier_id": 5, "data": "AAAA"}));
    assert_eq!(rig.sent("reportWebProxyCarrierError").len(), 1);
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
    rig.host_fails(
        8,
        CarrierError::LoadFailed("the relay page answered HTTP 404".into()),
    );
    let report = &rig.sent("reportWebProxyCarrierError")[0];
    assert_eq!(report["error"]["@type"], "webProxyCarrierErrorLoadFailed");
    assert_eq!(
        report["error"]["message"],
        "the relay page answered HTTP 404"
    );
}

#[test]
fn a_host_that_cannot_start_the_view_reports_why() {
    let mut rig = Rig::new();
    rig.log.lock().unwrap().refuse_open = Some(CarrierError::Terminated);
    rig.open(4, 1);
    let reports = rig.sent("reportWebProxyCarrierError");
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0]["error"]["@type"],
        "webProxyCarrierErrorTerminated"
    );
    assert_eq!(rig.calls(), vec![Call::Open(4), Call::Close(4, false)]);
    assert_eq!(rig.driver.session.settings.proxy.carrier_failed, Some(1));
}

#[test]
fn a_host_that_finds_it_cannot_run_carriers_falls_back_to_the_refusal() {
    let mut rig = Rig::new();
    let web =
        crate::proxy::ProxyDraft::web("relay.example.com/app", "8561944064fc730cbfa4473562d8ec59")
            .validated()
            .unwrap();
    assert!(rig.driver.session.settings.proxy.can_enable(web.kind));
    // The capability check (or a carrier) found no web view here.
    rig.log.lock().unwrap().available = false;
    assert!(rig.driver.carrier_tick());
    assert!(!rig.driver.session.settings.proxy.can_enable(web.kind));
    rig.open(9, 1);
    assert!(rig.calls().is_empty());
    let reports = rig.sent("reportWebProxyCarrierError");
    assert_eq!(
        reports[0]["error"]["@type"],
        "webProxyCarrierErrorUnsupported"
    );
    // Nothing changes on the next tick.
    assert!(!rig.driver.carrier_tick());
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
    assert_eq!(rig.calls_after_shutdown(), vec![Call::Shutdown]);
    assert!(rig.driver.session.settings.proxy.carriers.open.is_empty());
    // Whatever the host still reports is dropped.
    rig.host_fails(2, CarrierError::Terminated);
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
    assert!(rig.calls_after_shutdown().contains(&Call::Shutdown));
    assert!(rig.driver.session.settings.proxy.carriers.open.is_empty());
}

/// The client stays logging out (then closed) for many ticks: the host
/// shuts down once, and the session knows no carrier can be hosted.
#[test]
fn the_host_shuts_down_once() {
    let mut rig = Rig::new();
    rig.open(1, 1);
    rig.calls();
    rig.feed(json!({"@type": "updateAuthorizationState",
        "authorization_state": {"@type": "authorizationStateLoggingOut"}}));
    for _ in 0..5 {
        rig.driver.carrier_tick();
    }
    rig.feed(json!({"@type": "updateAuthorizationState",
        "authorization_state": {"@type": "authorizationStateClosed"}}));
    for _ in 0..5 {
        rig.driver.carrier_tick();
    }
    crate::connect::closing::begin_close(&mut rig.driver);
    assert_eq!(rig.calls_after_shutdown(), vec![Call::Shutdown]);
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(rig.calls(), Vec::new());
    assert!(!rig.driver.session.settings.proxy.carriers.hosting);
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
    rig.open(12, 1);
    let state = format!("{:?}", rig.driver.session.settings.proxy);
    assert!(!state.contains("bridge=cap"));
    let request = crate::proxy_carrier::CarrierRequest {
        url: crate::proxy_carrier::CarrierUrl::new("https://x.example/?bridge=cap".into()),
        ..Default::default()
    };
    assert!(!format!("{request:?}").contains("bridge=cap"));
}

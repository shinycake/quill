#![allow(clippy::indexing_slicing)]
use super::*;

/// A client that answers each request at once, on the sending thread, the
/// way the receive loop would hand the answer over: by [`intercept`].
struct Answering {
    sent: Mutex<Vec<Value>>,
    /// What TDLib answers: `data` with the frame reversed, or an error.
    fail: bool,
    /// The request never reaches TDLib.
    refuse: bool,
}

impl Answering {
    fn new(fail: bool, refuse: bool) -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            fail,
            refuse,
        })
    }

    fn sent(&self) -> Vec<Value> {
        crate::locks::lock_or_recover(&self.sent).clone()
    }
}

impl CryptoLink for Answering {
    fn send(&self, request: &str) -> bool {
        if self.refuse {
            return false;
        }
        let request: Value = serde_json::from_str(request).expect("request is JSON");
        let extra = request["@extra"].clone();
        let answer = if self.fail {
            json!({"@type": "error", "code": 400, "message": "GROUPCALL_JOIN_MISSING", "@extra": extra, "@client_id": 3})
        } else {
            let mut frame = STANDARD
                .decode(request["data"].as_str().expect("data"))
                .expect("base64");
            frame.reverse();
            json!({"@type": "data", "data": STANDARD.encode(frame), "@extra": extra, "@client_id": 3})
        };
        crate::locks::lock_or_recover(&self.sent).push(request);
        assert!(intercept(&answer.to_string()));
        true
    }
}

#[test]
fn frames_are_dropped_until_the_call_is_joined() {
    let link = Answering::new(false, false);
    let crypto = GroupCallCrypto::new(link.clone());
    assert_eq!(crypto.encrypt(&[1, 2, 3], 1), None);
    assert_eq!(crypto.decrypt(42, &[1, 2, 3]), None);
    assert!(link.sent().is_empty());

    crypto.bind(77);
    crypto.unbind();
    assert_eq!(crypto.encrypt(&[1, 2, 3], 1), None);
    assert!(link.sent().is_empty());
}

#[test]
fn a_joined_call_encrypts_and_decrypts_through_tdlib() {
    let link = Answering::new(false, false);
    let crypto = GroupCallCrypto::new(link.clone());
    crypto.bind(77);
    assert_eq!(crypto.encrypt(&[1, 2, 3], 2), Some(vec![3, 2, 1]));
    assert_eq!(crypto.decrypt(42, &[4, 5]), Some(vec![5, 4]));

    let sent = link.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0]["@type"], "encryptGroupCallData");
    assert_eq!(sent[0]["group_call_id"], 77);
    assert_eq!(sent[0]["data_channel"]["@type"], "groupCallDataChannelMain");
    assert_eq!(sent[0]["unencrypted_prefix_size"], 2);
    assert_eq!(sent[1]["@type"], "decryptGroupCallData");
    assert_eq!(
        sent[1]["participant_id"],
        json!({"@type": "messageSenderUser", "user_id": 42})
    );
    assert_eq!(sent[1]["data_channel"]["@type"], "groupCallDataChannelMain");
    for request in &sent {
        assert!(
            request["@extra"]
                .as_str()
                .is_some_and(|extra| extra.starts_with(EXTRA_PREFIX))
        );
    }
}

#[test]
fn an_error_or_a_failed_send_drops_the_frame() {
    let crypto = GroupCallCrypto::new(Answering::new(true, false));
    crypto.bind(77);
    assert_eq!(crypto.encrypt(&[1], 0), None);
    assert_eq!(crypto.decrypt(42, &[1]), None);

    let crypto = GroupCallCrypto::new(Answering::new(false, true));
    crypto.bind(77);
    assert_eq!(crypto.encrypt(&[1], 0), None);
}

#[test]
fn frame_answers_are_taken_and_ui_envelopes_pass() {
    // An answer whose media thread already gave up is still taken.
    assert!(intercept(
        r#"{"@type":"data","data":"AQI=","@extra":"quill-e2e:18446744073709551615"}"#
    ));
    assert!(!intercept(r#"{"@type":"ok","@extra":17}"#));
    assert!(!intercept(r#"{"@type":"ok","@extra":"quill-agent:5"}"#));
    assert!(!intercept(
        r#"{"@type":"updateNewMessage","message":{"content":{"text":{"text":"quill-e2e:5"}}}}"#
    ));
}

/// A client that takes every request and never answers; it says when a
/// request went out, and records the request log's hiding.
struct Silent {
    sent: Mutex<Option<mpsc::Sender<()>>>,
    log: Mutex<Vec<bool>>,
}

impl Silent {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(None),
            log: Mutex::new(Vec::new()),
        })
    }

    fn log(&self) -> Vec<bool> {
        crate::locks::lock_or_recover(&self.log).clone()
    }
}

impl CryptoLink for Silent {
    fn send(&self, _request: &str) -> bool {
        if let Some(sent) = crate::locks::lock_or_recover(&self.sent).as_ref() {
            let _ = sent.send(());
        }
        true
    }

    fn hide_request_log(&self, hide: bool) {
        crate::locks::lock_or_recover(&self.log).push(hide);
    }
}

#[test]
fn unbinding_wakes_a_frame_waiting_on_tdlib() {
    let link = Silent::new();
    let (sent_tx, sent_rx) = mpsc::channel();
    *crate::locks::lock_or_recover(&link.sent) = Some(sent_tx);
    // No answer ever comes, and the wait would outlast the test run.
    let crypto = Arc::new(GroupCallCrypto::with_timeout(
        link,
        Duration::from_secs(24 * 3600),
    ));
    crypto.bind(77);
    let waiting = {
        let crypto = crypto.clone();
        std::thread::spawn(move || crypto.encrypt(&[1, 2], 0))
    };
    sent_rx.recv().expect("the frame went out");
    crypto.unbind();
    assert_eq!(waiting.join().expect("media thread"), None);
    // Woken by the call's end: neither an answer nor a timeout.
    assert_eq!(crypto.take_stats().round_trips, 0);
}

#[test]
fn round_trips_timeouts_and_errors_are_counted() {
    let crypto = GroupCallCrypto::new(Answering::new(false, false));
    crypto.bind(77);
    crypto.encrypt(&[1], 0);
    crypto.decrypt(42, &[1]);
    let stats = crypto.take_stats();
    assert_eq!((stats.round_trips, stats.timeouts, stats.errors), (2, 0, 0));
    assert!(stats.max_micros >= stats.avg_micros);
    assert_eq!(crypto.take_stats().round_trips, 0, "a summary starts over");

    let crypto = GroupCallCrypto::new(Answering::new(true, false));
    crypto.bind(77);
    crypto.encrypt(&[1], 0);
    let crypto_refused = GroupCallCrypto::new(Answering::new(false, true));
    crypto_refused.bind(77);
    crypto_refused.encrypt(&[1], 0);
    let failed = crypto.take_stats();
    let refused = crypto_refused.take_stats();
    assert_eq!((failed.round_trips, failed.errors), (1, 1));
    assert_eq!((refused.round_trips, refused.errors), (1, 1));
    assert_eq!((failed.avg_micros, failed.max_micros), (0, 0));

    // An answer that can't come in time: a zero wait times out at once.
    let crypto = GroupCallCrypto::with_timeout(Silent::new(), Duration::ZERO);
    crypto.bind(77);
    assert_eq!(crypto.encrypt(&[1], 0), None);
    let stats = crypto.take_stats();
    assert_eq!((stats.round_trips, stats.timeouts), (1, 1));
    assert_eq!(
        CryptoStats {
            round_trips: 3,
            timeouts: 1,
            errors: 0,
            avg_micros: 150,
            max_micros: 200
        }
        .log_line(77),
        "[call] group 77 e2e: 3 round trips, 1 timeouts, 0 errors, avg 150 us, max 200 us"
    );
}

#[test]
fn a_bound_call_hides_the_request_log_until_it_ends() {
    let link = Silent::new();
    let crypto = GroupCallCrypto::new(link.clone());
    crypto.bind(77);
    crypto.bind(77);
    assert_eq!(link.log(), vec![true], "hidden once per call");
    crypto.unbind();
    crypto.unbind();
    assert_eq!(link.log(), vec![true, false]);

    // A call dropped while bound shows the log again too.
    let link = Silent::new();
    drop({
        let crypto = GroupCallCrypto::new(link.clone());
        crypto.bind(78);
        crypto
    });
    assert_eq!(link.log(), vec![true, false]);
}

#[test]
fn the_request_log_level_comes_back_once_no_call_hides_it() {
    let gate = RequestLogGate::new();
    let set = Mutex::new(Vec::new());
    let record = |level| crate::locks::lock_or_recover(&set).push(level);
    gate.hide(|| Some(2), record);
    gate.hide(|| Some(99), record);
    gate.show(record);
    assert_eq!(
        *crate::locks::lock_or_recover(&set),
        vec![HIDDEN_REQUEST_LOG_LEVEL]
    );
    gate.show(record);
    gate.show(record);
    assert_eq!(
        *crate::locks::lock_or_recover(&set),
        vec![HIDDEN_REQUEST_LOG_LEVEL, 2]
    );
    // Unreadable: TDLib's own default comes back.
    gate.hide(|| None, record);
    gate.show(record);
    assert_eq!(
        crate::locks::lock_or_recover(&set).last().copied(),
        Some(DEFAULT_REQUEST_LOG_LEVEL)
    );
}

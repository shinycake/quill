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

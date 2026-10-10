//! Settings account tests: Ask a Question (`getSupportUser`) and the call
//! privacy rules with their exception lists.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::requests::PrivacyWho;
use crate::telegram::requests_privacy::PrivacySettingKey;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Fixture = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

fn fixture() -> Fixture {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (dir, driver, recorder, sink, seq)
}

fn cleanup(f: Fixture) {
    let (dir, driver, ..) = f;
    drop(driver);
    std::fs::remove_dir_all(dir).unwrap();
}

fn ingest(f: &mut Fixture, json: &str) {
    let owned = copy_and_parse(json, &f.4, &f.3).unwrap();
    f.1.ingest(owned).unwrap();
}

fn last_request(f: &Fixture) -> Value {
    serde_json::from_str(f.2.snapshot().last().unwrap()).unwrap()
}

fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap().to_string()
}

#[test]
fn ask_a_question_resolves_the_support_user_then_opens_the_chat() {
    let mut f = fixture();
    assert!(f.1.open_support_chat().unwrap());
    let request = last_request(&f);
    assert_eq!(request["@type"], "getSupportUser");
    // A second tap while the first is in flight sends nothing.
    let sent = f.2.snapshot().len();
    assert!(!f.1.open_support_chat().unwrap());
    assert_eq!(f.2.snapshot().len(), sent);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"user","id":424242,"@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    let open = last_request(&f);
    assert_eq!(open["@type"], "createPrivateChat");
    assert_eq!(open["user_id"], 424242);
    assert!(f.1.session.support_user_ready.is_none());
    cleanup(f);
}

#[test]
fn call_rules_keep_their_exceptions_and_feed_the_call_fields() {
    let mut f = fixture();
    f.1.fetch_privacy_rules(PrivacySettingKey::AllowCalls)
        .unwrap();
    let request = last_request(&f);
    assert_eq!(request["setting"]["@type"], "userPrivacySettingAllowCalls");
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"userPrivacySettingRules","rules":[{{"@type":"userPrivacySettingRuleRestrictUsers","user_ids":[6]}},{{"@type":"userPrivacySettingRuleAllowUsers","user_ids":[5]}},{{"@type":"userPrivacySettingRuleAllowContacts"}}],"@extra":"{}"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(
        f.1.session.calls.privacy_allow_calls,
        Some(PrivacyWho::Contacts)
    );
    let detail = match f
        .1
        .session
        .settings
        .privacy
        .get(&PrivacySettingKey::AllowCalls)
    {
        Some(crate::privacy::PrivacyKeyState::Ready(d)) => d.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(detail.always, vec![5]);
    assert_eq!(detail.never, vec![6]);

    f.1.set_privacy_rules(
        PrivacySettingKey::AllowCalls,
        detail.with_base(PrivacyWho::Nobody),
    )
    .unwrap();
    let set = last_request(&f);
    assert_eq!(set["@type"], "setUserPrivacySettingRules");
    let rules = set["rules"]["rules"].to_string();
    assert!(
        rules.contains("userPrivacySettingRuleAllowUsers"),
        "{rules}"
    );
    assert!(
        rules.contains("userPrivacySettingRuleRestrictUsers"),
        "{rules}"
    );
    assert!(
        rules.contains("userPrivacySettingRuleRestrictAll"),
        "{rules}"
    );
    assert_eq!(
        f.1.session.calls.privacy_allow_calls,
        Some(PrivacyWho::Nobody)
    );

    // Another device changes the peer-to-peer rule.
    ingest(
        &mut f,
        r#"{"@type":"updateUserPrivacySettingRules","setting":{"@type":"userPrivacySettingAllowPeerToPeerCalls"},"rules":{"@type":"userPrivacySettingRules","rules":[{"@type":"userPrivacySettingRuleAllowAll"}]}}"#,
    );
    assert_eq!(f.1.session.calls.privacy_p2p, Some(PrivacyWho::Everybody));
    assert!(matches!(
        f.1.session
            .settings
            .privacy
            .get(&PrivacySettingKey::PeerToPeer),
        Some(crate::privacy::PrivacyKeyState::Ready(_))
    ));
    cleanup(f);
}

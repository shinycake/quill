use super::super::RecordingSender;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::RequestPurpose;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::AuthorizationState;
use serde_json::json;
use std::sync::{Arc, atomic::AtomicU64};

#[test]
fn registration_requires_current_terms_and_valid_names_without_auto_acceptance() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut super::super::ConnectDriver<Arc<RecordingSender>>,
                  payload: serde_json::Value| {
        driver
            .ingest(copy_and_parse(&payload.to_string(), &seq, &sink).unwrap())
            .unwrap()
    };
    assert!(driver.register_user("Alice", "", None, false).is_err());
    let terms = |text: &str| json!({"@type":"termsOfService","text":{"@type":"formattedText","text":text,"entities":[]},"min_user_age":16,"show_popup":true});
    let update = |terms| json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitRegistration","terms_of_service":terms}});
    ingest(&mut driver, update(terms("Original terms")));
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::RegisterUser)
    );
    assert!(driver.register_user("Alice", "", None, false).is_err());
    let AuthorizationState::WaitRegistration {
        terms: Some(accepted),
    } = driver.session.auth.clone()
    else {
        panic!("terms were not parsed");
    };
    assert_eq!(accepted.min_user_age, 16);
    assert_eq!(accepted.text, "Original terms");
    assert!(
        driver
            .register_user(" ", "", Some(&accepted), false)
            .is_err()
    );
    assert!(
        driver
            .register_user(&"é".repeat(65), "", Some(&accepted), false)
            .is_err()
    );
    assert!(
        driver
            .register_user("Alice", &"x".repeat(65), Some(&accepted), false)
            .is_err()
    );
    assert!(
        driver
            .register_user("Alice\nAdmin", "", Some(&accepted), false)
            .is_err()
    );
    let first = driver
        .register_user(&"é".repeat(64), "", Some(&accepted), false)
        .unwrap();
    let request = sent_request(&recorder, "registerUser");
    assert_eq!(request["disable_notification"], true);
    assert_eq!(request["last_name"], "");
    assert!(
        driver
            .register_user("Alice", "", Some(&accepted), false)
            .is_err()
    );
    ingest(
        &mut driver,
        json!({"@type":"error","@extra":first.as_extra(),"code":400,"message":"private name"}),
    );
    assert_eq!(
        driver.session.last_auth_error.unwrap().user_message(),
        "registration not accepted — check your name"
    );
    driver
        .register_user("Alice", "Last", Some(&accepted), true)
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "registerUser")["disable_notification"],
        false
    );
    ingest(&mut driver, update(terms("Changed terms")));
    assert!(
        driver
            .register_user("Alice", "", Some(&accepted), false)
            .is_err()
    );
    ingest(
        &mut driver,
        update(json!({"@type":"termsOfService","min_user_age":16})),
    );
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::Unknown(_)
    ));
    assert!(driver.register_user("Alice", "", None, false).is_err());
    ingest(&mut driver, update(serde_json::Value::Null));
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::WaitRegistration { terms: None }
    ));
    driver
        .register_user(" Alice ", " Last ", None, false)
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "registerUser")["first_name"],
        "Alice"
    );
    ingest(
        &mut driver,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}),
    );
    assert!(driver.register_user("Alice", "", None, false).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::envelope::{NotificationSettingsScope, ScopeNotificationSettings};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Two quick changes of one scope: the second starts from the first (the
/// session shows a save at once), so it never sends the first field's old
/// value back (sweep 19-001).
#[test]
fn a_second_scope_change_keeps_the_first_one_that_has_not_been_answered() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let scope = NotificationSettingsScope::PrivateChats;
    driver
        .session
        .settings
        .scope_notification_settings
        .insert(scope, ScopeNotificationSettings::default());
    // The first switch: mentions off.
    let mut first = driver
        .session
        .settings
        .scope_notification_settings
        .get(&scope)
        .cloned()
        .expect("the cached scope");
    first.disable_mention_notifications = true;
    driver
        .send_scope_notification_settings(scope, &first)
        .expect("sent");
    // The second, before any answer: pinned off, from what the session shows.
    let mut second = driver
        .session
        .settings
        .scope_notification_settings
        .get(&scope)
        .cloned()
        .expect("the cached scope");
    assert!(
        second.disable_mention_notifications,
        "the first shows at once"
    );
    second.disable_pinned_message_notifications = true;
    driver
        .send_scope_notification_settings(scope, &second)
        .expect("sent");
    let sent = sent_request(&recorder, "setScopeNotificationSettings");
    assert_eq!(
        sent["notification_settings"]["disable_mention_notifications"],
        true
    );
    assert_eq!(
        sent["notification_settings"]["disable_pinned_message_notifications"],
        true
    );
    let _ = std::fs::remove_dir_all(&dir);
}

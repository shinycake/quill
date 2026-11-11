//! Scope notification settings sent in a burst start from each other, and
//! a refusal puts the settings from before back (sweep 11-p5-016).
use super::common::*;
use super::*;
use crate::telegram::envelope::{NotificationSettingsScope, ScopeNotificationSettings};

const SCOPE: NotificationSettingsScope = NotificationSettingsScope::PrivateChats;

fn send(session: &mut Session, edit: impl FnOnce(&mut ScopeNotificationSettings)) -> RequestId {
    let mut next = session
        .settings
        .scope_notification_settings
        .get(&SCOPE)
        .cloned()
        .unwrap_or_default();
    edit(&mut next);
    let id = session.request_for_scope(RequestPurpose::SetScopeNotificationSettings, SCOPE);
    session.show_scope_save(SCOPE, id, &next);
    id
}

fn shown(session: &Session) -> ScopeNotificationSettings {
    session
        .settings
        .scope_notification_settings
        .get(&SCOPE)
        .cloned()
        .unwrap_or_default()
}

#[test]
fn two_quick_edits_do_not_revert_each_other() {
    let (mut session, _sink) = session();
    session
        .settings
        .scope_notification_settings
        .insert(SCOPE, ScopeNotificationSettings::default());
    send(&mut session, |s| s.show_preview = false);
    send(&mut session, |s| s.mute_for = 3600);
    let now = shown(&session);
    assert!(!now.show_preview, "the first edit stays in the second send");
    assert_eq!(now.mute_for, 3600);
}

#[test]
fn a_refused_burst_puts_the_settings_from_before_back() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session
        .settings
        .scope_notification_settings
        .insert(SCOPE, ScopeNotificationSettings::default());
    let first = send(&mut session, |s| s.show_preview = false);
    let second = send(&mut session, |s| s.mute_for = 3600);
    let refuse = |session: &mut Session, id: RequestId| {
        apply_json(
            session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
                id.0
            ),
        )
    };
    refuse(&mut session, first);
    assert_eq!(shown(&session).mute_for, 3600, "one save is still out");
    refuse(&mut session, second);
    assert_eq!(shown(&session), ScopeNotificationSettings::default());
}

#[test]
fn an_update_while_saving_is_the_echo_and_changes_nothing() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session
        .settings
        .scope_notification_settings
        .insert(SCOPE, ScopeNotificationSettings::default());
    let id = send(&mut session, |s| s.mute_for = 60);
    send(&mut session, |s| s.mute_for = 120);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            "{{\"@type\":\"updateScopeNotificationSettings\",\"scope\":{{\"@type\":\"notificationSettingsScopePrivateChats\"}},\"notification_settings\":{}}}",
            scope_settings_json("", 60, true).replace(",\"@extra\":\"\"", "")
        ),
    );
    assert_eq!(shown(&session).mute_for, 120);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, id.0),
    );
    assert_eq!(shown(&session).mute_for, 120);
}

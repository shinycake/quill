use super::*;
use crate::connect::{ConnectDriver, RecordingSender, prepare_connect};
use crate::credentials::TelegramCredentials;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;

fn credentials() -> TelegramCredentials {
    TelegramCredentials {
        api_id: 99,
        api_hash: "unit-test-hash-not-for-network".into(),
    }
}

/// A background account's driver over a recording sender: no chat open,
/// no window showing it, the app in front.
struct Background {
    driver: ConnectDriver<Arc<RecordingSender>>,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: PathBuf,
}

impl Background {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "quill-notify-accounts-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let key = AccountKey::new("account-1").unwrap();
        let store = MemorySecretStore::new();
        let prepared = prepare_connect(&dir, key.clone(), &store, &credentials()).unwrap();
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let mut session = Session::new(key, sink.clone());
        session.app_active = true;
        let driver = ConnectDriver::new(
            session,
            Arc::new(RecordingSender::new()),
            credentials(),
            prepared,
        );
        Self {
            driver,
            sink,
            seq: AtomicU64::new(0),
            dir,
        }
    }

    fn ingest(&mut self, json: &str) {
        let owned = copy_and_parse(json, &self.seq, &self.sink).expect("parses");
        self.driver.ingest(owned).expect("ingest");
    }

    fn ready_with_chat(&mut self, chat_id: i64, muted: bool) {
        self.ingest(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        );
        let mute_for = if muted { 2147483647 } else { 0 };
        self.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{chat_id},"title":"Alice","type":{{"@type":"chatTypePrivate","user_id":{chat_id}}},"unread_count":0,"notification_settings":{{"@type":"chatNotificationSettings","use_default_mute_for":false,"mute_for":{mute_for},"use_default_sound":true,"sound_id":"0","use_default_show_preview":false,"show_preview":true,"use_default_mute_stories":true,"mute_stories":false,"use_default_story_sound":true,"story_sound_id":"0","use_default_show_story_poster":true,"show_story_poster":false,"use_default_disable_pinned_message_notifications":true,"disable_pinned_message_notifications":false,"use_default_disable_mention_notifications":true,"disable_mention_notifications":false}}}}}}"#
        ));
    }

    fn incoming(&mut self, chat_id: i64, message_id: i64, text: &str) {
        self.ingest(&format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{message_id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}}}"#
        ));
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn only_the_account_on_screen_notifies_when_all_accounts_is_off() {
    assert!(account_may_notify(true, true));
    assert!(account_may_notify(true, false));
    assert!(account_may_notify(false, true));
    assert!(!account_may_notify(false, false));
}

#[test]
fn the_setting_appears_with_two_accounts() {
    assert!(!notify_from_all_setting_visible(0));
    assert!(!notify_from_all_setting_visible(1));
    assert!(notify_from_all_setting_visible(2));
    assert!(notify_from_all_setting_visible(4));
}

#[test]
fn background_status_follows_the_authorization_state() {
    assert_eq!(
        background_status(&AuthorizationState::WaitTdlibParameters),
        BackgroundStatus::Connecting
    );
    assert_eq!(
        background_status(&AuthorizationState::Ready),
        BackgroundStatus::Ready
    );
    for auth in [
        AuthorizationState::WaitPhoneNumber,
        AuthorizationState::WaitEmailAddress,
        AuthorizationState::WaitPassword {
            has_recovery_email: false,
        },
        AuthorizationState::WaitRegistration { terms: None },
    ] {
        let status = background_status(&auth);
        assert_eq!(status, BackgroundStatus::NeedsSignIn);
        assert!(status.stops_client());
        assert_eq!(status.label(), Some("Not signed in"));
    }
    for auth in [
        AuthorizationState::LoggingOut,
        AuthorizationState::Closing,
        AuthorizationState::Closed,
    ] {
        let status = background_status(&auth);
        assert_eq!(status, BackgroundStatus::SignedOut);
        assert!(status.stops_client());
    }
    assert!(!BackgroundStatus::Ready.stops_client());
    assert!(!BackgroundStatus::Connecting.stops_client());
    assert_eq!(BackgroundStatus::Ready.label(), None);
    assert!(BackgroundStatus::Failed("Keychain locked".into()).stops_client());
}

#[test]
fn a_background_account_notifies_for_an_unmuted_chat() {
    let mut account = Background::new("notify");
    account.ready_with_chat(7, false);
    account.incoming(7, 11, "hi");
    let drain = drain_background(&mut account.driver.session, true);
    assert_eq!(drain.notifications.len(), 1);
    let shown = drain.notifications[0].for_display();
    assert_eq!(shown.chat_id, ChatId(7));
    assert_eq!(shown.title, "Alice");
    assert!(drain.attention, "a new message flashes the taskbar too");
    // Drained: the next poll starts empty.
    let again = drain_background(&mut account.driver.session, true);
    assert!(again.notifications.is_empty());
    assert!(!again.attention);
}

#[test]
fn all_accounts_off_drops_background_notifications_but_still_withdraws() {
    let mut account = Background::new("off");
    account.ready_with_chat(7, false);
    account.incoming(7, 11, "hi");
    account.driver.session.settings.pending_notification_clears = vec![ChatId(9)];
    let drain = drain_background(&mut account.driver.session, false);
    assert!(drain.notifications.is_empty());
    assert!(!drain.attention);
    assert_eq!(drain.clears, vec![ChatId(9)]);
    // Nothing waits in the session to pop out after a switch.
    assert!(
        account
            .driver
            .session
            .settings
            .pending_notifications
            .is_empty()
    );
}

#[test]
fn a_muted_chat_on_a_background_account_stays_quiet() {
    let mut account = Background::new("muted");
    account.ready_with_chat(7, true);
    account.incoming(7, 11, "hi");
    assert!(
        drain_background(&mut account.driver.session, true)
            .notifications
            .is_empty()
    );
}

#[test]
fn a_force_reply_never_reaches_the_screen_from_the_background() {
    let mut account = Background::new("force-reply");
    account.ready_with_chat(7, false);
    account.driver.session.bots.pending_force_reply = Some(crate::state::ForceReplyTarget {
        chat_id: ChatId(7),
        message_id: crate::ids::MessageId(11),
    });
    let _ = drain_background(&mut account.driver.session, true);
    assert!(account.driver.session.bots.pending_force_reply.is_none());
}

#[test]
fn unread_counts_come_from_the_server_totals_and_badge_prefs() {
    let mut account = Background::new("unread");
    account.ready_with_chat(7, false);
    account.ingest(
        r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":5,"unread_unmuted_count":2}"#,
    );
    assert_eq!(
        AccountUnread::of(&account.driver.session),
        AccountUnread {
            count: 5,
            muted: false
        }
    );
    account.ingest(
        r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":3,"unread_unmuted_count":0}"#,
    );
    assert_eq!(
        AccountUnread::of(&account.driver.session),
        AccountUnread {
            count: 3,
            muted: true
        }
    );
    // The account's own "Count muted chats" choice applies.
    account.driver.session.settings.badge_prefs.include_muted = false;
    assert_eq!(
        AccountUnread::of(&account.driver.session),
        AccountUnread::default()
    );
}

#[test]
fn combined_unread_is_muted_only_when_every_counted_account_is() {
    let loud = AccountUnread {
        count: 2,
        muted: false,
    };
    let quiet = AccountUnread {
        count: 4,
        muted: true,
    };
    let none = AccountUnread::default();
    assert_eq!(
        loud.combine(quiet),
        AccountUnread {
            count: 6,
            muted: false
        }
    );
    assert_eq!(quiet.combine(none), quiet);
    assert_eq!(none.combine(quiet), quiet);
    assert_eq!(none.combine(none), none);
    let full = AccountUnread {
        count: u32::MAX,
        muted: true,
    };
    assert_eq!(full.combine(quiet).count, u32::MAX);
}

#[test]
fn other_accounts_unread_round_trips_through_the_shared_cell() {
    let value = AccountUnread {
        count: 1234,
        muted: true,
    };
    assert_eq!(AccountUnread::unpack(value.pack()), value);
    let max = AccountUnread {
        count: u32::MAX,
        muted: false,
    };
    assert_eq!(AccountUnread::unpack(max.pack()), max);
}

#[test]
fn a_click_routes_by_the_account_in_the_tag() {
    let active = AccountKey::primary();
    let work = AccountKey::new("account-1").unwrap();
    let known = [active.clone(), work.clone()];
    assert_eq!(
        plan_click("account:primary", NotificationAction::Open, &active, &known),
        ClickPlan::Active
    );
    assert_eq!(
        plan_click(
            "account:account-1",
            NotificationAction::Open,
            &active,
            &known
        ),
        ClickPlan::SwitchThenOpen(work.clone())
    );
    assert_eq!(
        plan_click(
            "account:account-1",
            NotificationAction::Reply,
            &active,
            &known
        ),
        ClickPlan::SwitchThenOpen(work.clone())
    );
    assert_eq!(
        plan_click(
            "account:account-1",
            NotificationAction::MarkRead,
            &active,
            &known
        ),
        ClickPlan::MarkReadInBackground(work)
    );
    // Removed since the notification was shown, or garbage.
    assert_eq!(
        plan_click(
            "account:account-7",
            NotificationAction::Open,
            &active,
            &known
        ),
        ClickPlan::Ignore
    );
    assert_eq!(
        plan_click("account:../x", NotificationAction::Open, &active, &known),
        ClickPlan::Ignore
    );
    assert_eq!(
        plan_click("primary", NotificationAction::Open, &active, &known),
        ClickPlan::Ignore
    );
}

#[test]
fn the_title_names_the_account_only_with_two_signed_in() {
    assert!(!account_name_in_title(0));
    assert!(!account_name_in_title(1));
    assert!(account_name_in_title(2));
}

#[test]
fn one_call_at_a_time_across_accounts() {
    assert_eq!(plan_background_call(false, false), BackgroundCallPlan::Ring);
    assert_eq!(
        plan_background_call(true, false),
        BackgroundCallPlan::DeclineBusy
    );
    assert_eq!(
        plan_background_call(false, true),
        BackgroundCallPlan::DeclineBusy
    );
}

#[test]
fn only_an_incoming_pending_call_rings() {
    let mut account = Background::new("ringing");
    account.ready_with_chat(7, false);
    assert_eq!(ringing_incoming_call(&account.driver.session), None);
    account.ingest(
        r#"{"@type":"updateCall","call":{"@type":"call","id":5,"unique_id":"9","user_id":7,"is_outgoing":true,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    );
    assert_eq!(
        ringing_incoming_call(&account.driver.session),
        None,
        "outgoing"
    );
    account.ingest(
        r#"{"@type":"updateCall","call":{"@type":"call","id":5,"unique_id":"9","user_id":7,"is_outgoing":true,"is_video":false,"state":{"@type":"callStateDiscarded","reason":{"@type":"callDiscardReasonHungUp"},"need_rating":false,"need_debug_information":false,"need_log":false}}}"#,
    );
    account.ingest(
        r#"{"@type":"updateCall","call":{"@type":"call","id":6,"unique_id":"10","user_id":7,"is_outgoing":false,"is_video":true,"state":{"@type":"callStatePending","is_created":true,"is_received":true}}}"#,
    );
    assert_eq!(
        ringing_incoming_call(&account.driver.session),
        Some((6, 7, true))
    );
}

/// The tray and the icon badge ask on every wake: a wake with no applied
/// update reuses the count, and the first wake after one has the new count.
#[test]
fn the_unread_memo_recounts_only_after_an_applied_update() {
    let mut account = Background::new("memo");
    account.ready_with_chat(7, false);
    account.ingest(
        r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":5,"unread_unmuted_count":2}"#,
    );
    let mut memo = UnreadMemo::default();
    assert_eq!(memo.get(&account.driver.session).count, 5);
    // Not through an update: nothing tells the memo, so it does not count.
    account.driver.session.chat_list.unread_totals.main.messages = None;
    assert_eq!(memo.get(&account.driver.session).count, 5);
    account.ingest(
        r#"{"@type":"updateUnreadMessageCount","chat_list":{"@type":"chatListMain"},"unread_count":3,"unread_unmuted_count":1}"#,
    );
    assert_eq!(memo.get(&account.driver.session).count, 3);
    // The account's badge preferences are part of the key.
    account.driver.session.settings.badge_prefs.include_muted = false;
    assert_eq!(memo.get(&account.driver.session).count, 1);
}

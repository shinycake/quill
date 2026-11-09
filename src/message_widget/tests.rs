use super::storage::{self, Stored, QUOTA_BYTES};
use super::*;
use crate::telegram::envelope::TdError;

fn key(message: i64) -> WidgetKey {
    WidgetKey {
        chat_id: ChatId(5),
        message_id: MessageId(message),
    }
}

fn entry(message: i64, live: bool, visible: bool, hidden_at: u64) -> PoolEntry {
    PoolEntry {
        key: key(message),
        live,
        visible,
        hidden_at,
    }
}

#[test]
fn the_live_widget_hidden_longest_is_released_first() {
    let entries = [
        entry(1, true, false, 30),
        entry(2, true, false, 10),
        entry(3, true, true, 0),
        entry(4, false, false, 1),
    ];
    assert_eq!(victim(&entries), Some(key(2)));
}

#[test]
fn widgets_on_screen_are_never_released() {
    let entries = [entry(1, true, true, 0), entry(2, false, false, 3)];
    assert_eq!(victim(&entries), None);
}

#[test]
fn every_visible_widget_gets_a_sandbox() {
    assert_eq!(capacity(3), MAX_LIVE);
    assert_eq!(capacity(11), 11);
}

fn error(code: i32) -> TdError {
    TdError::from_code(code)
}

#[test]
fn query_failures_map_as_telegram_desktop_tells_the_widget() {
    assert_eq!(QueryFailure::of(&error(502)), QueryFailure::Timeout);
    let mut named = error(400);
    named.server_type = Some("WIDGET_RESPONSE_TIMEOUT".into());
    assert_eq!(QueryFailure::of(&named), QueryFailure::Timeout);
    assert_eq!(QueryFailure::of(&error(429)), QueryFailure::RateLimited);
    assert_eq!(QueryFailure::of(&error(400)), QueryFailure::Failed);
    assert_eq!(QueryFailure::Timeout.code(), 1);
    assert_eq!(QueryFailure::Failed.code(), 3);
}

#[test]
fn poll_periods_are_held_to_bounds() {
    assert_eq!(poll_period(0), DEFAULT_POLL_PERIOD);
    assert_eq!(poll_period(-4), DEFAULT_POLL_PERIOD);
    assert_eq!(poll_period(1), Duration::from_secs(5));
    assert_eq!(poll_period(90), Duration::from_secs(90));
    assert_eq!(poll_period(99_999), Duration::from_secs(3600));
}

#[test]
fn a_widget_opens_at_its_fixed_then_last_height() {
    assert_eq!(initial_height(240, Some(90.0)), 240.0);
    assert_eq!(initial_height(9000, None), MAX_HEIGHT);
    assert_eq!(initial_height(0, Some(90.0)), 90.0);
    assert_eq!(initial_height(0, Some(4000.0)), PLACEHOLDER_HEIGHT);
    assert_eq!(initial_height(0, None), PLACEHOLDER_HEIGHT);
}

#[test]
fn a_card_takes_the_bots_width_within_the_bubble() {
    assert_eq!(card_width(0, 430.0), 430.0);
    assert_eq!(card_width(120, 430.0), MIN_WIDTH);
    assert_eq!(card_width(320, 430.0), 320.0);
    assert_eq!(card_width(900, 430.0), 430.0);
    assert_eq!(card_width(120, 150.0), 150.0);
}

#[test]
fn stopped_and_failed_both_show_the_stopped_label() {
    assert!(Lifecycle::Stopped { reason: "cpu".into() }.is_stopped());
    assert!(Lifecycle::Failed { reason: "bundle".into() }.is_stopped());
    assert!(!Lifecycle::Released.is_stopped());
    assert!(Lifecycle::Loading.is_live() && Lifecycle::Running.is_live());
    assert!(!Lifecycle::Waiting.is_live());
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("quill-widget-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn storage_round_trips_and_an_empty_one_leaves_no_file() {
    let dir = temp_dir("round-trip");
    let mut stored = Stored::default();
    stored.local.insert("tab".into(), "2".into());
    stored.height = Some(212.0);
    storage::save(&dir, key(9), &stored).unwrap();
    assert_eq!(storage::load(&dir, key(9)), stored);
    assert_eq!(storage::load(&dir, key(10)), Stored::default());
    storage::save(&dir, key(9), &Stored::default()).unwrap();
    assert!(!storage::file(&dir, key(9)).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn storage_over_the_quota_or_out_of_bounds_is_not_restored() {
    let dir = temp_dir("bounds");
    std::fs::create_dir_all(&dir).unwrap();
    let big = "x".repeat(QUOTA_BYTES + 1);
    let text = format!("{{\"local\":{{\"k\":\"{big}\"}},\"height\":9000.0}}");
    std::fs::write(storage::file(&dir, key(3)), text).unwrap();
    assert_eq!(storage::load(&dir, key(3)), Stored::default());
    std::fs::write(storage::file(&dir, key(4)), b"not json").unwrap();
    assert_eq!(storage::load(&dir, key(4)), Stored::default());
    let _ = std::fs::remove_dir_all(&dir);
}

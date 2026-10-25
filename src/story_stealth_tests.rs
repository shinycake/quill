use super::{
    FUTURE_PERIOD_OPTION, PAST_PERIOD_OPTION, StealthButton, StealthEntry, StealthPeriods,
    StealthPress, StealthToast, about_text, active_left, press, time_left_text,
};
use crate::state::StoryStealthMode;
use crate::telegram::envelope::OptionValue;

const NOW: i64 = 1_700_000_000;

fn mode(active_for: i64, cooldown_for: i64) -> StoryStealthMode {
    StoryStealthMode {
        active_until_date: i32::try_from(NOW + active_for).unwrap_or(0),
        cooldown_until_date: i32::try_from(NOW + cooldown_for).unwrap_or(0),
    }
}

#[test]
fn time_left_reads_like_tdesktop() {
    assert_eq!(time_left_text(9), "0:09");
    assert_eq!(time_left_text(245), "4:05");
    assert_eq!(time_left_text(3723), "1:02:03");
    assert_eq!(time_left_text(-4), "0:00");
}

#[test]
fn the_button_follows_premium_and_the_cooldown() {
    let off = StoryStealthMode::default();
    assert_eq!(
        StealthButton::of(off, NOW, false, false),
        StealthButton::Unlock
    );
    assert_eq!(
        StealthButton::of(off, NOW, true, false),
        StealthButton::Enable
    );
    assert_eq!(
        StealthButton::of(off, NOW, true, true),
        StealthButton::EnableAndOpen
    );
    assert_eq!(
        StealthButton::of(mode(-10, 299), NOW, true, true),
        StealthButton::Cooldown { left: 299 }
    );
    assert_eq!(
        StealthButton::of(mode(-10, 299), NOW, false, false),
        StealthButton::Unlock,
        "a free account sees the lock whatever the cooldown"
    );
    assert_eq!(
        StealthButton::Cooldown { left: 299 }.label(),
        "Available in 4:59"
    );
    assert_eq!(StealthButton::Unlock.label(), "Unlock Stealth Mode");
    assert_eq!(
        StealthButton::EnableAndOpen.label(),
        "Enable and open the story"
    );
}

#[test]
fn a_press_does_what_tdesktop_does() {
    let off = StoryStealthMode::default();
    assert_eq!(
        press(mode(60, 3600), NOW, true, false),
        StealthPress::EnabledToast
    );
    assert_eq!(press(off, NOW, false, false), StealthPress::PremiumPreview);
    assert_eq!(
        press(mode(-10, 60), NOW, true, false),
        StealthPress::CooldownToast
    );
    assert_eq!(press(off, NOW, true, false), StealthPress::Activate);
    assert_eq!(press(off, NOW, true, true), StealthPress::Wait);
}

#[test]
fn an_entry_point_skips_the_box_while_stealth_is_on() {
    assert_eq!(active_left(mode(90, 3600), NOW), Some(90));
    assert_eq!(active_left(mode(0, 3600), NOW), None);
    assert_eq!(active_left(StoryStealthMode::default(), NOW), None);
}

#[test]
fn the_menu_icon_locks_only_for_a_free_account_without_stealth() {
    let off = StoryStealthMode::default();
    assert!(StealthEntry::of(false, off, NOW).locked);
    assert!(!StealthEntry::of(true, off, NOW).locked);
    assert!(!StealthEntry::of(false, mode(30, 600), NOW).locked);
}

#[test]
fn the_default_periods_keep_tdesktops_strings() {
    let periods = StealthPeriods::default();
    assert_eq!(
        periods.past_about(),
        "Hide my views from the past 5 minutes."
    );
    assert_eq!(
        periods.next_about(),
        "Hide my views for the next 25 minutes."
    );
    let toast = StealthToast::activated(periods);
    assert_eq!(toast.title, Some("Stealth Mode On"));
    assert!(toast.text.contains("**5 minutes**"));
    assert!(toast.text.contains("**25 minutes**"));
    assert_eq!(
        about_text(false),
        "Subscribe to Telegram Premium to watch stories without appearing in the list of viewers."
    );
}

#[test]
fn periods_come_from_the_tdlib_options() {
    let mut periods = StealthPeriods::default();
    periods.apply_option(PAST_PERIOD_OPTION, &OptionValue::Integer(600));
    periods.apply_option(FUTURE_PERIOD_OPTION, &OptionValue::Integer(3600));
    periods.apply_option(FUTURE_PERIOD_OPTION, &OptionValue::Integer(0));
    periods.apply_option("other", &OptionValue::Integer(5));
    assert_eq!(
        periods,
        StealthPeriods {
            past: 600,
            future: 3600
        }
    );
    assert_eq!(
        periods.past_about(),
        "Hide my views from the past 10 minutes."
    );
    assert_eq!(periods.next_about(), "Hide my views for the next 1 hour.");
    let toast = StealthToast::activated(periods);
    assert!(toast.text.contains("**10 minutes**"), "{}", toast.text);
    assert!(toast.text.contains("**1 hour**"), "{}", toast.text);
    periods.apply_option(PAST_PERIOD_OPTION, &OptionValue::Integer(45));
    assert_eq!(
        periods.past_about(),
        "Hide my views from the past 45 seconds."
    );
}

#[test]
fn the_toasts_carry_tdesktops_titles() {
    let already = StealthToast::already(125);
    assert_eq!(already.title, Some("You are in Stealth Mode"));
    assert!(already.text.contains("**2:05**"), "{}", already.text);
    let cooldown = StealthToast::cooldown();
    assert_eq!(cooldown.title, None);
    assert_eq!(
        cooldown.plain_text(),
        "Please wait until Stealth Mode is ready to use again."
    );
}

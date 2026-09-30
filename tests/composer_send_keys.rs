//! Slice `parity:settings-enter-send` + `parity:settings-ctrlenter-send`:
//! the send-key mode policy. Proves through the public
//! `composer::should_send_on_enter` interface:
//! 1. `SendKeyMode::Enter` (default): plain Enter sends; Shift+Enter and
//!    Ctrl/Cmd+Enter do not; IME composition never sends.
//! 2. `SendKeyMode::CtrlEnter`: plain Enter does NOT send (inserts a
//!    newline); Ctrl/Cmd+Enter sends; Shift+Ctrl+Enter does not; IME
//!    composition never sends.
//! 3. `ChatPrefs` defaults to `SendKeyMode::Enter` and round-trips
//!    through JSON (missing file → defaults, never an error).

use quill::composer::{EnterEvent, SendKeyMode, should_send_on_enter};
use quill::ids::AccountKey;
use quill::settings::{AccountPaths, ChatPrefs, load_chat_prefs, save_chat_prefs};

fn ev(composing: bool, shift: bool, secondary: bool) -> EnterEvent {
    EnterEvent {
        composing,
        shift,
        secondary,
    }
}

#[test]
fn enter_mode_plain_enter_sends() {
    assert!(should_send_on_enter(
        ev(false, false, false),
        SendKeyMode::Enter
    ));
}

#[test]
fn enter_mode_modifiers_do_not_send() {
    // Shift+Enter inserts a newline.
    assert!(!should_send_on_enter(
        ev(false, true, false),
        SendKeyMode::Enter
    ));
    // Ctrl/Cmd+Enter inserts a newline in Enter mode.
    assert!(!should_send_on_enter(
        ev(false, false, true),
        SendKeyMode::Enter
    ));
    assert!(!should_send_on_enter(
        ev(false, true, true),
        SendKeyMode::Enter
    ));
}

#[test]
fn enter_mode_ime_never_sends() {
    assert!(!should_send_on_enter(
        ev(true, false, false),
        SendKeyMode::Enter
    ));
    assert!(!should_send_on_enter(
        ev(true, false, true),
        SendKeyMode::Enter
    ));
}

#[test]
fn ctrl_enter_mode_plain_enter_does_not_send() {
    assert!(!should_send_on_enter(
        ev(false, false, false),
        SendKeyMode::CtrlEnter
    ));
    assert!(!should_send_on_enter(
        ev(false, true, false),
        SendKeyMode::CtrlEnter
    ));
}

#[test]
fn ctrl_enter_mode_secondary_sends() {
    assert!(should_send_on_enter(
        ev(false, false, true),
        SendKeyMode::CtrlEnter
    ));
    // Shift+Ctrl+Enter does not send.
    assert!(!should_send_on_enter(
        ev(false, true, true),
        SendKeyMode::CtrlEnter
    ));
}

#[test]
fn ctrl_enter_mode_ime_never_sends() {
    assert!(!should_send_on_enter(
        ev(true, false, true),
        SendKeyMode::CtrlEnter
    ));
    assert!(!should_send_on_enter(
        ev(true, false, false),
        SendKeyMode::CtrlEnter
    ));
}

#[test]
fn send_key_mode_default_is_enter() {
    assert_eq!(SendKeyMode::default(), SendKeyMode::Enter);
    assert_eq!(ChatPrefs::default().send_key_mode, SendKeyMode::Enter);
}

#[test]
fn chat_prefs_roundtrip_and_missing_file() {
    let dir = std::env::temp_dir().join(format!("quill-test-sendkeys-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let paths = AccountPaths::for_root(&dir, &AccountKey::primary());

    // Missing file → defaults, never an error.
    assert_eq!(load_chat_prefs(&paths).send_key_mode, SendKeyMode::Enter);

    let prefs = ChatPrefs {
        send_key_mode: SendKeyMode::CtrlEnter,
        spellcheck_enabled: false,
    };
    save_chat_prefs(&paths, &prefs).unwrap();
    assert_eq!(load_chat_prefs(&paths), prefs);

    let _ = std::fs::remove_dir_all(&dir);
}

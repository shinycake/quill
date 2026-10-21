//! Calls page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page, Platforms};

const P: Page = Page::Calls;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new(
        "calls.confirm_before_calling",
        P,
        keys::Q_PREFS_CONFIRM_BEFORE_CALLING,
    )
    .keywords(&["ask", "accidental"]),
    Entry::new("calls.push_to_talk", P, keys::Q_AV_PUSH_TO_TALK)
        .keywords(&["ptt", "hold key", "hotkey"]),
    Entry::new("calls.ptt_shortcut", P, keys::Q_SETTINGS_CALLS_PTT_SHORTCUT)
        .section(keys::Q_AV_PUSH_TO_TALK)
        .keywords(&["key", "binding"]),
    Entry::new("calls.ptt_delay", P, keys::Q_SETTINGS_CALLS_PTT_DELAY)
        .section(keys::Q_AV_PUSH_TO_TALK)
        .keywords(&["tail", "cut off"]),
    Entry::new("calls.mic_test", P, keys::Q_OPS_TEST_MICROPHONE)
        .keywords(&["level", "input check"]),
    Entry::new(
        "calls.accept_on_device",
        P,
        keys::Q_SETTINGS_CALLS_ACCEPT_ON_DEVICE,
    )
    .keywords(&["incoming", "ring"]),
    Entry::new(
        "calls.system_sound_prefs",
        P,
        keys::Q_SETTINGS_CALLS_SYSTEM_SOUND_PREFS,
    )
    .keywords(&["audio settings"])
    .only(Platforms::MACOS.or(Platforms::WINDOWS)),
    Entry::new(
        "calls.noise_suppression",
        P,
        keys::GROUP_CALL_NOISE_SUPPRESSION,
    )
    .keywords(&["denoise", "background noise"]),
    Entry::new("calls.input_device", P, keys::Q_SETTINGS_CALLS_INPUT_DEVICE)
        .keywords(&["input", "mic"]),
    Entry::new(
        "calls.output_device",
        P,
        keys::Q_SETTINGS_CALLS_OUTPUT_DEVICE,
    )
    .keywords(&["output", "headphones", "speakers"]),
];

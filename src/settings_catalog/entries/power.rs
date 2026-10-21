//! Battery and Animations page entries.

use crate::lang::keys;
use crate::settings_catalog::{Entry, Page};

const P: Page = Page::Power;

pub(super) const ENTRIES: &[Entry] = &[
    Entry::new(
        "power.stickers_panel",
        P,
        keys::Q_SETTINGS_POWER_STICKERS_PANEL,
    )
    .section(keys::Q_SETTINGS_POWER_SEC_STICKERS),
    Entry::new(
        "power.stickers_chat",
        P,
        keys::Q_SETTINGS_POWER_STICKERS_CHAT,
    )
    .section(keys::Q_SETTINGS_POWER_SEC_STICKERS),
    Entry::new("power.emoji_panel", P, keys::Q_SETTINGS_POWER_EMOJI_PANEL)
        .section(keys::Q_SETTINGS_POWER_SEC_EMOJI),
    Entry::new(
        "power.emoji_reactions",
        P,
        keys::Q_SETTINGS_POWER_EMOJI_REACTIONS,
    )
    .section(keys::Q_SETTINGS_POWER_SEC_EMOJI),
    Entry::new("power.emoji_chat", P, keys::Q_SETTINGS_POWER_EMOJI_CHAT)
        .section(keys::Q_SETTINGS_POWER_SEC_EMOJI),
    Entry::new("power.emoji_status", P, keys::Q_SETTINGS_POWER_EMOJI_STATUS)
        .section(keys::Q_SETTINGS_POWER_SEC_EMOJI),
    Entry::new(
        "power.wallpaper_rotation",
        P,
        keys::Q_SETTINGS_POWER_WALLPAPER_ROTATION,
    )
    .section(keys::Q_SETTINGS_POWER_SEC_CHATS)
    .keywords(&["background animation"]),
    Entry::new(
        "power.spoiler_effect",
        P,
        keys::Q_SETTINGS_POWER_SPOILER_EFFECT,
    )
    .section(keys::Q_SETTINGS_POWER_SEC_CHATS)
    .keywords(&["particles"]),
    Entry::new(
        "power.message_effects",
        P,
        keys::Q_SETTINGS_POWER_MESSAGE_EFFECTS,
    )
    .section(keys::Q_SETTINGS_POWER_SEC_CHATS)
    .keywords(&["fireworks", "confetti"]),
    Entry::new("power.calls", P, keys::Q_SETTINGS_POWER_CALLS).keywords(&["video call"]),
    Entry::new("power.interface", P, keys::Q_SETTINGS_POWER_INTERFACE).keywords(&[
        "transitions",
        "motion",
        "reduce motion",
    ]),
    Entry::new("power.save_power", P, keys::SETTINGS_POWER_AUTO).keywords(&[
        "battery saver",
        "laptop",
        "low power",
    ]),
];

//! "Battery and animations": tdesktop's `PowerSaving` flags (`ui/power_saving.h`).
//!
//! A flag set means "save power here": that animation stays still. Only the
//! categories that animate something in Quill are offered.
//!
//! The bits keep tdesktop's values so a future import of its settings maps
//! one to one. The current set lives in a process-wide atomic: the views read
//! it while rendering, and the Appearance dialog writes it.
#![deny(clippy::indexing_slicing)]

use std::sync::atomic::{AtomicU32, Ordering};

/// One power-saving category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// Interface animations: message reveal, selection fades, switches.
    Animations,
    /// Animated stickers in the sticker panel.
    StickersPanel,
    /// Animated stickers in chats.
    StickersChat,
    /// Animated emoji in the emoji panel.
    EmojiPanel,
    /// Animated custom emoji in the expanded reaction selector (tdesktop
    /// `kEmojiReactions`).
    EmojiReactions,
    /// Animated emoji in messages and the chat list.
    EmojiChat,
    /// Wallpaper rotation: the freeform gradient turning on each sent
    /// message (tdesktop `kChatBackground`).
    ChatBackground,
    /// The moving specks over spoilers.
    ChatSpoiler,
    /// The pulses and rings on the call panels.
    Calls,
    /// Animated Premium emoji statuses next to names (tdesktop
    /// `kEmojiStatus`).
    EmojiStatus,
    /// Effects in messages: the sticker flying in from the panel, the
    /// dust of a deleted message and the Premium sticker effects (tdesktop
    /// `kChatEffects`).
    ChatEffects,
}

impl Flag {
    pub const fn bit(self) -> u32 {
        match self {
            Self::Animations => 1 << 0,
            Self::StickersPanel => 1 << 1,
            Self::StickersChat => 1 << 2,
            Self::EmojiPanel => 1 << 3,
            Self::EmojiReactions => 1 << 4,
            Self::EmojiChat => 1 << 5,
            Self::ChatBackground => 1 << 6,
            Self::ChatSpoiler => 1 << 7,
            Self::Calls => 1 << 8,
            Self::EmojiStatus => 1 << 9,
            Self::ChatEffects => 1 << 10,
        }
    }
}

/// All categories Quill offers, in the dialog's order.
pub const ALL_FLAGS: [Flag; 11] = [
    Flag::StickersPanel,
    Flag::StickersChat,
    Flag::EmojiPanel,
    Flag::EmojiReactions,
    Flag::EmojiChat,
    Flag::EmojiStatus,
    Flag::ChatBackground,
    Flag::ChatSpoiler,
    Flag::ChatEffects,
    Flag::Calls,
    Flag::Animations,
];

/// Union of every offered bit.
pub const ALL_BITS: u32 = Flag::Animations.bit()
    | Flag::StickersPanel.bit()
    | Flag::StickersChat.bit()
    | Flag::EmojiPanel.bit()
    | Flag::EmojiReactions.bit()
    | Flag::EmojiChat.bit()
    | Flag::ChatBackground.bit()
    | Flag::ChatSpoiler.bit()
    | Flag::Calls.bit()
    | Flag::EmojiStatus.bit()
    | Flag::ChatEffects.bit();

/// A titled group of switches (`lng_settings_power_*`).
pub struct Group {
    pub title: Option<&'static str>,
    pub items: &'static [(Flag, &'static str)],
}

/// The dialog's layout, with tdesktop's wording.
pub const GROUPS: [Group; 5] = [
    Group {
        title: Some("Animated Stickers"),
        items: &[
            (Flag::StickersPanel, "Autoplay in panel"),
            (Flag::StickersChat, "Autoplay in chat"),
        ],
    },
    Group {
        title: Some("Animated Emoji"),
        items: &[
            (Flag::EmojiPanel, "Autoplay in panel"),
            (Flag::EmojiReactions, "Autoplay in reactions menu"),
            (Flag::EmojiChat, "Autoplay in messages"),
            (Flag::EmojiStatus, "Autoplay in premium status"),
        ],
    },
    Group {
        title: Some("Animations in Chats"),
        items: &[
            (Flag::ChatBackground, "Wallpaper rotation"),
            (Flag::ChatSpoiler, "Animated spoiler effect"),
            (Flag::ChatEffects, "Effects in messages"),
        ],
    },
    Group {
        title: None,
        items: &[(Flag::Calls, "Animations in Calls")],
    },
    Group {
        title: None,
        items: &[(Flag::Animations, "Interface animations")],
    },
];

/// Drop bits Quill doesn't know (hand-edited prefs, or newer files).
pub const fn sanitize(bits: u32) -> u32 {
    bits & ALL_BITS
}

/// `bits` with `flag` turned on or off.
pub const fn with_flag(bits: u32, flag: Flag, on: bool) -> u32 {
    if on {
        bits | flag.bit()
    } else {
        bits & !flag.bit()
    }
}

/// Whether `flag` saves power under `bits`; `force` (a battery saver) saves
/// everything.
pub const fn is_on(bits: u32, force: bool, flag: Flag) -> bool {
    force || bits & flag.bit() != 0
}

/// The reduced-motion setting GPUI should run with: the OS preference, or
/// the interface-animations switch.
pub const fn reduce_motion(os_prefers: bool, bits: u32, force: bool) -> bool {
    os_prefers || is_on(bits, force, Flag::Animations)
}

const SLOTS: usize = 6;
pub(crate) const BITS: usize = 0;
pub(crate) const FORCE_ALL: usize = 1;
pub(crate) const OS_REDUCE_MOTION: usize = 2;
pub(crate) const BATTERY_STATE: usize = 3;
pub(crate) const BATTERY_IGNORE: usize = 4;
pub(crate) const THERMAL_HOT: usize = 5;

static GLOBAL: [AtomicU32; SLOTS] = [const { AtomicU32::new(0) }; SLOTS];

thread_local! {
    /// This thread's own copy, once it is isolated.
    static LOCAL: std::cell::Cell<Option<[u32; SLOTS]>> = const { std::cell::Cell::new(None) };
}

/// Keep this state for the calling thread alone from now on, starting from
/// nothing saved. The app keeps one process-wide copy; UI tests run in
/// parallel on threads of their own, so each gets its own (the same split as
/// `wake::isolate_current_thread`). Isolating again keeps what is set.
#[doc(hidden)]
pub fn isolate_for_tests() {
    let _ = LOCAL.try_with(|local| {
        if local.get().is_none() {
            local.set(Some([0; SLOTS]));
        }
    });
}

fn local() -> Option<[u32; SLOTS]> {
    LOCAL.try_with(std::cell::Cell::get).ok().flatten()
}

pub(crate) fn slot_get(slot: usize) -> u32 {
    match local() {
        Some(all) => all.get(slot).copied().unwrap_or(0),
        None => GLOBAL
            .get(slot)
            .map_or(0, |value| value.load(Ordering::Relaxed)),
    }
}

pub(crate) fn slot_set(slot: usize, value: u32) {
    match local() {
        Some(mut all) => {
            if let Some(entry) = all.get_mut(slot) {
                *entry = value;
            }
            let _ = LOCAL.try_with(|local| local.set(Some(all)));
        }
        None => {
            if let Some(entry) = GLOBAL.get(slot) {
                entry.store(value, Ordering::Relaxed);
            }
        }
    }
}

/// Replace the current set (sanitized).
pub fn set(bits: u32) {
    slot_set(BITS, sanitize(bits));
}

/// The current set.
pub fn current() -> u32 {
    slot_get(BITS)
}

/// Save power everywhere regardless of the set (tdesktop's `SetForceAll`).
pub fn set_force_all(force: bool) {
    slot_set(FORCE_ALL, force.into());
}

fn forced() -> bool {
    slot_get(FORCE_ALL) != 0
}

/// Whether `flag` saves power right now.
pub fn on(flag: Flag) -> bool {
    is_on(current(), forced(), flag)
}

/// Remember the OS reduce-motion preference, read once at startup.
pub fn set_os_reduce_motion(prefers: bool) {
    slot_set(OS_REDUCE_MOTION, prefers.into());
}

/// The OS reduce-motion preference (or `QUILL_REDUCED_MOTION`) alone,
/// without Quill's own Power Saving.
pub fn os_reduce_motion() -> bool {
    slot_get(OS_REDUCE_MOTION) != 0
}

/// What `App::set_reduce_motion` should be right now.
pub fn reduce_motion_now() -> bool {
    reduce_motion(slot_get(OS_REDUCE_MOTION) != 0, current(), forced())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_list_every_flag_exactly_once() {
        let mut seen = 0u32;
        for group in &GROUPS {
            for (flag, _) in group.items {
                assert_eq!(seen & flag.bit(), 0, "{flag:?} listed twice");
                seen |= flag.bit();
            }
        }
        assert_eq!(seen, ALL_BITS);
        let mut from_all = 0;
        for flag in ALL_FLAGS {
            from_all |= flag.bit();
        }
        assert_eq!(from_all, ALL_BITS);
    }

    #[test]
    fn bits_are_distinct_and_match_tdesktop() {
        let mut seen = 0u32;
        for flag in ALL_FLAGS {
            assert_eq!(flag.bit().count_ones(), 1);
            assert_eq!(seen & flag.bit(), 0);
            seen |= flag.bit();
        }
        // ui/power_saving.h
        assert_eq!(Flag::Animations.bit(), 1);
        assert_eq!(Flag::StickersChat.bit(), 4);
        assert_eq!(Flag::EmojiReactions.bit(), 16);
        assert_eq!(Flag::ChatBackground.bit(), 64);
        assert_eq!(Flag::ChatSpoiler.bit(), 128);
        assert_eq!(Flag::Calls.bit(), 256);
        assert_eq!(Flag::EmojiStatus.bit(), 512);
        assert_eq!(Flag::ChatEffects.bit(), 1024);
    }

    #[test]
    fn sanitize_drops_unknown_bits() {
        assert_eq!(sanitize(u32::MAX), ALL_BITS);
        assert_eq!(
            sanitize(1 << 6),
            Flag::ChatBackground.bit(),
            "wallpaper rotation"
        );
        assert_eq!(sanitize(1 << 4), Flag::EmojiReactions.bit());
        assert_eq!(sanitize(1 << 11), 0, "bits past kAll are dropped");
        assert_eq!(sanitize(Flag::Calls.bit()), Flag::Calls.bit());
    }

    #[test]
    fn flags_toggle_independently() {
        let bits = with_flag(0, Flag::StickersChat, true);
        let bits = with_flag(bits, Flag::Calls, true);
        assert!(is_on(bits, false, Flag::StickersChat));
        assert!(is_on(bits, false, Flag::Calls));
        assert!(!is_on(bits, false, Flag::EmojiChat));
        let bits = with_flag(bits, Flag::StickersChat, false);
        assert!(!is_on(bits, false, Flag::StickersChat));
        assert!(is_on(bits, false, Flag::Calls));
    }

    #[test]
    fn every_flag_gates_independently_of_the_others() {
        for flag in ALL_FLAGS {
            let bits = with_flag(0, flag, true);
            for other in ALL_FLAGS {
                assert_eq!(
                    is_on(bits, false, other),
                    other == flag,
                    "{flag:?} must not leak into {other:?}"
                );
            }
        }
    }

    #[test]
    fn global_gate_follows_the_set_and_the_forced_state() {
        // One test owns the process-wide state so parallel tests cannot
        // interleave with it.
        set(0);
        set_force_all(false);
        assert!(!on(Flag::EmojiReactions));
        set(Flag::EmojiReactions.bit());
        assert!(on(Flag::EmojiReactions));
        assert!(!on(Flag::EmojiPanel));
        set(0);
        set_force_all(true);
        assert!(on(Flag::EmojiPanel));
        set_force_all(false);
        assert!(!on(Flag::EmojiPanel));
    }

    #[test]
    fn force_saves_everything() {
        for flag in ALL_FLAGS {
            assert!(is_on(0, true, flag));
            assert!(!is_on(0, false, flag));
        }
    }

    #[test]
    fn reduced_motion_follows_the_os_or_the_switch() {
        assert!(!reduce_motion(false, 0, false));
        assert!(reduce_motion(true, 0, false));
        assert!(reduce_motion(false, Flag::Animations.bit(), false));
        assert!(!reduce_motion(false, Flag::StickersChat.bit(), false));
        assert!(reduce_motion(false, 0, true));
    }
}

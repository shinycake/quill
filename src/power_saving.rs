//! "Battery and animations": tdesktop's `PowerSaving` flags (`ui/power_saving.h`).
//!
//! A flag set means "save power here": that animation stays still. Only the
//! categories that animate something in Quill are offered. tdesktop also has
//! wallpaper rotation, reaction-menu emoji, premium-status emoji and message
//! effects; Quill has none of those animations, so there is nothing to gate.
//!
//! The bits keep tdesktop's values so a future import of its settings maps
//! one to one. The current set lives in a process-wide atomic: the views read
//! it while rendering, and the Appearance dialog writes it.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

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
    /// Animated emoji in messages and the chat list.
    EmojiChat,
    /// The moving specks over spoilers.
    ChatSpoiler,
    /// The pulses and rings on the call panels.
    Calls,
}

impl Flag {
    pub const fn bit(self) -> u32 {
        match self {
            Self::Animations => 1 << 0,
            Self::StickersPanel => 1 << 1,
            Self::StickersChat => 1 << 2,
            Self::EmojiPanel => 1 << 3,
            Self::EmojiChat => 1 << 5,
            Self::ChatSpoiler => 1 << 7,
            Self::Calls => 1 << 8,
        }
    }
}

/// All categories Quill offers, in the dialog's order.
pub const ALL_FLAGS: [Flag; 7] = [
    Flag::StickersPanel,
    Flag::StickersChat,
    Flag::EmojiPanel,
    Flag::EmojiChat,
    Flag::ChatSpoiler,
    Flag::Calls,
    Flag::Animations,
];

/// Union of every offered bit.
pub const ALL_BITS: u32 = Flag::Animations.bit()
    | Flag::StickersPanel.bit()
    | Flag::StickersChat.bit()
    | Flag::EmojiPanel.bit()
    | Flag::EmojiChat.bit()
    | Flag::ChatSpoiler.bit()
    | Flag::Calls.bit();

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
            (Flag::EmojiChat, "Autoplay in messages"),
        ],
    },
    Group {
        title: Some("Animations in Chats"),
        items: &[(Flag::ChatSpoiler, "Animated spoiler effect")],
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

static BITS: AtomicU32 = AtomicU32::new(0);
static FORCE_ALL: AtomicBool = AtomicBool::new(false);
static OS_REDUCE_MOTION: AtomicBool = AtomicBool::new(false);

/// Replace the current set (sanitized).
pub fn set(bits: u32) {
    BITS.store(sanitize(bits), Ordering::Relaxed);
}

/// The current set.
pub fn current() -> u32 {
    BITS.load(Ordering::Relaxed)
}

/// Save power everywhere regardless of the set (tdesktop's `SetForceAll`).
pub fn set_force_all(force: bool) {
    FORCE_ALL.store(force, Ordering::Relaxed);
}

/// Whether `flag` saves power right now.
pub fn on(flag: Flag) -> bool {
    is_on(current(), FORCE_ALL.load(Ordering::Relaxed), flag)
}

/// Remember the OS reduce-motion preference, read once at startup.
pub fn set_os_reduce_motion(prefers: bool) {
    OS_REDUCE_MOTION.store(prefers, Ordering::Relaxed);
}

/// What `App::set_reduce_motion` should be right now.
pub fn reduce_motion_now() -> bool {
    reduce_motion(
        OS_REDUCE_MOTION.load(Ordering::Relaxed),
        current(),
        FORCE_ALL.load(Ordering::Relaxed),
    )
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
        assert_eq!(Flag::ChatSpoiler.bit(), 128);
        assert_eq!(Flag::Calls.bit(), 256);
    }

    #[test]
    fn sanitize_drops_unknown_bits() {
        assert_eq!(sanitize(u32::MAX), ALL_BITS);
        assert_eq!(sanitize(1 << 6), 0, "wallpaper rotation is not offered");
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

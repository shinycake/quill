//! The one limit event a Premium limit error raises: a box that stays
//! until it is closed, or a toast the window shows once.
#![deny(clippy::indexing_slicing)]

use crate::premium_limits::PremiumLimitKind;

/// What a failed request that named a limit asks the window to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitNotice {
    /// The limit box is open (pins, caption, public links, ...), with how
    /// far over the limit the action went when that is known (caption).
    Box { kind: PremiumLimitKind, extra: i64 },
    /// The toast waits for the next frame (saved GIFs, favorite stickers,
    /// similar channels, owned bots); the window takes it once.
    Toast(PremiumLimitKind),
}

impl LimitNotice {
    /// The limit event of `kind`: its toast for the kinds tdesktop tells
    /// about with one, else the box.
    pub fn of(kind: PremiumLimitKind, extra: i64) -> Self {
        if kind.shows_toast() {
            Self::Toast(kind)
        } else {
            Self::Box { kind, extra }
        }
    }

    /// The open box.
    pub fn open_box(&self) -> Option<(PremiumLimitKind, i64)> {
        match *self {
            Self::Box { kind, extra } => Some((kind, extra)),
            Self::Toast(_) => None,
        }
    }
}

/// The slot the notice lives in. An open box is not replaced by a toast
/// (the user is already looking at a limit); a box replaces a toast that
/// has not been shown yet.
pub fn raise(slot: &mut Option<LimitNotice>, notice: LimitNotice) {
    if matches!(slot, Some(LimitNotice::Box { .. })) && matches!(notice, LimitNotice::Toast(_)) {
        return;
    }
    *slot = Some(notice);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toast_does_not_close_an_open_box_but_a_box_replaces_a_toast() {
        let mut slot = None;
        raise(
            &mut slot,
            LimitNotice::of(PremiumLimitKind::FavoriteStickers, 0),
        );
        assert_eq!(
            slot,
            Some(LimitNotice::Toast(PremiumLimitKind::FavoriteStickers))
        );
        raise(&mut slot, LimitNotice::of(PremiumLimitKind::Pins, 0));
        assert_eq!(
            slot.and_then(|n| n.open_box()),
            Some((PremiumLimitKind::Pins, 0))
        );
        raise(&mut slot, LimitNotice::of(PremiumLimitKind::OwnedBots, 0));
        assert_eq!(
            slot.and_then(|n| n.open_box()),
            Some((PremiumLimitKind::Pins, 0)),
            "the open box stays"
        );
    }
}

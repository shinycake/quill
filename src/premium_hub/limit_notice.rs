//! The one limit event a Premium limit error raises: a box that stays
//! until it is closed, or a toast the window shows once.
#![deny(clippy::indexing_slicing)]

use crate::premium_limits::PremiumLimitKind;

/// What a failed request that named a limit asks the window to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitNotice {
    /// The limit box is open (pins, caption, public links, ...), with how
    /// far over the limit the action went when that is known (caption).
    /// `toast` is a toast raised while the box is open: it waits for the
    /// box to close, or the window takes it at once.
    Box {
        kind: PremiumLimitKind,
        extra: i64,
        toast: Option<PremiumLimitKind>,
    },
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
            Self::Box {
                kind,
                extra,
                toast: None,
            }
        }
    }

    /// The open box.
    pub fn open_box(&self) -> Option<(PremiumLimitKind, i64)> {
        match *self {
            Self::Box { kind, extra, .. } => Some((kind, extra)),
            Self::Toast(_) => None,
        }
    }
}

impl LimitNotice {
    /// The toast waiting, under a box or alone.
    pub fn toast(&self) -> Option<PremiumLimitKind> {
        match *self {
            Self::Toast(kind) => Some(kind),
            Self::Box { toast, .. } => toast,
        }
    }

    /// The notice without its toast: the box stays open, or nothing.
    pub fn without_toast(self) -> Option<Self> {
        match self {
            Self::Toast(_) => None,
            Self::Box { kind, extra, .. } => Some(Self::Box {
                kind,
                extra,
                toast: None,
            }),
        }
    }
}

/// The slot the notice lives in. A toast raised over an open box waits
/// inside the box (the user is already looking at a limit); a box raised
/// over a toast that has not been shown yet keeps that toast.
pub fn raise(slot: &mut Option<LimitNotice>, notice: LimitNotice) {
    let waiting = slot.as_ref().and_then(|n| n.toast());
    *slot = Some(match (*slot, notice) {
        (Some(LimitNotice::Box { kind, extra, .. }), LimitNotice::Toast(toast)) => {
            LimitNotice::Box {
                kind,
                extra,
                toast: Some(toast),
            }
        }
        (_, LimitNotice::Box { kind, extra, toast }) => LimitNotice::Box {
            kind,
            extra,
            toast: toast.or(waiting),
        },
        (_, toast @ LimitNotice::Toast(_)) => toast,
    });
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

    #[test]
    fn a_toast_raised_over_a_box_is_kept_for_after_the_box() {
        let mut slot = None;
        raise(&mut slot, LimitNotice::of(PremiumLimitKind::Pins, 0));
        raise(
            &mut slot,
            LimitNotice::of(PremiumLimitKind::FavoriteStickers, 0),
        );
        assert_eq!(
            slot.and_then(|n| n.toast()),
            Some(PremiumLimitKind::FavoriteStickers)
        );
        assert_eq!(
            slot.and_then(|n| n.without_toast())
                .and_then(|n| n.open_box()),
            Some((PremiumLimitKind::Pins, 0))
        );
    }

    #[test]
    fn a_box_raised_over_a_waiting_toast_keeps_the_toast() {
        let mut slot = None;
        raise(
            &mut slot,
            LimitNotice::of(PremiumLimitKind::FavoriteStickers, 0),
        );
        raise(&mut slot, LimitNotice::of(PremiumLimitKind::Pins, 0));
        assert_eq!(
            slot.and_then(|n| n.toast()),
            Some(PremiumLimitKind::FavoriteStickers)
        );
        assert_eq!(
            slot.and_then(|n| n.open_box()),
            Some((PremiumLimitKind::Pins, 0))
        );
    }
}

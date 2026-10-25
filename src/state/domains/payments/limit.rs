//! Premium limit boxes and toasts: a failed request that named a limit
//! opens the box (or the toast) instead of an error line (tdesktop
//! `ShowImportError`, `ChannelsLimitBox`, `PublicLinksLimitBox`,
//! `PinsLimitBox`, `SublistsPinsLimitBox`, `CaptionLimitReachedBox`), and
//! the free, Premium and in-force values every box and toast shows.
use crate::premium_hub::promo::limit_pair;
use crate::premium_limits::{LimitPair, PremiumLimitKind, shown_limit};
use crate::state::*;

impl Session {
    /// Open the limit box for a request that failed on a Premium limit.
    /// True when it did, so the caller skips its generic error line.
    pub(crate) fn note_premium_limit(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) -> bool {
        let Some(mut kind) = err.premium_limit else {
            return false;
        };
        if kind == PremiumLimitKind::Pins {
            let archived = pending
                .and_then(|p| p.chat_id)
                .and_then(|id| self.chats.get(&id.0))
                .is_some_and(|chat| chat.in_archive);
            if archived {
                kind = PremiumLimitKind::PinsArchive;
            }
        }
        self.open_premium_limit(kind, 0);
        true
    }

    /// Show the box for `kind`, or its toast for the kinds tdesktop tells
    /// about with one.
    pub fn open_premium_limit(&mut self, kind: PremiumLimitKind, extra: i64) {
        if kind.shows_toast() {
            self.payments.hub.limit_toast = Some(kind);
        } else {
            self.payments.hub.limit_box = Some((kind, extra));
        }
    }

    /// The free and Premium value of `kind`: `getPremiumFeatures`, then
    /// `getPremiumLimit`, then tdesktop's fallback.
    pub fn premium_limit_pair(&self, kind: PremiumLimitKind) -> LimitPair {
        let hub = &self.payments.hub;
        hub.premium
            .as_ref()
            .and_then(|info| info.limit_pair(kind))
            .or_else(|| hub.limits.answered(kind))
            .unwrap_or_else(|| limit_pair(None, kind))
    }

    /// The limit in force for this account: TDLib's option where it keeps
    /// one, else the free or Premium value.
    pub fn premium_limit_in_force(&self, kind: PremiumLimitKind) -> i64 {
        let option = match kind {
            PremiumLimitKind::Pins => i64::from(self.chat_list.pinned_chat_count_max),
            PremiumLimitKind::PinsArchive => {
                i64::from(self.chat_list.pinned_archived_chat_count_max)
            }
            PremiumLimitKind::Caption => i64::from(self.messages.message_caption_length_max),
            _ => self.payments.hub.limits.option(kind).unwrap_or(0),
        };
        if option > 0 {
            option
        } else {
            shown_limit(self.premium_limit_pair(kind), self.my_is_premium())
        }
    }

    /// Whether pinning one more Saved Messages chat would go over the limit
    /// (tdesktop `Data::Session::pinnedCanPin` for a sublist).
    pub fn saved_pin_limit_reached(&self) -> bool {
        let pinned = self
            .threads
            .saved
            .topics
            .values()
            .filter(|topic| topic.is_pinned)
            .count();
        i64::try_from(pinned).unwrap_or(i64::MAX)
            >= self.premium_limit_in_force(PremiumLimitKind::SavedPins)
    }

    /// Whether this server sells Premium at all (TDLib's
    /// `is_premium_available` option; true until it says otherwise).
    pub fn premium_possible(&self) -> bool {
        self.payments.premium_available.unwrap_or(true)
    }

    /// Whether Premium marks show next to names and on stickers (tdesktop
    /// `Session::premiumBadgesShown`): a Premium account sees them, anyone
    /// else only where Premium can be bought.
    pub fn premium_badges_shown(&self) -> bool {
        self.my_is_premium() || self.premium_possible()
    }
}

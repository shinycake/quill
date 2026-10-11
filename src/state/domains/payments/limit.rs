//! Premium limit boxes: a failed request that named a limit opens the box
//! instead of an error line (tdesktop `ShowImportError`, `ChannelsLimitBox`,
//! `PublicLinksLimitBox`, `PinsLimitBox`, `CaptionLimitReachedBox`).
use crate::premium_limits::PremiumLimitKind;
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
        self.payments.hub.limit_box = Some((kind, 0));
        true
    }

    /// Whether this server sells Premium at all (TDLib's
    /// `is_premium_available` option; true until it says otherwise).
    pub fn premium_possible(&self) -> bool {
        self.payments.premium_available.unwrap_or(true)
    }
}

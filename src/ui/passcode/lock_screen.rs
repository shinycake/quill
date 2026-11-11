//! What the lock screen shows beside its field: nothing extra, the system
//! unlock prompt about to be offered, or the Log out confirmation. One
//! value, so offering the prompt and asking to confirm a log out never
//! both stand.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LockScreen {
    #[default]
    Entry,
    /// Locked with the system unlock on: the next frame offers the prompt
    /// once (`take_system_offer`).
    OfferSystemUnlock,
    /// "Log out" was pressed and waits for its confirm or Cancel.
    ConfirmLogout,
}

impl LockScreen {
    pub(crate) fn confirming_logout(self) -> bool {
        self == Self::ConfirmLogout
    }

    /// The screen was just locked: offer the prompt when it can be
    /// answered, or show the plain field.
    pub(crate) fn on_locked(offer_system_unlock: bool) -> Self {
        if offer_system_unlock {
            Self::OfferSystemUnlock
        } else {
            Self::Entry
        }
    }

    /// Whether the system prompt is to be offered now; asking once.
    pub(crate) fn take_system_offer(&mut self) -> bool {
        if *self == Self::OfferSystemUnlock {
            *self = Self::Entry;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LockScreen;

    #[test]
    fn the_prompt_is_offered_once_and_a_logout_confirm_replaces_it() {
        let mut screen = LockScreen::on_locked(true);
        assert!(screen.take_system_offer());
        assert!(!screen.take_system_offer());
        let mut screen = LockScreen::ConfirmLogout;
        assert!(screen.confirming_logout() && !screen.take_system_offer());
        assert_eq!(LockScreen::on_locked(false), LockScreen::Entry);
    }
}

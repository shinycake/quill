//! "Gift Stars": pick a contact, see the packs `getStarGiftPaymentOptions`
//! lists for them, and pay one in the checkout (tdesktop
//! `boxes/gift_credits_box.cpp`, `ShowGiftCreditsBox` and `GiftCreditsBox`).
//! tdesktop offers it as the "Send Stars" button on the Stars page; the
//! contact list leaves out yourself and bots.
#![deny(clippy::indexing_slicing)]
use crate::star_packs::StarPack;

/// The contact the Stars go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarsGiftTarget {
    pub user_id: i64,
    pub name: String,
}

/// The Gift Stars dialog.
#[derive(Debug, Clone, Default)]
pub struct StarsGiftState {
    pub open: bool,
    /// `None` while the contact list shows.
    pub target: Option<StarsGiftTarget>,
    pub options: Vec<StarPack>,
    pub loading: bool,
    pub error: Option<String>,
}

impl StarsGiftState {
    /// Back to the contact list.
    pub fn clear_target(&mut self) {
        self.target = None;
        self.options.clear();
        self.loading = false;
        self.error = None;
    }
}

/// Whether a contact can get Stars: someone else, not a bot.
pub fn can_gift_stars(user_id: i64, my_user_id: Option<i64>, is_bot: bool) -> bool {
    user_id > 0 && Some(user_id) != my_user_id && !is_bot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stars_go_to_other_people_but_not_bots() {
        assert!(can_gift_stars(5, Some(1), false));
        assert!(!can_gift_stars(1, Some(1), false));
        assert!(!can_gift_stars(5, Some(1), true));
        assert!(!can_gift_stars(0, None, false));
    }

    #[test]
    fn going_back_forgets_the_contact_and_packs() {
        let mut state = StarsGiftState {
            open: true,
            target: Some(StarsGiftTarget {
                user_id: 5,
                name: "Ada".into(),
            }),
            loading: true,
            error: Some("x".into()),
            ..Default::default()
        };
        state.clear_target();
        assert!(state.open && state.target.is_none() && !state.loading && state.error.is_none());
    }
}

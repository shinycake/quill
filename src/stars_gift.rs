//! "Gift Stars": pick a contact, see the packs `getStarGiftPaymentOptions`
//! lists for them, and pay one in the checkout (tdesktop
//! `boxes/gift_credits_box.cpp`, `ShowGiftCreditsBox` and `GiftCreditsBox`).
//! tdesktop offers it as the "Send Stars" button on the Stars page; the
//! contact list leaves out yourself and bots.
#![deny(clippy::indexing_slicing)]
use crate::ids::RequestId;
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
    pub stage: GiftStage,
}

/// What the dialog shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GiftStage {
    /// The contact list shows.
    #[default]
    Contacts,
    /// A contact was picked; its packs load, or are in.
    Packs {
        target: StarsGiftTarget,
        load: PackLoad,
    },
}

/// The packs `getStarGiftPaymentOptions` lists for the picked contact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackLoad {
    /// The request in flight. Only its answer or error lands: an earlier
    /// contact's, or one after Back or close, is dropped.
    Loading(RequestId),
    Ready(Vec<StarPack>),
    Failed(String),
}

impl StarsGiftState {
    /// Back to the contact list.
    pub fn clear_target(&mut self) {
        self.stage = GiftStage::Contacts;
    }

    pub fn target(&self) -> Option<&StarsGiftTarget> {
        match &self.stage {
            GiftStage::Packs { target, .. } => Some(target),
            GiftStage::Contacts => None,
        }
    }

    pub fn options(&self) -> &[StarPack] {
        match &self.stage {
            GiftStage::Packs {
                load: PackLoad::Ready(options),
                ..
            } => options,
            _ => &[],
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(
            self.stage,
            GiftStage::Packs {
                load: PackLoad::Loading(_),
                ..
            }
        )
    }

    pub fn error(&self) -> Option<&str> {
        match &self.stage {
            GiftStage::Packs {
                load: PackLoad::Failed(error),
                ..
            } => Some(error),
            _ => None,
        }
    }

    /// The packs of the contact picked: the answer of `request` only.
    pub fn answer(&mut self, request: Option<RequestId>, options: Vec<StarPack>) {
        if let Some(load) = self.waiting_load(request) {
            *load = PackLoad::Ready(options);
        }
    }

    /// The error of `request`, under the same rule as [`Self::answer`].
    pub fn fail(&mut self, request: Option<RequestId>, error: String) {
        if let Some(load) = self.waiting_load(request) {
            *load = PackLoad::Failed(error);
        }
    }

    fn waiting_load(&mut self, request: Option<RequestId>) -> Option<&mut PackLoad> {
        match &mut self.stage {
            GiftStage::Packs { load, .. } if *load == PackLoad::Loading(request?) => Some(load),
            _ => None,
        }
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
            stage: GiftStage::Packs {
                target: StarsGiftTarget {
                    user_id: 5,
                    name: "Ada".into(),
                },
                load: PackLoad::Failed("x".into()),
            },
        };
        state.clear_target();
        assert!(state.open && state.target().is_none() && state.error().is_none());
    }

    #[test]
    fn only_the_request_that_asked_fills_the_packs() {
        let mut state = StarsGiftState {
            open: true,
            stage: GiftStage::Packs {
                target: StarsGiftTarget {
                    user_id: 5,
                    name: "Ada".into(),
                },
                load: PackLoad::Loading(RequestId(2)),
            },
        };
        state.answer(Some(RequestId(1)), Vec::new());
        assert!(state.is_loading(), "an earlier contact's answer");
        state.fail(Some(RequestId(1)), "late".into());
        assert!(state.is_loading());
        state.fail(Some(RequestId(2)), "refused".into());
        assert_eq!(state.error(), Some("refused"));
        state.clear_target();
        state.answer(Some(RequestId(2)), Vec::new());
        assert!(state.target().is_none(), "an answer after Back is dropped");
    }
}

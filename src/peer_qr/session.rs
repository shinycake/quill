//! What the session keeps for the box: the backdrops the colors panel
//! offers, each peer's saved share theme, the gifts panel's list, the
//! current user's temporary contact link and the answers the open box
//! has not taken yet.

use super::free::{FreeCode, FreeRequest, FreeResult};
use super::link::{UserLinkData, UserLinkFetch};
use super::theme::{Backdrops, ShareTheme, ThemeChoice, ThemeGifts, ThemeOwner};
use crate::ids::RequestId;
use std::collections::{HashMap, HashSet};

/// A `getUserLink` answer before it is made absolute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserLinkAnswer {
    pub url: String,
    /// Seconds left; 0 when permanent.
    pub expires_in: i64,
    pub can_get_permanent_link: bool,
}

impl UserLinkAnswer {
    pub fn at(&self, now: i64) -> UserLinkData {
        UserLinkData {
            url: self.url.clone(),
            expires_at: if self.expires_in > 0 {
                now.saturating_add(self.expires_in)
            } else {
                0
            },
        }
    }
}

/// The users domain's share for the box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerQrData {
    /// `getProfileShareThemeBackdrops`, asked for again hourly.
    pub backdrops: Backdrops,
    /// Each peer's `share_theme` from its full info, or what a save sent
    /// (tdesktop `ShareThemes::_kept`).
    pub themes: HashMap<ThemeOwner, ShareTheme>,
    /// The gifts panel's owned upgraded gifts.
    pub gifts: ThemeGifts,
    /// `setProfileShareTheme` requests on their way, by request: whose
    /// theme and what was sent, kept until the answer or the request is
    /// found gone ([`Self::forget_dropped`]).
    pub saves: HashMap<RequestId, (ThemeOwner, ThemeChoice)>,
    /// Save answers in arrival order, by the request that was answered: the
    /// open box takes the one it waits for, the rest are dropped.
    pub save_answers: Vec<(RequestId, Result<ThemeChoice, String>)>,
    /// The plain `getUserLink`, for an account with no username and a
    /// hidden number.
    pub contact_link: UserLinkFetch,
    /// Free-code answers in arrival order, by the request that was
    /// answered.
    pub free_answers: Vec<(RequestId, FreeResult)>,
}

impl PeerQrData {
    pub fn accept_link(
        &mut self,
        free: Option<FreeRequest>,
        request: RequestId,
        answer: UserLinkAnswer,
        now: i64,
    ) {
        let link = answer.at(now);
        match free {
            None => {
                if self.contact_link == UserLinkFetch::Loading(request) {
                    self.contact_link = UserLinkFetch::Ready(link);
                }
            }
            Some(_) => self.free_answers.push((
                request,
                Ok(FreeCode {
                    link,
                    can_permanent: answer.can_get_permanent_link,
                }),
            )),
        }
    }

    /// `reason` is the error to show; empty when there is none to tell.
    pub fn fail_link(&mut self, free: Option<FreeRequest>, request: RequestId, reason: String) {
        match free {
            None => {
                if self.contact_link == UserLinkFetch::Loading(request) {
                    self.contact_link = UserLinkFetch::Failed(Some(reason));
                }
            }
            Some(_) => self.free_answers.push((request, Err(reason))),
        }
    }

    /// A new box asks again after a failure (tdesktop makes a new
    /// `OwnQrLinkState` for each box).
    pub fn retry_contact_link(&mut self) {
        if matches!(self.contact_link, UserLinkFetch::Failed(_)) {
            self.contact_link = UserLinkFetch::Idle;
        }
    }

    /// Why the last contact link failed, once: the box shows it and the
    /// failure stays without it.
    pub fn take_contact_error(&mut self) -> Option<String> {
        match &mut self.contact_link {
            UserLinkFetch::Failed(reason) => reason.take(),
            _ => None,
        }
    }

    pub fn take_free_answers(&mut self) -> Vec<(RequestId, FreeResult)> {
        std::mem::take(&mut self.free_answers)
    }

    /// The saved theme came in with `owner`'s full info.
    pub fn keep_theme(&mut self, owner: ThemeOwner, theme: ShareTheme) {
        self.themes.insert(owner, theme);
    }

    /// A save went out as `request`.
    pub fn start_save(&mut self, request: RequestId, owner: ThemeOwner, choice: ThemeChoice) {
        self.saves.insert(request, (owner, choice));
    }

    /// The server took the save `request`: the owner's theme is what was
    /// sent (tdesktop sets `_kept` on done).
    pub fn finish_save(&mut self, request: RequestId) {
        let Some((owner, choice)) = self.saves.remove(&request) else {
            return;
        };
        self.themes.insert(owner, choice.to_theme());
        self.save_answers.push((request, Ok(choice)));
    }

    /// `reason` is the error to show; empty when there is none to tell.
    pub fn fail_save(&mut self, request: RequestId, reason: String) {
        self.saves.remove(&request);
        self.save_answers.push((request, Err(reason)));
    }

    pub fn take_save_answers(&mut self) -> Vec<(RequestId, Result<ThemeChoice, String>)> {
        std::mem::take(&mut self.save_answers)
    }

    /// Something of the box waits for a request: a save, the gifts list or
    /// the contact link.
    pub fn waits(&self) -> bool {
        !self.saves.is_empty()
            || self.gifts.asking()
            || matches!(self.contact_link, UserLinkFetch::Loading(_))
    }

    /// Requests of the box that are no longer pending and were not answered
    /// (a reconnect or a client restart dropped them) end here: a save is
    /// forgotten, the gifts list and the contact link stop waiting. The
    /// open box's own save and free-code requests end through the
    /// answers the UI gives them when it finds them gone.
    pub fn forget_dropped(&mut self, pending: &HashSet<RequestId>) -> bool {
        let saves = self.saves.len();
        self.saves.retain(|request, _| pending.contains(request));
        let mut changed = self.saves.len() != saves;
        changed |= self.gifts.forget_dropped(pending);
        if let UserLinkFetch::Loading(request) = self.contact_link
            && !pending.contains(&request)
        {
            self.contact_link = UserLinkFetch::Failed(None);
            changed = true;
        }
        changed
    }
}

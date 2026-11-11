//! What the session keeps for the box: the backdrops the colors panel
//! offers, each peer's saved share theme, the gifts panel's list, the
//! current user's temporary contact link and the answers the open box
//! has not taken yet.

use crate::ids::RequestId;
use super::free::{FreeCode, FreeResult, FreeTicket};
use super::link::{UserLinkData, UserLinkFetch};
use super::state::SaveTicket;
use super::theme::{Backdrops, ShareTheme, ThemeChoice, ThemeGifts, ThemeOwner};
use std::collections::HashMap;

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
    /// `setProfileShareTheme` requests on their way.
    pub saves: Vec<(SaveTicket, ThemeOwner, ThemeChoice)>,
    /// Save answers in arrival order.
    pub save_answers: Vec<(SaveTicket, Result<ThemeChoice, String>)>,
    /// The plain `getUserLink`, for an account with no username and a
    /// hidden number.
    pub contact_link: UserLinkFetch,
    /// Free-code answers in arrival order.
    pub free_answers: Vec<(FreeTicket, FreeResult)>,
}

impl PeerQrData {
    pub fn accept_link(
        &mut self,
        free: Option<FreeTicket>,
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
            Some(ticket) => self.free_answers.push((
                ticket,
                Ok(FreeCode {
                    link,
                    can_permanent: answer.can_get_permanent_link,
                }),
            )),
        }
    }

    /// `reason` is the error to show; empty when there is none to tell.
    pub fn fail_link(&mut self, free: Option<FreeTicket>, request: RequestId, reason: String) {
        match free {
            None => {
                if self.contact_link == UserLinkFetch::Loading(request) {
                    self.contact_link = UserLinkFetch::Failed(Some(reason));
                }
            }
            Some(ticket) => self.free_answers.push((ticket, Err(reason))),
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

    pub fn take_free_answers(&mut self) -> Vec<(FreeTicket, FreeResult)> {
        std::mem::take(&mut self.free_answers)
    }

    /// The saved theme came in with `owner`'s full info.
    pub fn keep_theme(&mut self, owner: ThemeOwner, theme: ShareTheme) {
        self.themes.insert(owner, theme);
    }

    /// A save went out.
    pub fn start_save(&mut self, ticket: SaveTicket, owner: ThemeOwner, choice: ThemeChoice) {
        self.saves.push((ticket, owner, choice));
    }

    /// The server took the save: the owner's theme is what was sent
    /// (tdesktop sets `_kept` on done).
    pub fn finish_save(&mut self, ticket: SaveTicket) {
        let Some(at) = self.saves.iter().position(|(t, _, _)| *t == ticket) else {
            return;
        };
        let (ticket, owner, choice) = self.saves.remove(at);
        self.themes.insert(owner, choice.to_theme());
        self.save_answers.push((ticket, Ok(choice)));
    }

    /// `reason` is the error to show; empty when there is none to tell.
    pub fn fail_save(&mut self, ticket: SaveTicket, reason: String) {
        self.saves.retain(|(t, _, _)| *t != ticket);
        self.save_answers.push((ticket, Err(reason)));
    }

    pub fn take_save_answers(&mut self) -> Vec<(SaveTicket, Result<ThemeChoice, String>)> {
        std::mem::take(&mut self.save_answers)
    }
}

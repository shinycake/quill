//! What the session keeps for the box: the backdrops the colors panel
//! offers, the current user's temporary contact link and the free-code
//! answers the open box has not taken yet.

use super::appearance::ShareThemeBackdrop;
use super::free::{FreeCode, FreeResult, FreeTicket};
use super::link::{UserLinkData, UserLinkFetch};

/// `getProfileShareThemeBackdrops`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum BackdropsFetch {
    #[default]
    Idle,
    Loading,
    Ready(Vec<ShareThemeBackdrop>),
    Failed,
}

impl BackdropsFetch {
    pub fn backdrops(&self) -> &[ShareThemeBackdrop] {
        match self {
            BackdropsFetch::Ready(list) => list,
            _ => &[],
        }
    }
}

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
    pub backdrops: BackdropsFetch,
    /// The plain `getUserLink`, for an account with no username and a
    /// hidden number.
    pub contact_link: UserLinkFetch,
    /// Why the last contact link failed, until the box has shown it.
    pub contact_error: Option<String>,
    /// Free-code answers in arrival order.
    pub free_answers: Vec<(FreeTicket, FreeResult)>,
}

impl PeerQrData {
    pub fn accept_link(&mut self, free: Option<FreeTicket>, answer: UserLinkAnswer, now: i64) {
        let link = answer.at(now);
        match free {
            None => self.contact_link = UserLinkFetch::Ready(link),
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
    pub fn fail_link(&mut self, free: Option<FreeTicket>, reason: String) {
        match free {
            None => {
                self.contact_link = UserLinkFetch::Failed;
                self.contact_error = Some(reason);
            }
            Some(ticket) => self.free_answers.push((ticket, Err(reason))),
        }
    }

    /// A new box asks again after a failure (tdesktop makes a new
    /// `OwnQrLinkState` for each box).
    pub fn retry_contact_link(&mut self) {
        if self.contact_link == UserLinkFetch::Failed {
            self.contact_link = UserLinkFetch::Idle;
        }
        self.contact_error = None;
    }

    pub fn take_free_answers(&mut self) -> Vec<(FreeTicket, FreeResult)> {
        std::mem::take(&mut self.free_answers)
    }
}

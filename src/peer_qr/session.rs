//! What the session keeps for the box: the backdrops the colors panel
//! offers, the current user's temporary contact link and the free-code
//! answers the open box has not taken yet.

use super::appearance::ShareThemeBackdrop;
use super::free::{FreeCode, FreeRequest};
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
    /// Free-code answers in arrival order; `None` when the request failed.
    pub free_answers: Vec<(FreeRequest, Option<FreeCode>)>,
}

impl PeerQrData {
    pub fn accept_link(&mut self, free: Option<FreeRequest>, answer: UserLinkAnswer, now: i64) {
        let link = answer.at(now);
        match free {
            None => self.contact_link = UserLinkFetch::Ready(link),
            Some(request) => self.free_answers.push((
                request,
                Some(FreeCode {
                    link,
                    can_permanent: answer.can_get_permanent_link,
                }),
            )),
        }
    }

    pub fn fail_link(&mut self, free: Option<FreeRequest>) {
        match free {
            None => self.contact_link = UserLinkFetch::Failed,
            Some(request) => self.free_answers.push((request, None)),
        }
    }

    pub fn take_free_answers(&mut self) -> Vec<(FreeRequest, Option<FreeCode>)> {
        std::mem::take(&mut self.free_answers)
    }
}

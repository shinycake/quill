//! Share themes (`parity:profile-share-theme`): each peer's saved theme
//! from its full info, the gifts panel's pages, save answers, and what
//! the QR box reads about a peer (whose theme it is, who may save it).
use crate::peer_qr::state::QrPeer;
use crate::peer_qr::theme::{SaveRights, ShareTheme, ThemeGift, ThemeOwner};
use crate::premium_hub::GiftsPage;
use crate::state::*;

impl Session {
    /// Keep `owner`'s saved theme and the files its gift draws from.
    pub(crate) fn keep_share_theme(&mut self, owner: ThemeOwner, theme: ShareTheme) {
        self.remember_files(theme.files());
        self.users_state.peer_qr.keep_theme(owner, theme);
    }

    /// A page of the gifts panel's list: only the current request's for
    /// the list's owner, only upgraded gifts that are not burned.
    pub(crate) fn apply_share_theme_gifts(
        &mut self,
        page: GiftsPage,
        owner: ThemeOwner,
        serial: u64,
        first: bool,
    ) {
        if !self.users_state.peer_qr.gifts.is_current(owner, serial) {
            return;
        }
        let gifts: Vec<ThemeGift> = page
            .gifts
            .into_iter()
            .filter_map(|received| {
                let gift = received.gift.collectible.as_deref()?;
                (!gift.is_burned).then(|| ThemeGift::from_collectible(gift, received.id.clone()))
            })
            .collect();
        for gift in &gifts {
            self.remember_files(&gift.files);
        }
        self.users_state
            .peer_qr
            .gifts
            .accept(gifts, page.next_offset, first);
    }

    /// `setProfileShareTheme` answered `ok`.
    pub(crate) fn apply_share_theme_ok(&mut self, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Users(UsersPurpose::SetProfileShareTheme { ticket })) =
            pending.map(|p| p.purpose)
        {
            self.users_state.peer_qr.finish_save(ticket);
        }
    }

    /// Whose theme the box for `peer` shows: a user's, or a supergroup's
    /// or channel's.
    pub fn qr_theme_owner(&self, peer: QrPeer) -> Option<ThemeOwner> {
        match peer {
            QrPeer::User(user_id) => Some(ThemeOwner::User(user_id)),
            QrPeer::Chat(chat_id) => match self.chats.get(&chat_id)?.kind {
                ChatKind::Supergroup { supergroup_id, .. } => {
                    Some(ThemeOwner::Supergroup(supergroup_id))
                }
                _ => None,
            },
        }
    }

    /// `peer`'s saved theme, once its full info has come.
    pub fn qr_served_theme(&self, peer: QrPeer) -> Option<&ShareTheme> {
        let owner = self.qr_theme_owner(peer)?;
        self.users_state.peer_qr.themes.get(&owner)
    }

    /// What decides whether you may save `peer`'s theme.
    pub fn qr_save_rights(&self, peer: QrPeer) -> SaveRights {
        match peer {
            QrPeer::User(user_id) if self.my_user_id == Some(user_id) => SaveRights::Me,
            QrPeer::User(_) => SaveRights::OtherUser,
            QrPeer::Chat(chat_id) => {
                let Some(chat) = self.chats.get(&chat_id) else {
                    return SaveRights::Other;
                };
                match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => SaveRights::Chat {
                        has_username: self
                            .supergroup_username(supergroup_id)
                            .is_some_and(|name| !name.is_empty()),
                        can_change_info: chat.can_change_info(),
                    },
                    _ => SaveRights::Other,
                }
            }
        }
    }
}

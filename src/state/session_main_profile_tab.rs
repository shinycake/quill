//! The tab a profile opens on (`userFullInfo.main_profile_tab`).
use super::*;

impl Session {
    /// The shared-media tab a private chat's profile gallery opens on:
    /// the user's main profile tab when it has a gallery tab, else Media.
    pub fn main_profile_gallery_tab(&self, chat_id: ChatId) -> SharedMediaTab {
        self.private_chat_user_id(chat_id)
            .and_then(|user_id| self.user_full_info(user_id))
            .and_then(|info| info.extras.main_profile_tab)
            .and_then(|tab| tab.shared_media_tab())
            .unwrap_or_default()
    }

    /// Your own main profile tab, as last reported by `userFullInfo`.
    pub fn my_main_profile_tab(&self) -> Option<crate::profile_tab::ProfileTab> {
        let me = self.my_user_id?;
        self.user_full_info(me)?.extras.main_profile_tab
    }
}

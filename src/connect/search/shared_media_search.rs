//! Connect driver: search inside a gallery tab (tdesktop `info/media`
//! search field). The query rides on the tab's `searchChatMessages` page
//! requests next to its filter.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// The query `tab`'s pages are requested with.
    pub(super) fn shared_media_query(&self, tab: SharedMediaTab) -> String {
        self.session
            .media
            .shared_media
            .tab_at(tab)
            .map(|state| state.query.clone())
            .unwrap_or_default()
    }

    /// Search `tab` for `text` (an empty text goes back to the plain list)
    /// and fetch the first page of the answer. A no-op when `tab` is not
    /// the open tab (the text was typed for a tab since left) or already
    /// answers that query. The caller debounces typing.
    pub fn search_shared_media(
        &mut self,
        tab: SharedMediaTab,
        text: &str,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.media.shared_media.active_tab != tab {
            return Ok(());
        }
        if !self
            .session
            .media
            .shared_media
            .begin_search(tab, text.trim())
        {
            return Ok(());
        }
        self.fetch_shared_media(tab)
    }
}

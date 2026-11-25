//! The header and the bars above a chat's history: one list, so the
//! stacked history and the Header slice (Floating) show the same bars in
//! the same order (docs/decisions/floating-layout.md).

use super::*;

impl QuillApp {
    /// The header and every bar above the history's body, top to bottom.
    #[inline(never)]
    pub(super) fn history_bars(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let session = self.session();
        let open = session.and_then(|s| s.open_chat_id());
        let chat = open.and_then(|id| session.and_then(|s| s.chats.get(&id.0)));
        let title = chat
            .map(|c| c.title.clone())
            .unwrap_or_else(|| tr(keys::Q_CHAT_NO_CHAT_SELECTED).into());
        let chat_actions = chat.and_then(|chat| {
            chat.supported().then_some((
                chat.id,
                chat.is_muted(),
                chat.notification_settings.is_muted_forever(),
                chat.in_archive(),
            ))
        });
        let peer_typing = chat.is_some_and(|c| c.is_peer_typing());
        let tabs_used =
            open.is_some_and(|id| session.is_some_and(|s| s.subsection_tabs_used_for(id)));
        let topic_info = open.and_then(|id| session.and_then(|s| s.open_topic_info(id)));
        let chat_search_open = session.is_some_and(|s| s.search.chat_search.is_open());
        let mut bars = vec![
            self.conversation_header(&title, chat_actions, peer_typing, cx)
                .into_any_element(),
        ];
        if let Some(info) = topic_info.clone().filter(|_| !tabs_used) {
            bars.push(self.forum_topic_strip(&info, cx).into_any_element());
        }
        if let Some(info) = topic_info.filter(|_| self.chat_ui.info_card.topic()) {
            bars.push(self.topic_info_card(&info, cx));
        }
        bars.extend(self.subsection_tabs_strip(SubsectionTabsMode::Top, cx));
        bars.extend(self.saved_tags_bar(cx));
        bars.extend(self.bot_info_panel(cx));
        if let Some(chat_id) = open {
            bars.extend(self.chat_top_bars(chat_id, cx));
            bars.extend(self.pinned_message_banner(chat_id, cx));
            if self.pinned_list_open_for(chat_id) {
                bars.extend(self.pinned_list_panel(chat_id, cx));
            }
        }
        bars.extend(self.pending_bot_reply(cx));
        if chat_search_open {
            bars.push(self.chat_search_bar(cx).into_any_element());
        }
        bars
    }
}

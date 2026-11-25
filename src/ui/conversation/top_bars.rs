//! The header and the bars above a chat's history: one list, so the
//! stacked history and the Header slice (Floating) show the same bars in
//! the same order (docs/decisions/floating-layout.md).

use super::*;

impl QuillApp {
    /// The history shows a reply thread's list (`history_body`).
    fn thread_shown(&self) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        !session.thread_unavailable()
            && session.open_chat_id().is_some_and(|id| {
                session.thread_for_chat(id).is_some_and(|thread| {
                    !matches!(thread.status, quill::state::ThreadStatus::Failed(_))
                })
            })
    }

    /// The header slice's content when the bars float: one card with the
    /// header on top and every bar over the history under it, an inset
    /// hairline apart (the player, call and capture bars first, then the
    /// chat's own). With no bars it is the header pill. It blocks clicks
    /// to the rows under it.
    #[inline(never)]
    pub(super) fn floating_bars(&mut self, mode: PaneMode, cx: &mut Context<Self>) -> AnyElement {
        let (header, bars) = match mode {
            PaneMode::Ready => {
                let (header, bars) = self.history_bars(cx);
                (Some(header), bars)
            }
            PaneMode::Synthetic | PaneMode::Connecting => (None, Vec::new()),
        };
        let calls = [
            self.player_bar(cx).map(IntoElement::into_any_element),
            self.call_bar(cx)
                .or_else(|| self.group_call_bar(cx))
                .map(IntoElement::into_any_element),
            self.capture_notice(cx).map(IntoElement::into_any_element),
        ];
        let rows: Vec<AnyElement> = header
            .into_iter()
            .chain(calls.into_iter().flatten())
            .chain(bars)
            .collect();
        if rows.is_empty() {
            return Empty.into_any_element();
        }
        let inset = px(self.density().header_inset_x);
        let mut card = div()
            .id("conversation-bars-card")
            .flex()
            .flex_col()
            .block_mouse_except_scroll()
            .map(|this| self.stack_card(this, cx))
            .map(crate::ui::chrome::frames::card_clip);
        for (ix, row) in rows.into_iter().enumerate() {
            if ix > 0 {
                card = card.child(crate::ui::chrome::bars::card_divider(inset, cx.theme()));
            }
            card = card.child(row);
        }
        card.into_any_element()
    }

    /// The header, and every other bar above the history's body, top to
    /// bottom.
    #[inline(never)]
    pub(super) fn history_bars(&self, cx: &mut Context<Self>) -> (AnyElement, Vec<AnyElement>) {
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
        let header = self
            .conversation_header(&title, chat_actions, peer_typing, cx)
            .into_any_element();
        let mut bars = Vec::new();
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
        // A reply thread's root bar and info card sit over its list
        // (`thread_history_pane`); floating, they join the bars.
        if self.bars_float() && self.thread_shown() {
            bars.extend(self.thread_root_bar(cx));
            if self.chat_ui.info_card.thread() {
                bars.extend(self.thread_info_card(cx));
            }
        }
        (header, bars)
    }
}

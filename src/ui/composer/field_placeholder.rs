//! The compose field's placeholder follows the chat (tdesktop
//! `updateFieldPlaceholder`): "New message…" while editing, "Message for N
//! Stars" in paid chats, "Broadcast a message…" in channels.

use super::*;
use quill::composer::{ComposerPlaceholder, PlaceholderFacts};

impl QuillApp {
    /// What the open chat says about the field.
    fn placeholder_facts(&self) -> PlaceholderFacts {
        let Some(session) = self.session() else {
            return PlaceholderFacts::default();
        };
        let Some(chat) = session.open_chat else {
            return PlaceholderFacts::default();
        };
        let broadcast = session
            .chats
            .get(&chat.0)
            .is_some_and(|chat| chat.is_channel());
        PlaceholderFacts {
            editing: self.composer_ui.pending_edit.is_some(),
            stars_per_message: session.paid_send_price(chat),
            broadcast,
            silent: broadcast && self.composer_effective_silent(),
        }
    }

    /// Frame upkeep: put the placeholder the state asks for in the field.
    pub(in crate::ui) fn sync_composer_placeholder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = ComposerPlaceholder::for_facts(self.placeholder_facts()).text();
        if self.composer_ui.placeholder_text == text {
            return;
        }
        self.composer_ui.placeholder_text = text.clone();
        self.composer
            .update(cx, |input, cx| input.set_placeholder(text, window, cx));
    }
}

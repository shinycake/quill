//! The compose field's placeholder follows the chat (tdesktop
//! `updateFieldPlaceholder`): "New message…" while editing, "Message for N
//! Stars" in paid chats, "Broadcast a message…" in channels.

use super::*;
use quill::composer::{ComposerPlaceholder, PlaceholderFacts};
use quill::lang::{keys, tr};

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
    /// It is derived here, not at the event sites, because `set_placeholder`
    /// needs a `Window` and the Stars answer arrives in `poll_live`. Only a
    /// change of facts or language rebuilds the text.
    pub(in crate::ui) fn sync_composer_placeholder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let facts = self.placeholder_facts();
        // A new language pack hands out new strings, so the address of any
        // one of them tells the language changed.
        let language = tr(keys::MESSAGE_PH).as_ptr() as usize;
        if self.composer_ui.placeholder_shown == Some((facts, language)) {
            return;
        }
        self.composer_ui.placeholder_shown = Some((facts, language));
        let text = ComposerPlaceholder::for_facts(facts).text();
        self.composer
            .update(cx, |input, cx| input.set_placeholder(text, window, cx));
    }
}

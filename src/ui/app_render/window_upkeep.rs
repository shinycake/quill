//! The per-window half of the frame upkeep: what keeps one window's chat,
//! composer and history in step, run by every window's frame with that
//! window's state in place. The main window runs it from `frame_upkeep`,
//! a chat window from its own render (`ChatWindowView::render`), so a step
//! added here serves both; a step in `frame_upkeep` serves the main window
//! only and says why (`scripts/check-window-scope.py`).

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};

impl QuillApp {
    /// One window's frame upkeep, with that window's state in place.
    pub(in crate::ui) fn per_window_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A resolved comment thread moves the view into its discussion group.
        self.advance_thread(window, cx);
        // A deleted message's reply or edit bar goes.
        self.cancel_gone_composer_targets(window, cx);
        // What was open for a chat the view no longer shows goes.
        self.sync_chat_ui();
        // The histories every window still reads stay whole.
        self.sync_history_holds();
        // The inline results button's round trip back to its chat.
        self.flush_inline_return(window, cx);
        // A topic or thread switch without a window at hand asked for the
        // new slot's draft to be restored into the composer.
        self.restore_draft_if_pending(window, cx);
        // Text an open handed over goes in after the draft it lands on.
        self.apply_compose_after_open(window, cx);
        self.adopt_remote_draft(window, cx);
        self.sync_composer_placeholder(window, cx);
        // Emoji keywords for the `:` popup arrived.
        self.refresh_suggest_menu(cx);
        // Words with emoji: picked again when the text, caret or answers move.
        self.sync_emoji_words(cx);
        self.apply_ai_composer_answer(window, cx);
        // The countdowns tick while the chat shows a running one: slow
        // mode on the send button, self-destruct badges, live locations.
        self.ensure_slow_mode_tick(cx);
        self.ensure_self_destruct_tick(cx);
        self.ensure_live_location_tick(cx);
        // A clicked mention, hashtag, command or link (`entity_links`).
        self.run_pending_link(window, cx);
    }

    /// The per-window half of the poll loop, run once for every window
    /// with its state in place (`in_every_window`): the open chat's
    /// translation and Saved Messages' lists. Returns whether anything
    /// changed.
    pub(in crate::ui) fn poll_every_window(&mut self, cx: &mut Context<Self>) -> bool {
        let mut progressed = false;
        self.in_every_window(cx, |app, _| {
            progressed |= app.pump_translation();
            app.pump_saved_messages();
        });
        progressed
    }

    /// An AI answer for the composer of the chat this window shows
    /// replaces the draft (`msg-richtext-ai-tools`). An answer for a chat
    /// no window shows is dropped; one for another window's chat waits for
    /// that window's frame. Rich answers are written back as editor markup,
    /// inline formatting included, so the rich send path rebuilds the whole
    /// message.
    fn apply_ai_composer_answer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.session_mut() else {
            return;
        };
        let open_chat = session.open_chat;
        let ours = |chat: ChatId, session: &quill::state::Session| {
            open_chat == Some(chat) || !session.chat_is_shown(chat)
        };
        let ai_text = match session.messages.ai_composer_text.as_ref() {
            Some((chat, _)) if ours(*chat, session) => session.messages.ai_composer_text.take(),
            _ => None,
        };
        let ai_blocks = match session.messages.ai_composer_blocks.as_ref() {
            Some((chat, _, _)) if ours(*chat, session) => {
                session.messages.ai_composer_blocks.take()
            }
            _ => None,
        };
        if let Some((chat_id, text)) = ai_text
            && open_chat == Some(chat_id)
        {
            self.set_composer_markup(&text, window, cx);
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_REST_AI_UPDATED_THE_DRAFT)));
        }
        if let Some((chat_id, rich, note)) = ai_blocks
            && open_chat == Some(chat_id)
        {
            let text = quill::rich::RichDraft::from_message(&rich.blocks)
                .map(|draft| draft.markup())
                .unwrap_or_default();
            self.set_composer_markup(&text, window, cx);
            self.connection.set_note(StatusNote::info(note));
        }
    }
}

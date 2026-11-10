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
        // Whether a screen reader reads this window, for its history rows.
        self.slices
            .window
            .a11y_active
            .set(self.a11y_tree_wanted(window));
        // The window's menus and layers take and give back its focus, and
        // the menu's items claim their handles afresh for this frame.
        self.menu_focus_upkeep(window, cx);
        // A resolved comment thread moves the view into its discussion group.
        self.advance_thread(window, cx);
        // A deleted message's reply or edit bar goes.
        self.cancel_gone_composer_targets(window, cx);
        // The reply bar's message, when it is not in memory: each window's
        // own bar (the driver asks once per message).
        self.fetch_missing_reply_target();
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
        // MED4b: debounced `getLinkPreview` prefetch for this window's
        // detected-URL chip (at most one timer per new URL; the session
        // keeps a preview per URL).
        self.maybe_prefetch_link_preview(cx);
        // Two windows' GIF tabs share one list: the pressed one's GIFs.
        self.gif_tab_upkeep(cx);
        // Emoji keywords for the `:` popup arrived.
        self.refresh_suggest_menu(cx);
        // Words with emoji: picked again when the text, caret or answers move.
        self.sync_emoji_words(cx);
        self.apply_ai_composer_answer(window, cx);
        // The AI compose box and the apply-in-place shortcut opened over
        // this window's composer: their answers and toasts.
        self.ai_compose_window_upkeep(window, cx);
        // The countdowns tick while the chat shows a running one: slow
        // mode on the send button, self-destruct badges, live locations.
        self.ensure_slow_mode_tick(cx);
        self.ensure_self_destruct_tick(cx);
        self.ensure_live_location_tick(cx);
        // A clicked mention, hashtag, command or link (`entity_links`).
        self.run_pending_link(window, cx);
        // `parity:platform-deep-links`: the chat or box a deep link
        // resolved to, in the frame of the window it belongs to.
        self.run_resolved_deep_link(window, cx);
        // The transcription quota's toast, in the window of the last press
        // (the one whose Transcribe used a trial).
        if self.windows.pressed_here() {
            self.sync_speech_trial_hint(window, cx);
        }
        // A Premium preview a reducer asked for, in the window showing
        // the chat it is about.
        self.pump_promo_request(cx);
        // A Premium limit's toast (a full GIF list, too many pins), in the
        // window of the last press: the one whose action hit the limit.
        if self.windows.pressed_here() {
            self.premium_limit_toast_upkeep(window, cx);
        }
        self.gift_boxes_upkeep(window, cx);
        // A video chosen for an owned set finished downloading: its editor,
        // in the window the pick was made in.
        self.sticker_box_window_upkeep(window, cx);
        self.call_boxes_upkeep(window, cx);
        self.box_answers_upkeep(window, cx);
        self.checkout_prefill_upkeep(window, cx);
        // An edit-contact box opened before the full user info: its note.
        if self.windows.dialog_may_show_in(
            crate::ui::shell::DialogKind::ProfilePanel,
            self.windows.scope,
        ) {
            self.edit_contact_note_upkeep(window, cx);
        }
        // A Business screen opened before the account's full info arrived
        // is built from it at the first frame after it lands, in the
        // window that hosts the box (its inputs need that window).
        if self
            .windows
            .dialog_may_show_in(crate::ui::shell::DialogKind::Business, self.windows.scope)
        {
            self.business_info_upkeep(window, cx);
        }
        self.business_closed_error_upkeep();
        // Phase 4.6: push the playback clock into the seek sliders so the
        // thumb follows elapsed time (the tick has no `&mut Window`).
        self.sync_seek_slider(window, cx);
    }

    /// The video chat and stream boxes' follow-ups, in the window that
    /// hosts the box (`dialog_may_show_in`): "Start video chat" and
    /// "Stream with..." pressed in a chat window's header go on there with
    /// the main window minimized, and their error toasts show there.
    fn call_boxes_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::shell::DialogKind;
        let here = self.windows.scope;
        // Starting a video chat: the "join as" list, a refused create.
        if self
            .windows
            .dialog_may_show_in(DialogKind::GroupCallStart, here)
        {
            self.group_call_create_frame_upkeep(window, cx);
        }
        // "Stream with...": the server error of a failed request.
        if self.windows.dialog_may_show_in(DialogKind::RtmpStart, here) {
            self.rtmp_start_frame_upkeep(window, cx);
        }
    }

    /// The boxes whose answers fill their fields, in the window that hosts
    /// them: the folder editor's spec, a collectible's failed lookup.
    fn box_answers_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::shell::DialogKind;
        let here = self.windows.scope;
        // Parity slice: prefill the folder editor once its `getChatFolder`
        // spec arrives.
        if self
            .windows
            .dialog_may_show_in(DialogKind::FolderEditor, here)
        {
            self.maybe_prefill_folder_editor(window, cx);
        }
        // A collectible lookup that failed copies its link, with the toast
        // in the box's window.
        if self
            .windows
            .dialog_may_show_in(DialogKind::CollectibleInfo, here)
        {
            self.pump_collectible_info(cx);
        }
    }

    /// Slice P1 fix-up: the checkout opens on Buy before the form arrives;
    /// the saved order info and card go in once, at the first frame after
    /// the form lands, of the window that hosts the box (a Buy in a chat
    /// window does not wait for a main window that may be minimized). It
    /// can't live in `poll_live`: the inputs need a `&mut Window`.
    fn checkout_prefill_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = self.windows.scope;
        if !self.payments.dialog.as_ref().is_some_and(|d| !d.prefilled)
            || !self
                .windows
                .dialog_may_show_in(crate::ui::shell::DialogKind::PaymentForm, here)
        {
            return;
        }
        let payment_form = self.session().and_then(|s| s.payments.form.clone());
        if let (Some(dialog), Some(form)) = (self.payments.dialog.as_mut(), payment_form) {
            dialog.prefill_from_form(&form, window, cx);
        }
    }

    /// The gift and Grams boxes' follow-ups, each in the window that hosts
    /// its kit dialog (`dialog_may_show_in`): a box opened in a chat window
    /// goes on while the main window is minimized or closed, and its
    /// toasts and password field are that window's.
    fn gift_boxes_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::shell::DialogKind;
        let here = self.windows.scope;
        // Gift market: an auction whose state arrived, the followed
        // auction, the forge, toasts and sticker downloads.
        if self
            .windows
            .dialog_may_show_in(DialogKind::GiftMarket, here)
        {
            self.gift_market_frame_upkeep(window, cx);
        }
        // Gift transfer: Fragment's export page, the transferred toast.
        if self
            .windows
            .dialog_may_show_in(DialogKind::GiftTransfer, here)
        {
            self.gift_transfer_frame_upkeep(window, cx);
        }
        // Grams: Fragment's withdrawal page, the password field.
        if self
            .windows
            .dialog_may_show_in(DialogKind::GramsWithdraw, here)
        {
            self.grams_withdraw_frame_upkeep(window, cx);
        }
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
            self.set_note(StatusNote::silent(tr(keys::Q_REST_AI_UPDATED_THE_DRAFT)));
        }
        if let Some((chat_id, rich, note)) = ai_blocks
            && open_chat == Some(chat_id)
        {
            let text = quill::rich::RichDraft::from_message(&rich.blocks)
                .map(|draft| draft.markup())
                .unwrap_or_default();
            self.set_composer_markup(&text, window, cx);
            self.set_note(StatusNote::info(note));
        }
    }
}

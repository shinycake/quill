//! Screenshot demos: messages.

use crate::ui::app::QuillApp;
use crate::ui::chat_list::{apply_ready_mute_archive, apply_ready_pin};
use crate::ui::composer::apply_ready_reply;
use crate::ui::demo::demo_media_allowlist;
use crate::ui::demo::{seed_ready_media_session, seed_ready_send_media_session};
use crate::ui::find_demo::{
    apply_ready_jump_date, apply_ready_search_filters, apply_ready_search_frequent,
    apply_ready_search_from, apply_ready_search_from_hits, apply_ready_search_public,
};
use crate::ui::forward::apply_ready_forward;
use crate::ui::reactions::apply_ready_reactions;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::search_ui::{apply_ready_search, apply_ready_search_in_chat};
use crate::ui::*;
use gpui_kit::*;
use quill::composer::{ComposerEdit, ComposerReplyTo, DeleteConfirm, ForwardDraft};
use quill::ids::{ChatId, MessageId};
use std::sync::atomic::Ordering;

register_demos![
    // Own-message edit mode + delete confirm (injected, no live Telegram).
    DemoSpec::chats(
        "ready-edit-delete",
        "screenshot demo — edit + delete own messages (injected)"
    )
    .setup(QuillApp::demo_ready_edit_delete),
    // Edit bar above the composer for an outgoing photo (caption edit).
    DemoSpec::ready(
        "ready-edit-media",
        seed_ready_send_media_session,
        "screenshot demo — edit bar with a media thumbnail (injected)"
    )
    .setup(QuillApp::demo_ready_edit_media),
    // Forward select + dest picker + success (injected, no live Telegram).
    DemoSpec::chats(
        "ready-forward",
        "screenshot demo — forward message(s) (injected forwardMessages)"
    )
    .setup(QuillApp::demo_ready_forward),
    // Find in history: the "Jump to date" calendar box.
    DemoSpec::chats(
        "ready-jump-date",
        "screenshot demo — find in history (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_find_in_history(FindInHistoryDemo::JumpDate, window, cx)),
    // Mute presets + muted icon + archive section (injected, no live Telegram).
    DemoSpec::chats(
        "ready-mute-archive",
        "screenshot demo — mute / archive (injected notification + chat list updates)"
    )
    .setup(QuillApp::demo_ready_mute_archive),
    // Pin / unpin + pinned banner (injected, no live Telegram).
    DemoSpec::chats(
        "ready-pin",
        "screenshot demo — pin / unpin (injected updateMessageIsPinned)"
    )
    .setup(QuillApp::demo_ready_pin),
    // Emoji react / unreact + chips (injected, no live Telegram).
    DemoSpec::chats(
        "ready-reactions",
        "screenshot demo — emoji reactions (injected interaction_info)"
    )
    .setup(QuillApp::demo_ready_reactions),
    // Reply-to-message: composer quote + history quote strip.
    DemoSpec::chats(
        "ready-reply",
        "screenshot demo — reply to message (injected reply_to)"
    )
    .setup(QuillApp::demo_ready_reply),
    // Reply bar above the composer for a photo message, with its thumbnail.
    DemoSpec::ready(
        "ready-reply-media",
        seed_ready_media_session,
        "screenshot demo — reply bar with a media thumbnail (injected)"
    )
    .setup(QuillApp::demo_ready_reply_media),
    // A new message revealing at the bottom of the history; freeze the
    // frame with `QUILL_MOTION_HOLD_MS`.
    DemoSpec::chats(
        "ready-reveal",
        "screenshot demo — new message reveal (injected)"
    )
    .setup(QuillApp::demo_ready_reveal),
    // Scheduled messages; `QUILL_DEMO_SCHEDULED=button|picker|list|list-selected|reminder|reminder-list`
    // (composer button, date+time picker, list with Send now / Reschedule,
    // Saved Messages reminder wording; injected, no live Telegram).
    DemoSpec::chats(
        "ready-scheduled",
        "screenshot demo — scheduled messages (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_scheduled),
    // Sidebar search over injected recents / `searchChats` / `searchMessages`.
    DemoSpec::chats(
        "ready-search",
        "screenshot demo — sidebar search (injected searchChats / searchMessages)"
    )
    .setup(QuillApp::demo_ready_search),
    // Find in history: global search narrowed by the filter bar.
    DemoSpec::chats(
        "ready-search-filters",
        "screenshot demo — find in history (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_search_filters),
    // Search upgrades: the empty search with Frequent contacts + Recent.
    DemoSpec::chats(
        "ready-search-frequent",
        "screenshot demo — find in history (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_search_frequent),
    // Find in history: the in-chat "From:" member picker.
    DemoSpec::chats(
        "ready-search-from",
        "screenshot demo — find in history (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_find_in_history(
        FindInHistoryDemo::SearchFrom,
        window,
        cx
    )),
    // Find in history: a chosen member's messages with "N of M".
    DemoSpec::chats(
        "ready-search-from-hits",
        "screenshot demo — find in history (injected, no live Telegram)"
    )
    .setup(|app, window, cx| app.demo_find_in_history(
        FindInHistoryDemo::SearchFromHits,
        window,
        cx
    )),
    // In-chat search (`searchChatMessages`) + jump-to-message.
    DemoSpec::chats(
        "ready-search-in-chat",
        "screenshot demo — in-chat search (injected searchChatMessages)"
    )
    .setup(QuillApp::demo_ready_search_in_chat),
    // Search upgrades: a hashtag in the Public posts scope.
    DemoSpec::chats(
        "ready-search-public",
        "screenshot demo — find in history (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_ready_search_public),
    // Selection mode over a photo and a document that is not downloaded
    // yet: keyboard focus ring and the Download / Save buttons.
    DemoSpec::ready(
        "ready-select-keyboard",
        seed_ready_media_session,
        "screenshot demo — keyboard selection over media (injected)"
    )
    .setup(QuillApp::demo_ready_select_keyboard),
    // Message selection mode: check circles, selection tint and the
    // Forward N / Delete N / Cancel header (injected, no live Telegram).
    DemoSpec::chats(
        "ready-select-mode",
        "screenshot demo — message selection mode (injected)"
    )
    .setup(QuillApp::demo_ready_select_mode),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum FindInHistoryDemo {
    JumpDate,
    SearchFrom,
    SearchFromHits,
}

impl QuillApp {
    fn demo_ready_edit_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let edit = ComposerEdit::from_own_content(
            ChatId(11),
            MessageId(102),
            true,
            false,
            &quill::telegram::envelope::MessageContent::Text(
                "Reply from the session reducer.".into(),
            ),
        );
        self.composer.update(cx, |input, cx| {
            input.set_value("Reply from the session reducer.", window, cx);
            input.focus(window, cx);
        });
        self.pending_edit = edit;
        self.saved_edit_draft = "unrelated draft stays".into();
        self.pending_delete = DeleteConfirm::own(ChatId(11), MessageId(102), true, false);
    }

    fn demo_ready_edit_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self
            .demo_session
            .as_ref()
            .and_then(|session| session.histories.get(&11))
            .and_then(|history| history.messages.get(&302))
            .map(|message| message.content.clone());
        self.pending_edit = content.and_then(|content| {
            ComposerEdit::from_own_content(ChatId(11), MessageId(302), true, false, &content)
        });
        self.composer.update(cx, |input, cx| {
            input.set_value("Outgoing photo", window, cx);
            input.focus(window, cx);
        });
        // B5: "Replace attachment" staged with a new photo, caption
        // moved above the media.
        self.set_edit_replacement(&demo_media_allowlist().join("demo-thumb.png"), cx);
        if let Some(edit) = self.pending_edit.as_mut() {
            edit.caption_above = true;
        }
    }

    fn demo_ready_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_forward(session, &self.demo_sink, &self.demo_seq);
            self.forward_result = session.last_forward.clone();
        }
        let mut draft =
            ForwardDraft::from_message(ChatId(11), MessageId(101), false).expect("forward 101");
        draft.toggle(ChatId(11), MessageId(102), false);
        self.pending_forward = Some(draft);
        self.forward_picker_open = true;
        self.forward_search_input.update(cx, |input, cx| {
            input.set_value("Demo chat B", window, cx);
            input.focus(window, cx);
        });
        self.status_note = "screenshot demo — select → pick dest → forwarded".into();
    }

    fn demo_ready_mute_archive(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_mute_archive(session, &self.demo_sink, &self.demo_seq);
        }
        self.mute_menu_open = true;
        self.status_note = "screenshot demo — mute presets · muted icon · archive".into();
    }

    fn demo_ready_pin(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_pin(session, &self.demo_sink, &self.demo_seq);
            let _ = session.begin_chat_search_jump(MessageId(101));
        }
        self.status_note = "screenshot demo — pin · unpin · pinned bar".into();
    }

    fn demo_ready_reactions(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_reactions(session, &self.demo_sink, &self.demo_seq);
        }
        // The message menu with its reaction strip, expanded.
        if let Some(session) = self.demo_session.as_mut() {
            use quill::state::{MessageReactionOptions, ReactionChoice};
            let emoji = |e: &str| ReactionChoice::Emoji(e.to_string());
            session.message_reaction_options = Some(MessageReactionOptions {
                chat_id: ChatId(11),
                message_id: MessageId(101),
                top: ["❤", "👍", "🔥", "😂", "😮", "😢", "🎉"]
                    .into_iter()
                    .map(emoji)
                    .collect(),
                recent: vec![emoji("👏")],
                popular: [
                    "🤔", "🙏", "👌", "😍", "🤯", "😱", "🥰", "🤩", "💯", "⚡", "🏆", "🤝",
                ]
                .into_iter()
                .map(emoji)
                .collect(),
                allow_custom_emoji: false,
            });
        }
        self.message_menu = Some(MessageMenuState {
            chat_id: ChatId(11),
            message_id: MessageId(101),
            position: point(px(420.), px(200.)),
        });
        self.reactions_expanded = true;
        self.status_note = "screenshot demo — react · unreact · chips".into();
    }

    fn demo_ready_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("sounds good", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_reply(session, &self.demo_sink, &self.demo_seq);
            self.pending_reply = Some(ComposerReplyTo::new(
                ChatId(11),
                MessageId(101),
                "Hello from injected JSON.",
            ));
        }
    }

    fn demo_ready_reply_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.set_value("nice shot", window, cx);
            input.focus(window, cx);
        });
        self.pending_reply = Some(ComposerReplyTo::new(
            ChatId(11),
            MessageId(201),
            "Loaded photo",
        ));
    }

    fn demo_ready_reveal(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // The last message "just arrived": reveal it (freeze the frame
        // with `QUILL_MOTION_HOLD_MS`).
        // Read, so the list opens at the bottom (no unread anchor).
        if let Some(session) = self.demo_session.as_mut() {
            if let Some(chat) = session.chats.get_mut(&11) {
                chat.unread_count = 0;
            }
            if let Some(history) = session.histories.get_mut(&11) {
                history.unread_anchor = None;
            }
        }
        let rows = self
            .demo_session
            .as_ref()
            .and_then(|session| session.histories.get(&11))
            .map_or(0, |history| history.messages.len());
        self.motion
            .start_reveal(rows.saturating_sub(1), std::time::Instant::now());
    }

    fn demo_ready_scheduled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = crate::ui::scheduled_demo::ScheduledView::from_env();
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            crate::ui::scheduled_demo::apply_ready_scheduled(
                session,
                &self.demo_sink,
                &self.demo_seq,
                view,
            );
        }
        match view {
            crate::ui::scheduled_demo::ScheduledView::Picker
            | crate::ui::scheduled_demo::ScheduledView::Reminder => {
                self.open_schedule_picker(
                    crate::ui::scheduled::ScheduleTarget::Composer,
                    window,
                    cx,
                );
            }
            crate::ui::scheduled_demo::ScheduledView::List
            | crate::ui::scheduled_demo::ScheduledView::ReminderList => {
                self.scheduled_dialog_open = true;
            }
            crate::ui::scheduled_demo::ScheduledView::ListSelected => {
                self.scheduled_dialog_open = true;
                self.scheduled_selected =
                    vec![quill::ids::MessageId(501), quill::ids::MessageId(503)];
            }
            crate::ui::scheduled_demo::ScheduledView::Button => {}
        }
        self.status_note = "screenshot demo — scheduled messages".into();
    }

    fn demo_ready_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |input, cx| {
            input.set_value("hello", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search(session, &self.demo_sink, &self.demo_seq);
        }
    }

    fn demo_ready_search_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |input, cx| {
            input.set_value("hello", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search(session, &self.demo_sink, &self.demo_seq);
            apply_ready_search_filters(session);
        }
    }

    fn demo_ready_search_frequent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search_frequent(session, &self.demo_sink, &self.demo_seq);
        }
        self.search_input
            .update(cx, |input, cx| input.focus(window, cx));
    }

    fn demo_ready_search_in_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat_search_input.update(cx, |input, cx| {
            input.set_value("hello", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search_in_chat(session, &self.demo_sink, &self.demo_seq);
        }
    }

    fn demo_ready_search_public(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |input, cx| {
            input.set_value("#dune", window, cx);
            input.focus(window, cx);
        });
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_search_public(session, &self.demo_sink, &self.demo_seq);
        }
    }

    fn demo_ready_select_keyboard(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let mut draft =
            ForwardDraft::from_message(ChatId(11), MessageId(201), false).expect("select 201");
        draft.toggle(ChatId(11), MessageId(203), false);
        self.pending_forward = Some(draft);
        self.selection_anchor = Some(MessageId(201));
        self.selection_focus = Some(MessageId(201));
        self.status_note = "screenshot demo — keyboard selection".into();
    }

    fn demo_ready_select_mode(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let mut draft =
            ForwardDraft::from_message(ChatId(11), MessageId(101), false).expect("select 101");
        draft.toggle(ChatId(11), MessageId(102), false);
        self.pending_forward = Some(draft);
        self.status_note = "screenshot demo — selection mode".into();
    }

    fn demo_find_in_history(
        &mut self,
        demo: FindInHistoryDemo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_seq.store(session.last_seq, Ordering::SeqCst);
            match demo {
                FindInHistoryDemo::JumpDate => {
                    apply_ready_jump_date(session, &self.demo_sink, &self.demo_seq)
                }
                FindInHistoryDemo::SearchFrom => {
                    apply_ready_search_from(session, &self.demo_sink, &self.demo_seq)
                }
                _ => apply_ready_search_from_hits(session, &self.demo_sink, &self.demo_seq),
            }
            if !matches!(demo, FindInHistoryDemo::JumpDate) {
                self.chat_search_input
                    .update(cx, |input, cx| input.focus(window, cx));
            }
            self.status_note = "screenshot demo — find in history".into();
        }
    }
}

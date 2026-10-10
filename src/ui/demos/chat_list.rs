//! Screenshot demos: chat list.

use crate::ui::app::QuillApp;
use crate::ui::chat_list::apply_ready_chat_list_3;
use crate::ui::conversation::apply_ready_typing;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::state::SearchStatus;
use std::sync::atomic::Ordering;

register_demos![
    // Slice CL3: chat list with the @ mention badge, the ♥ reaction
    // badge, multi-select mode (two chats checked + the select bar),
    // and the row menu open showing Report / Block user (injected,
    // no live Telegram).
    DemoSpec::chats(
        "ready-chat-list-3",
        "screenshot demo — chat list: mentions · reactions · multi-select"
    )
    .setup(QuillApp::demo_ready_chat_list3),
    // Slice CL2: sidebar search with an empty result (injected, no
    // live Telegram).
    DemoSpec::chats(
        "ready-chat-list-search",
        "screenshot demo — chat list: search empty state"
    )
    .setup(QuillApp::demo_ready_chat_list_search),
    // Peer `chatActionTyping` in the open-chat header and sidebar row.
    DemoSpec::chats(
        "ready-typing",
        "screenshot demo — peer typing (injected updateChatAction)"
    )
    .setup(QuillApp::demo_ready_typing),
];

impl QuillApp {
    fn demo_ready_chat_list3(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL3: select mode (chats 11 + 12 checked, select bar),
        // the @ mention badge on chat 11, the ♥ reaction badge on chat
        // 12, and the row menu open on chat 11 showing Report / Block
        // user / Select. The menu sits below the rows so both badges
        // and the select bar stay visible (capture at
        // QUILL_DEMO_WINDOW_SIZE=1200x1250 on a 1400x1400 display).
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_chat_list_3(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.chat_list.selected.insert(11);
        self.chat_list.selected.insert(12);
        self.chat_list.menu = Some(ChatMenuState {
            chat_id: ChatId(11),
            position: Point::new(px(120.), px(770.)),
        });
        self.connection.status_note =
            "screenshot demo — mentions · reactions · multi-select · report · block".into();
    }

    fn demo_ready_chat_list_search(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL2: sidebar search showing the empty-result state.
        if let Some(session) = self.demo_session.as_mut() {
            session.open_search();
            session.search.begin_query("xyzzy-no-such-chat");
            session.search.status = SearchStatus::Empty;
        }
        self.connection.status_note = "screenshot demo — search empty state".into();
    }

    fn demo_ready_typing(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_typing(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection.status_note = "screenshot demo — typing…".into();
    }
}

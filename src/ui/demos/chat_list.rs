//! Screenshot demos: chat list.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::chat_list::apply_ready_chat_list_3;
use crate::ui::conversation::apply_ready_typing;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
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
        // `QUILL_DEMO_CHAT_MENU_SUBMENU=mute|folders` opens that submenu
        // of the row menu; the demo account has two folders for it.
        if let Some(session) = self.demo_session.as_mut() {
            for (id, name) in [(7, "Work"), (8, "Family")] {
                session
                    .chat_list
                    .chat_folders
                    .push(quill::telegram::envelope::ChatFolderInfo {
                        id,
                        name: name.to_string(),
                        icon_name: String::new(),
                        color_id: -1,
                        is_shareable: false,
                        has_my_invite_links: false,
                    });
            }
            if let Some(chat) = session.chats.get_mut(&11) {
                // `QUILL_DEMO_CHAT_MENU_KIND=group|channel` shows the
                // delete or leave entry of that chat kind.
                match std::env::var("QUILL_DEMO_CHAT_MENU_KIND").as_deref() {
                    Ok("group") => {
                        chat.kind =
                            quill::telegram::envelope::ChatKind::BasicGroup { basic_group_id: 11 };
                    }
                    Ok("channel") => {
                        chat.kind = quill::telegram::envelope::ChatKind::Supergroup {
                            supergroup_id: 11,
                            is_channel: true,
                        };
                    }
                    _ => {}
                }
                chat.can_be_deleted_only_for_self = true;
                chat.folder_positions.insert(7, 1);
            }
        }
        let mut menu = ChatMenuState::at(ChatId(11), Point::new(px(120.), px(300.)));
        menu.submenu = match std::env::var("QUILL_DEMO_CHAT_MENU_SUBMENU").as_deref() {
            Ok("mute") => Some(crate::ui::menu_states::ChatMenuSubmenu::Mute),
            Ok("folders") => Some(crate::ui::menu_states::ChatMenuSubmenu::Folders),
            _ => None,
        };
        self.chat_list.menu = Some(menu);
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — mentions · reactions · multi-select · report · block",
        ));
    }

    fn demo_ready_chat_list_search(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // Slice CL2: sidebar search showing the empty-result state.
        if let Some(session) = self.demo_session.as_mut() {
            session.open_search();
            session.search.search.begin_query("xyzzy-no-such-chat");
            session.search.search.status = SearchStatus::Empty;
        }
        self.connection
            .set_note(StatusNote::silent("screenshot demo — search empty state"));
    }

    fn demo_ready_typing(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(session) = self.demo_session.as_mut() {
            self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            apply_ready_typing(session, &self.demo_ui.sink, &self.demo_ui.seq);
        }
        self.connection
            .set_note(StatusNote::silent("screenshot demo — typing…"));
    }
}

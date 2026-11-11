//! Screenshot demo for the shared empty state: `ready-chat-list-empty`
//! shows the chat list with nothing in it. `QUILL_DEMO_CHAT_LIST_EMPTY`
//! picks which: `none` (default, no chats at all), `folder`, `unread`,
//! `archive` or `community` (still loading).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};

register_demos![
    DemoSpec::chat_list("ready-chat-list-empty").setup(|app, _, _| app.demo_chat_list_empty())
];

impl QuillApp {
    fn demo_chat_list_empty(&mut self) {
        let kind = std::env::var("QUILL_DEMO_CHAT_LIST_EMPTY").unwrap_or_default();
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.chat_list.main_paging = quill::state::ListPaging::Exhausted;
        match kind.as_str() {
            "folder" => {
                session.chat_list.chat_folders = vec![quill::telegram::envelope::ChatFolderInfo {
                    id: 9,
                    name: "Travel".into(),
                    icon_name: "Custom".into(),
                    color_id: -1,
                    is_shareable: false,
                    has_my_invite_links: false,
                }];
                self.chat_list.view = crate::ui::chat_list_state::ChatListView::Folder(9);
            }
            "unread" => {
                for chat in session.chats.values_mut() {
                    chat.unread_count = 0;
                    chat.is_marked_as_unread = false;
                }
                self.chat_list.view =
                    crate::ui::chat_list_state::ChatListView::Unread { folder: None };
            }
            "archive" => {
                session.archive_order.clear();
                self.chat_list.view = crate::ui::chat_list_state::ChatListView::Archived;
            }
            "community" => {
                self.chat_list.view = crate::ui::chat_list_state::ChatListView::Community(77);
            }
            _ => {
                session.main_order.clear();
                session.archive_order.clear();
            }
        }
    }
}

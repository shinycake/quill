//! Keeps the native window title in step with the open chat and the unread
//! total (`quill::window_title`). `Window::set_window_title` is backed on
//! macOS, Linux and Windows, so this needs no per-platform code.

use super::app::QuillApp;
use gpui_kit::*;
use quill::window_title::{TitleChat, window_title};

impl QuillApp {
    /// Call once per frame; the title is only handed to the platform when
    /// it changed.
    pub(super) fn sync_window_title(&self, window: &mut Window) {
        let title = self.compute_window_title();
        if *self.frame.window_title_shown.borrow() == title {
            return;
        }
        window.set_window_title(&title);
        // `QUILL_TRACE_TITLE=1`: print each title (what the platform shows).
        if std::env::var_os("QUILL_TRACE_TITLE").is_some() {
            eprintln!(
                "title: {:?} (window reports {:?})",
                title,
                window.window_title()
            );
        }
        *self.frame.window_title_shown.borrow_mut() = title;
    }

    fn compute_window_title(&self) -> String {
        // A locked app shows no chat name (tdesktop: `locked ? nullptr`).
        if self.account.passcode.locked {
            return window_title(None, 0, None);
        }
        let Some(session) = self.session() else {
            return window_title(None, 0, None);
        };
        let total = quill::tray::badge_count(session, &session.settings.badge_prefs);
        let open = session.open_chat.and_then(|id| session.chats.get(&id.0));
        let topic = open.and_then(|chat| session.open_topic_info(chat.id));
        let name = open.map(|chat| {
            if let Some(topic) = &topic {
                topic.name.clone()
            } else if session.is_saved_messages(chat.id) {
                "Saved Messages".to_string()
            } else {
                chat.title.clone()
            }
        });
        let unread = match (&topic, open) {
            (Some(topic), _) => topic.unread_count,
            (None, Some(chat)) => chat.unread_count,
            (None, None) => 0,
        };
        let chat = name.as_deref().map(|name| TitleChat {
            name,
            unread: unread.max(0) as u32,
        });
        window_title(chat, total, None)
    }
}

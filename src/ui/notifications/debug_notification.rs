//! Developer check for the OS notification path. With
//! `QUILL_DEBUG_NOTIFICATION` set, the first frame posts one notification for
//! a chat that does not exist (id 0): the buttons, the inline reply field and
//! the picture show up like a real one, and whatever is typed or pressed runs
//! the real handlers, where the send is refused (unknown chat, status note)
//! and the chat open finds nothing. Set it to an image path and that image
//! is the sender picture. Never set in normal use.

use super::*;
use std::sync::atomic::AtomicBool;

impl QuillApp {
    pub(super) fn post_debug_notification_once(&mut self, cx: &mut Context<Self>) {
        static CHECKED: AtomicBool = AtomicBool::new(false);
        if CHECKED.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(value) = std::env::var_os("QUILL_DEBUG_NOTIFICATION") else {
            return;
        };
        let picture = PathBuf::from(&value);
        let icon = picture
            .is_file()
            .then(|| super::avatar_icon::circular_icon_for(&picture))
            .flatten();
        let locked = self.account.passcode.locked;
        let notification = quill::notify::OsNotification {
            chat_id: ChatId(0),
            message_id: Some(quill::ids::MessageId(1)),
            title: "Quill notification check".to_string(),
            body: "Type a reply or press a button; nothing is sent.".to_string(),
        };
        eprintln!(
            "posting the QUILL_DEBUG_NOTIFICATION check (picture: {})",
            icon.is_some()
        );
        self.show_os_notification(notification, icon, locked, cx);
    }
}

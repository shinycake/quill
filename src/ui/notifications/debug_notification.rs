//! Developer check for the OS notification path. With
//! `QUILL_DEBUG_NOTIFICATION` set, the first frame posts one notification for
//! a chat that does not exist (id 0): the buttons, the inline reply field and
//! the picture show up like a real one, and whatever is typed or pressed runs
//! the real handlers, where the send is refused (unknown chat, status note)
//! and the chat open finds nothing. Set it to an image path and that image
//! is the sender picture; `QUILL_DEBUG_NOTIFICATION_PHOTO` names an image
//! shown as the message's photo, and `QUILL_DEBUG_NOTIFICATION_CUSTOM`
//! shows it in Quill's own notification window instead. Set that to a
//! number and the check is posted that many times, each replacing the
//! last, so each opens a new window (with `QUILL_TRACE_TOASTS=1`, the toast
//! timing run in `docs/build.md`). Never set in normal use.

use super::*;
use quill::lang::{keys, tr};
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
        let preview = std::env::var_os("QUILL_DEBUG_NOTIFICATION_PHOTO")
            .map(PathBuf::from)
            .filter(|path| path.is_file());
        let locked = self.account.passcode.locked;
        let notification = quill::notify::OsNotification {
            chat_id: ChatId(0),
            message_id: Some(quill::ids::MessageId(1)),
            title: tr(keys::Q_PREFS_QUILL_NOTIFICATION_CHECK).to_string(),
            body: tr(keys::Q_PREFS_TYPE_A_REPLY_OR_PRESS).to_string(),
        };
        eprintln!(
            "posting the QUILL_DEBUG_NOTIFICATION check (picture: {}, photo: {})",
            icon.is_some(),
            preview.is_some()
        );
        if let Some(custom) = std::env::var_os("QUILL_DEBUG_NOTIFICATION_CUSTOM") {
            let times = custom
                .to_str()
                .and_then(|times| times.parse::<usize>().ok())
                .unwrap_or(1);
            for _ in 0..times {
                self.show_custom_notification(
                    None,
                    notification.clone(),
                    icon.clone(),
                    preview.clone(),
                    locked,
                    true,
                    cx,
                );
            }
            return;
        }
        self.show_os_notification(notification, icon, preview, locked, cx);
    }
}

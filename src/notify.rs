//! Phase 8.1: OS desktop notifications for new incoming messages.
//!
//! The reducer (`Session::apply`, `src/state.rs`) decides *whether* to notify
//! from pure inputs via [`decide_notify`]; it queues [`QueuedNotification`]
//! records and the GPUI layer drains them and dispatches through the platform
//! backend built here.
//!
//! Backends:
//! - Linux: `notify-send` (libnotify), with `--wait --action=default=Open`
//!   so a click can focus the chat. Action support varies by notification
//!   daemon (GNOME/KDE honor it; others may ignore clicks) — the click path
//!   is best-effort, documented in DECISIONS.md.
//! - macOS: `osascript` `display notification` fallback. gpui-kit 0.6.1
//!   exposes no NotificationCenter binding, and inventing a native
//!   `UNUserNotificationCenter` binding here is out of scope, so macOS
//!   notifications are display-only (no click-to-focus) until a real native
//!   binding lands.
//!
//! Command arguments are passed to the process without a shell, so message
//! text can never inject shell syntax. The reducer never spawns processes;
//! dispatch runs on UI-thread-spawned worker threads.

use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::ParsedMessage;

/// Generic body used when previews are hidden (user setting or per-chat
/// `chatNotificationSettings`), or when the content has no preview text.
pub const GENERIC_BODY: &str = "New message";

/// One notification the UI should show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsNotification {
    pub chat_id: ChatId,
    pub title: String,
    pub body: String,
}

/// A queued notification with burst coalescing: consecutive `updateNewMessage`
/// decisions for the same chat merge into one entry whose display body becomes
/// "N new messages" once `count > 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedNotification {
    pub chat_id: ChatId,
    pub title: String,
    pub body: String,
    pub count: u32,
}

impl QueuedNotification {
    /// What to actually show: the original body for a single message, a
    /// summary for a burst.
    pub fn for_display(&self) -> OsNotification {
        let body = if self.count > 1 {
            format!("{} new messages", self.count)
        } else {
            self.body.clone()
        };
        OsNotification {
            chat_id: self.chat_id,
            title: self.title.clone(),
            body,
        }
    }
}

/// Merge `notification` into `queue`: an existing pending entry for the same
/// chat bumps its burst count; otherwise a fresh entry is appended.
pub fn coalesce_notification(queue: &mut Vec<QueuedNotification>, notification: OsNotification) {
    if let Some(existing) = queue.iter_mut().find(|q| q.chat_id == notification.chat_id) {
        existing.count += 1;
    } else {
        queue.push(QueuedNotification {
            chat_id: notification.chat_id,
            title: notification.title,
            body: notification.body,
            count: 1,
        });
    }
}

/// Pure inputs for the notify / don't-notify decision. Keeping the decision
/// input flat (rather than borrowing `Session` / `ChatSummary`) makes the
/// rules unit-testable without a reducer.
pub struct NotifyInput<'a> {
    /// The `updateNewMessage` payload.
    pub message: &'a ParsedMessage,
    /// Resolved chat title (`ChatSummary::title`); `None` when the chat is
    /// unknown to the reducer.
    pub chat_title: Option<&'a str>,
    /// `ChatSummary::is_muted()` — exception mute from `chatNotificationSettings`.
    pub chat_muted: bool,
    /// `ChatSummary::last_read_inbox_message_id` when the chat is known.
    pub last_read_inbox_message_id: Option<MessageId>,
    /// Currently open chat (`Session::open_chat`).
    pub open_chat: Option<ChatId>,
    /// Whether the OS considers our window focused (`Window::is_window_active`).
    pub app_active: bool,
    /// `settings::Preferences::hide_notification_previews` (default true).
    pub hide_previews: bool,
    /// Per-chat preview allowance from `chatNotificationSettings`:
    /// `use_default_show_preview || show_preview`.
    pub chat_preview_allowed: bool,
}

/// Decide whether an `updateNewMessage` deserves an OS notification.
///
/// Rules (see DECISIONS.md Phase 8.1):
/// 1. Outgoing messages never notify.
/// 2. Muted chats (`chatNotificationSettings` exception mute) never notify.
/// 3. Messages at or below `last_read_inbox_message_id` never notify
///    (already read — e.g. a server echo of a read message).
/// 4. The currently open chat does not notify *while the app is active*;
///    notify when the app is in the background or the chat isn't open.
/// 5. Unknown chats (no title, no verified mute/read state) do not notify.
/// 6. The body is the `MessageContent::preview()` unless previews are hidden
///    by the user setting or the per-chat setting (or the preview is empty),
///    in which case it is the generic "New message".
pub fn decide_notify(input: &NotifyInput) -> Option<OsNotification> {
    let message = input.message;
    if message.is_outgoing {
        return None;
    }
    if input.chat_muted {
        return None;
    }
    let chat_id = message.chat_id;
    let title = input.chat_title?;
    if let Some(last_read) = input.last_read_inbox_message_id
        && message.id.0 <= last_read.0
    {
        return None;
    }
    if input.app_active && input.open_chat == Some(chat_id) {
        return None;
    }
    let body = if input.hide_previews || !input.chat_preview_allowed {
        GENERIC_BODY.to_string()
    } else {
        let preview = message.content.preview();
        if preview.trim().is_empty() {
            GENERIC_BODY.to_string()
        } else {
            preview
        }
    };
    Some(OsNotification {
        chat_id,
        title: title.to_string(),
        body,
    })
}

/// Platform notification backend available on this build target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyBackend {
    /// Linux: `notify-send` (libnotify).
    NotifySend,
    /// macOS: `osascript` `display notification` fallback (display only).
    MacOsScript,
    /// No supported backend on this platform.
    Unsupported,
}

pub fn current_backend() -> NotifyBackend {
    if cfg!(target_os = "linux") {
        NotifyBackend::NotifySend
    } else if cfg!(target_os = "macos") {
        NotifyBackend::MacOsScript
    } else {
        NotifyBackend::Unsupported
    }
}

/// A shell-free OS command that shows one notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Whether a run of this command can report a user click.
    pub report_click: bool,
}

fn linux_notify_send_command(notification: &OsNotification) -> NotificationCommand {
    NotificationCommand {
        program: "notify-send".to_string(),
        // `--wait` blocks until the notification is dismissed or an action
        // fires; `--action=default=Open` prints "default" on stdout when the
        // user clicks, which the caller maps to focusing the chat.
        args: vec![
            "--app-name=Quill".to_string(),
            "--wait".to_string(),
            "--action=default=Open".to_string(),
            notification.title.clone(),
            notification.body.clone(),
        ],
        report_click: true,
    }
}

/// Escape a string for embedding in an AppleScript double-quoted literal.
fn applescript_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn macos_osascript_command(notification: &OsNotification) -> NotificationCommand {
    NotificationCommand {
        program: "osascript".to_string(),
        args: vec![
            "-e".to_string(),
            format!(
                "display notification \"{}\" with title \"{}\"",
                applescript_escape(&notification.body),
                applescript_escape(&notification.title),
            ),
        ],
        // `display notification` offers no click callback, so macOS
        // notifications are display-only in this slice.
        report_click: false,
    }
}

/// Build the OS command for one notification, or `None` when this platform
/// has no backend.
pub fn build_notification_command(notification: &OsNotification) -> Option<NotificationCommand> {
    match current_backend() {
        NotifyBackend::NotifySend => Some(linux_notify_send_command(notification)),
        NotifyBackend::MacOsScript => Some(macos_osascript_command(notification)),
        NotifyBackend::Unsupported => None,
    }
}

/// Result of running a [`NotificationCommand`] to completion.
pub struct NotificationOutcome {
    /// True when the user clicked the notification (Linux `--wait` action).
    pub clicked: bool,
}

/// Run the command to completion. Spawning the process itself failing (e.g.
/// no notification daemon) yields `clicked: false`; never panics.
pub fn run_notification_command(command: &NotificationCommand) -> NotificationOutcome {
    let output = std::process::Command::new(&command.program)
        .args(&command.args)
        .output();
    let clicked = match output {
        Ok(output) => {
            command.report_click
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.trim() == "default")
        }
        Err(_) => false,
    };
    NotificationOutcome { clicked }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{MessageContent, TextContent};

    fn test_message(chat_id: i64, id: i64, outgoing: bool, text: &str) -> ParsedMessage {
        ParsedMessage {
            id: MessageId(id),
            chat_id: ChatId(chat_id),
            is_outgoing: outgoing,
            is_pinned: false,
            media_album_id: 0,
            content: MessageContent::Text(TextContent::plain(text)),
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            reply_markup: None,
        }
    }

    fn input<'a>(
        message: &'a ParsedMessage,
        app_active: bool,
        open_chat: Option<ChatId>,
    ) -> NotifyInput<'a> {
        NotifyInput {
            message,
            chat_title: Some("Ada"),
            chat_muted: false,
            last_read_inbox_message_id: Some(MessageId(0)),
            open_chat,
            app_active,
            hide_previews: false,
            chat_preview_allowed: true,
        }
    }

    #[test]
    fn background_incoming_message_notifies_with_preview() {
        let message = test_message(7, 42, false, "hello there");
        let notification = decide_notify(&input(&message, false, None)).expect("notify");
        assert_eq!(notification.chat_id, ChatId(7));
        assert_eq!(notification.title, "Ada");
        assert_eq!(notification.body, "hello there");
    }

    #[test]
    fn foreground_other_chat_still_notifies() {
        // "or the chat isn't open": app active but a different chat is open.
        let message = test_message(7, 42, false, "hello");
        let notification = decide_notify(&input(&message, true, Some(ChatId(9)))).expect("notify");
        assert_eq!(notification.title, "Ada");
    }

    #[test]
    fn background_open_chat_still_notifies() {
        // The chat is "open" server-side but the user isn't looking at the app.
        let message = test_message(7, 42, false, "hello");
        let notification = decide_notify(&input(&message, false, Some(ChatId(7)))).expect("notify");
        assert_eq!(notification.title, "Ada");
    }

    #[test]
    fn foreground_open_chat_does_not_notify() {
        let message = test_message(7, 42, false, "hello");
        assert!(decide_notify(&input(&message, true, Some(ChatId(7)))).is_none());
    }

    #[test]
    fn outgoing_message_does_not_notify() {
        let message = test_message(7, 42, true, "hello");
        assert!(decide_notify(&input(&message, false, None)).is_none());
    }

    #[test]
    fn muted_chat_does_not_notify() {
        let message = test_message(7, 42, false, "hello");
        let mut ctx = input(&message, false, None);
        ctx.chat_muted = true;
        assert!(decide_notify(&ctx).is_none());
    }

    #[test]
    fn already_read_message_does_not_notify() {
        let message = test_message(7, 42, false, "hello");
        let mut ctx = input(&message, false, None);
        ctx.last_read_inbox_message_id = Some(MessageId(42));
        assert!(decide_notify(&ctx).is_none());
        // Strictly newer than the read marker still notifies.
        let newer = test_message(7, 43, false, "hello");
        let mut ctx = input(&newer, false, None);
        ctx.last_read_inbox_message_id = Some(MessageId(42));
        assert!(decide_notify(&ctx).is_some());
    }

    #[test]
    fn unknown_chat_does_not_notify() {
        let message = test_message(7, 42, false, "hello");
        let mut ctx = input(&message, false, None);
        ctx.chat_title = None;
        assert!(decide_notify(&ctx).is_none());
    }

    #[test]
    fn hidden_previews_show_generic_body() {
        let message = test_message(7, 42, false, "secret text");
        let mut ctx = input(&message, false, None);
        ctx.hide_previews = true;
        let notification = decide_notify(&ctx).expect("notify");
        assert_eq!(notification.title, "Ada");
        assert_eq!(notification.body, GENERIC_BODY);
    }

    #[test]
    fn per_chat_preview_disabled_shows_generic_body() {
        let message = test_message(7, 42, false, "secret text");
        let mut ctx = input(&message, false, None);
        ctx.chat_preview_allowed = false;
        let notification = decide_notify(&ctx).expect("notify");
        assert_eq!(notification.body, GENERIC_BODY);
    }

    #[test]
    fn empty_preview_falls_back_to_generic_body() {
        let message = test_message(7, 42, false, "   ");
        let notification = decide_notify(&input(&message, false, None)).expect("notify");
        assert_eq!(notification.body, GENERIC_BODY);
    }

    #[test]
    fn coalesce_bursts_into_summary() {
        let mut queue = Vec::new();
        let first = OsNotification {
            chat_id: ChatId(7),
            title: "Ada".into(),
            body: "one".into(),
        };
        coalesce_notification(&mut queue, first);
        coalesce_notification(
            &mut queue,
            OsNotification {
                chat_id: ChatId(7),
                title: "Ada".into(),
                body: "two".into(),
            },
        );
        coalesce_notification(
            &mut queue,
            OsNotification {
                chat_id: ChatId(8),
                title: "Noor".into(),
                body: "hi".into(),
            },
        );
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].count, 2);
        assert_eq!(queue[0].for_display().body, "2 new messages");
        assert_eq!(queue[0].for_display().title, "Ada");
        assert_eq!(queue[1].count, 1);
        assert_eq!(queue[1].for_display().body, "hi");
    }

    #[test]
    fn linux_command_shape() {
        let notification = OsNotification {
            chat_id: ChatId(7),
            title: "Ada".into(),
            body: "hello".into(),
        };
        let cmd = linux_notify_send_command(&notification);
        assert_eq!(cmd.program, "notify-send");
        assert_eq!(
            cmd.args,
            vec![
                "--app-name=Quill",
                "--wait",
                "--action=default=Open",
                "Ada",
                "hello",
            ]
        );
        assert!(cmd.report_click);
    }

    #[test]
    fn macos_command_escapes_quotes() {
        let notification = OsNotification {
            chat_id: ChatId(7),
            title: "Ada \"the\" dev".into(),
            body: "back\\slash".into(),
        };
        let cmd = macos_osascript_command(&notification);
        assert_eq!(cmd.program, "osascript");
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0], "-e");
        assert_eq!(
            cmd.args[1],
            "display notification \"back\\\\slash\" with title \"Ada \\\"the\\\" dev\""
        );
        assert!(!cmd.report_click);
    }

    #[test]
    fn failed_spawn_never_clicks() {
        let cmd = NotificationCommand {
            program: "quill-definitely-not-a-real-binary".to_string(),
            args: Vec::new(),
            report_click: true,
        };
        assert!(!run_notification_command(&cmd).clicked);
    }
}

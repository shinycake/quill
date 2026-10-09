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
//! - macOS / Windows: native GPUI system notifications (dispatched by the
//!   UI): `UNUserNotificationCenter` on macOS, WinRT toasts (AUMID
//!   registered by GPUI under HKCU) on Windows. A click on the body comes
//!   back through `App::on_system_notification_response` carrying the tag
//!   built by [`notification_tag`], which [`parse_notification_tag`] maps
//!   back to the chat.
//!
//! Command arguments are passed to the process without a shell, so message
//! text can never inject shell syntax. The reducer never spawns processes;
//! dispatch runs on UI-thread-spawned worker threads.

use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::{ParsedMessage, ReactionNotificationSource, effective_content};

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
    /// Sound decided at queue time (parity slice: notification sounds). The
    /// decision is made when the message arrives — the same moment the toast
    /// decision is made — so later mute/focus changes don't retroactively
    /// silence or unsilence it.
    pub sound: Option<NotificationSoundKind>,
}

impl QueuedNotification {
    /// What to actually show: the original body for a single message, a
    /// summary for a burst.
    /// What to show while the app is locked by a passcode: no sender, no
    /// text (tdesktop hides the message when the app is locked). The chat id
    /// stays so a click still lands on the chat after unlocking.
    pub fn for_locked_display(&self) -> OsNotification {
        OsNotification {
            chat_id: self.chat_id,
            title: "Quill".to_string(),
            body: if self.count > 1 {
                format!("{} new messages", self.count)
            } else {
                "You have a new message".to_string()
            },
        }
    }

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
/// chat bumps its burst count; otherwise a fresh entry is appended. The
/// sound from the first message of a burst wins — a burst plays once.
pub fn coalesce_notification(queue: &mut Vec<QueuedNotification>, notification: OsNotification) {
    coalesce_notification_with_sound(queue, notification, None);
}

/// Like [`coalesce_notification`], carrying the sound decision for the new
/// message. Burst entries keep the first message's sound (play once).
pub fn coalesce_notification_with_sound(
    queue: &mut Vec<QueuedNotification>,
    notification: OsNotification,
    sound: Option<NotificationSoundKind>,
) {
    if let Some(existing) = queue.iter_mut().find(|q| q.chat_id == notification.chat_id) {
        existing.count += 1;
    } else {
        queue.push(QueuedNotification {
            chat_id: notification.chat_id,
            title: notification.title,
            body: notification.body,
            count: 1,
            sound,
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
    /// Effective mute for this chat (`Session::effective_muted`): the chat's
    /// exception mute, plus the scope default's `mute_for` when the chat
    /// keeps `use_default_mute_for`.
    pub chat_muted: bool,
    /// `ChatSummary::last_read_inbox_message_id` when the chat is known.
    pub last_read_inbox_message_id: Option<MessageId>,
    /// Currently open chat (`Session::open_chat`).
    pub open_chat: Option<ChatId>,
    /// Whether the OS considers our window focused (`Window::is_window_active`).
    pub app_active: bool,
    /// `settings::Preferences::hide_notification_previews` (default true).
    pub hide_previews: bool,
    /// Effective preview allowance (`Session::effective_preview_allowed`):
    /// the chat's `show_preview`, or the scope default's when the chat keeps
    /// `use_default_show_preview`.
    pub chat_preview_allowed: bool,
}

/// Decide whether an `updateNewMessage` deserves an OS notification.
///
/// Rules (see DECISIONS.md Phase 8.1):
/// 1. Outgoing messages never notify.
/// 2. Effectively muted chats (exception mute, or scope default mute via
///    `use_default_mute_for`) never notify.
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
        let preview = effective_content(&message.content, message.ephemeral.as_ref()).preview();
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

/// Which sound to play for a notification (parity slice: notification sounds).
///
/// Resolved from `chatNotificationSettings` (`use_default_sound` / `sound_id`)
/// with the chat-scope default as fallback; `0` means disabled (schema
/// comment on `sound_id`, td_api.tl line 3350) and `-1` means the
/// app-dependent default sound (scope comment, line 3368).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSoundKind {
    /// App default tone (schema `-1`, or `use_default_sound` with no scope
    /// override, or a custom id missing from the saved-sound list — TDLib
    /// says "if a sound isn't in the list, then default sound needs to be
    /// used", `getSavedNotificationSounds` comment, line 13647).
    Default,
    /// A saved notification sound (`notificationSound.id`); its MP3 comes
    /// from `notificationSound.sound` (`file`, line 8857) via `downloadFile`.
    Custom(i64),
}

/// Pure inputs for the play / don't-play decision.
pub struct SoundInput {
    /// Whether the OS considers our window focused (`Window::is_window_active`).
    pub app_active: bool,
    /// Effective mute for this chat (`Session::effective_muted`) — the chat's
    /// exception mute, or the scope default's `mute_for` when the chat keeps
    /// `use_default_mute_for`. Same gate as the toast.
    pub chat_muted: bool,
    /// `chatNotificationSettings.use_default_sound`.
    pub use_default_sound: bool,
    /// `chatNotificationSettings.sound_id`.
    pub chat_sound_id: i64,
    /// The chat scope's `scopeNotificationSettings.sound_id`, when loaded.
    /// `None` (scope settings not yet fetched) falls back to the app default.
    pub scope_sound_id: Option<i64>,
}

/// Decide whether an incoming-message notification should play a sound.
///
/// Rules (parity slice, matching official behavior):
/// 1. No sound while the app window is focused — the user is looking at the
///    app already ("no sound when the chat is open/focused").
/// 2. Muted chats never make sound (same gate as the toast).
/// 3. A resolved sound id of `0` means disabled — no sound.
/// 4. `-1` (or an unknown scope) resolves to the app default tone.
/// 5. Any other id resolves to the saved notification sound; the playback
///    layer falls back to the default tone when the id is not in the saved
///    list (per the `getSavedNotificationSounds` comment).
pub fn decide_notification_sound(input: &SoundInput) -> Option<NotificationSoundKind> {
    if input.app_active || input.chat_muted {
        return None;
    }
    let resolved = if input.use_default_sound {
        input.scope_sound_id.unwrap_or(-1)
    } else {
        input.chat_sound_id
    };
    match resolved {
        0 => None,
        -1 => Some(NotificationSoundKind::Default),
        id => Some(NotificationSoundKind::Custom(id)),
    }
}

/// A button on an OS notification. tdesktop shows "Reply" and "Mark as
/// read" on the platforms that support notification actions
/// (`Window::Notifications::Manager`, `platform/*/notifications_manager_*`).
///
/// GPUI's `SystemNotification` offers action buttons on macOS, Windows and
/// Linux but no inline text field, so "Reply" brings the app forward with
/// the chat open and the composer focused (the documented fallback for
/// inline text input).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationAction {
    /// The notification body itself: open the chat.
    Open,
    /// Open the chat and focus the composer.
    Reply,
    /// Mark the chat as read without opening it.
    MarkRead,
}

impl NotificationAction {
    /// Buttons offered, in order (tdesktop `lng_notification_reply` then
    /// `lng_context_mark_read`).
    pub const BUTTONS: [NotificationAction; 2] =
        [NotificationAction::Reply, NotificationAction::MarkRead];

    /// Stable id carried through the platform response; also the
    /// `notify-send --action` name on Linux (`default` is the body click).
    pub fn id(self) -> &'static str {
        match self {
            NotificationAction::Open => "default",
            NotificationAction::Reply => "reply",
            NotificationAction::MarkRead => "read",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NotificationAction::Open => "Open",
            NotificationAction::Reply => "Reply",
            NotificationAction::MarkRead => "Mark as read",
        }
    }

    /// Inverse of [`NotificationAction::id`]; `None` (a body activation) and
    /// unknown ids open the chat.
    pub fn from_id(id: Option<&str>) -> NotificationAction {
        match id {
            Some("reply") => NotificationAction::Reply,
            Some("read") => NotificationAction::MarkRead,
            _ => NotificationAction::Open,
        }
    }
}

/// The `(id, label)` buttons to attach to a notification. A locked app
/// offers none: a reply or read receipt must not bypass the passcode.
pub fn action_buttons(locked: bool) -> Vec<(&'static str, &'static str)> {
    if locked {
        return Vec::new();
    }
    NotificationAction::BUTTONS
        .iter()
        .map(|action| (action.id(), action.label()))
        .collect()
}

/// Pure inputs for a reaction notification ("X reacted to your message").
pub struct ReactionNotifyInput<'a> {
    pub chat_id: ChatId,
    pub chat_title: &'a str,
    /// `reactionNotificationSettings.message_reaction_source`.
    pub source: ReactionNotificationSource,
    pub sender_name: Option<&'a str>,
    pub sender_is_contact: bool,
    /// The emoji, when the reaction is a plain emoji.
    pub emoji: Option<&'a str>,
    /// `reactionNotificationSettings.show_preview`.
    pub show_preview: bool,
    pub chat_muted: bool,
    pub app_active: bool,
    pub open_chat: Option<ChatId>,
}

/// tdesktop only notifies about reactions the user's settings allow
/// (`None` / `Contacts` / `All`), never in a muted chat, and not while the
/// chat is open in the focused window.
pub fn decide_reaction_notify(input: &ReactionNotifyInput) -> Option<OsNotification> {
    match input.source {
        ReactionNotificationSource::None => return None,
        ReactionNotificationSource::Contacts if !input.sender_is_contact => return None,
        ReactionNotificationSource::Contacts | ReactionNotificationSource::All => {}
    }
    if input.chat_muted || (input.app_active && input.open_chat == Some(input.chat_id)) {
        return None;
    }
    let body = match (input.show_preview, input.sender_name, input.emoji) {
        (true, Some(name), Some(emoji)) => format!("{name} reacted {emoji} to your message"),
        (true, Some(name), None) => format!("{name} reacted to your message"),
        (true, None, Some(emoji)) => format!("Reacted {emoji} to your message"),
        _ => "New reaction to your message".to_string(),
    };
    Some(OsNotification {
        chat_id: input.chat_id,
        title: input.chat_title.to_string(),
        body,
    })
}

/// Platform notification backend available on this build target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyBackend {
    /// Linux: `notify-send` (libnotify).
    NotifySend,
    /// macOS and Windows: GPUI `show_system_notification`, no process spawn.
    Native,
    /// No supported backend on this platform.
    Unsupported,
}

pub fn current_backend() -> NotifyBackend {
    if cfg!(target_os = "linux") {
        NotifyBackend::NotifySend
    } else if cfg!(any(target_os = "macos", target_os = "windows")) {
        NotifyBackend::Native
    } else {
        NotifyBackend::Unsupported
    }
}

/// Registry key (under HKCU) GPUI fills with the toast app's `DisplayName`
/// for the AppUserModelID `app_id`; the toast icon lives next to it.
#[cfg(any(windows, test))]
fn aumid_registry_key(app_id: &str) -> String {
    format!(r"Software\Classes\AppUserModelId\{app_id}")
}

/// Windows: give the toast identity `app_id` the Quill icon (`IconUri`).
/// GPUI registers only the display name, so unpackaged toasts would show a
/// blank icon. The PNG is written under `%LOCALAPPDATA%\Quill` (HKCU only,
/// no elevation); every failure is ignored, the toast still shows.
#[cfg(windows)]
pub fn register_toast_icon(app_id: &str) {
    const ICON: &[u8] = include_bytes!("../assets/icons/hicolor/128x128/apps/quill.png");
    let Some(dir) = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from) else {
        return;
    };
    let dir = dir.join("Quill");
    let path = dir.join("toast-icon.png");
    let current = std::fs::metadata(&path).map(|m| m.len()).ok();
    if current != Some(ICON.len() as u64)
        && (std::fs::create_dir_all(&dir).is_err() || std::fs::write(&path, ICON).is_err())
    {
        return;
    }
    let _ = crate::winreg::set_string(
        &aumid_registry_key(app_id),
        "IconUri",
        &path.to_string_lossy(),
    );
}

/// Stable GPUI notification tag for a chat: re-posting for the same chat
/// replaces the earlier toast, and the click response carries it back.
pub fn notification_tag(account: &str, chat_id: ChatId) -> String {
    format!("account:{account}:chat:{}", chat_id.0)
}

/// Inverse of [`notification_tag`]: `(account key, chat)`. The account key
/// keeps its `account:` prefix so callers compare it against
/// `format!("account:{}", ..)`.
pub fn parse_notification_tag(tag: &str) -> Option<(&str, ChatId)> {
    let (account, chat) = tag.rsplit_once(":chat:")?;
    Some((account, ChatId(chat.parse().ok()?)))
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
        // fires; each `--action=name=Label` prints its name on stdout when
        // the user picks it ("default" for a click on the body), which the
        // caller maps to a [`NotificationAction`].
        args: vec![
            "--app-name=Quill".to_string(),
            "--wait".to_string(),
            "--action=default=Open".to_string(),
            "--action=reply=Reply".to_string(),
            "--action=read=Mark as read".to_string(),
            // `--` ends option parsing so a title like `--action=x=Label`
            // is treated as the title, not another action button.
            "--".to_string(),
            notification.title.clone(),
            notification.body.clone(),
        ],
        report_click: true,
    }
}

pub fn build_notification_command(notification: &OsNotification) -> Option<NotificationCommand> {
    match current_backend() {
        NotifyBackend::NotifySend => Some(linux_notify_send_command(notification)),
        NotifyBackend::Native | NotifyBackend::Unsupported => None,
    }
}

/// Result of running a [`NotificationCommand`] to completion.
pub struct NotificationOutcome {
    /// True when the user clicked the notification (Linux `--wait` action).
    pub clicked: bool,
    /// Which action was picked, when `clicked`.
    pub action: Option<NotificationAction>,
}

/// Map `notify-send --wait` stdout (the picked action name) to an action.
fn parse_notify_send_output(stdout: &str) -> Option<NotificationAction> {
    stdout.lines().map(str::trim).find_map(|line| match line {
        "default" | "reply" | "read" => Some(NotificationAction::from_id(Some(line))),
        _ => None,
    })
}

/// Run the command to completion. Spawning the process itself failing (e.g.
/// no notification daemon) yields `clicked: false`; never panics.
pub fn run_notification_command(command: &NotificationCommand) -> NotificationOutcome {
    let output = std::process::Command::new(&command.program)
        .args(&command.args)
        .output();
    let action = match output {
        Ok(output) if command.report_click => {
            parse_notify_send_output(&String::from_utf8_lossy(&output.stdout))
        }
        _ => None,
    };
    NotificationOutcome {
        clicked: action.is_some(),
        action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telegram::envelope::{MessageContent, TextContent};

    fn test_message(chat_id: i64, id: i64, outgoing: bool, text: &str) -> ParsedMessage {
        ParsedMessage {
            sender: None,
            id: MessageId(id),
            chat_id: ChatId(chat_id),
            date: 0,
            is_outgoing: outgoing,
            is_pinned: false,
            media_album_id: 0,
            author_signature: None,
            ephemeral: None,
            topic_id: None,
            thread_id: None,
            content: MessageContent::Text(TextContent::plain(text)),
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            scheduling_state: None,
            can_retry: false,
            send_state: Default::default(),
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
    fn locked_notifications_reveal_no_sender_or_text() {
        let mut queue = Vec::new();
        coalesce_notification(
            &mut queue,
            OsNotification {
                chat_id: ChatId(7),
                title: "Ada".into(),
                body: "my secret".into(),
            },
        );
        let shown = queue[0].for_locked_display();
        assert_eq!(shown.chat_id, ChatId(7));
        assert!(!shown.title.contains("Ada") && !shown.body.contains("secret"));
        coalesce_notification(
            &mut queue,
            OsNotification {
                chat_id: ChatId(7),
                title: "Ada".into(),
                body: "more".into(),
            },
        );
        assert_eq!(queue[0].for_locked_display().body, "2 new messages");
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
                "--action=reply=Reply",
                "--action=read=Mark as read",
                "--",
                "Ada",
                "hello",
            ]
        );
        assert!(cmd.report_click);
    }

    #[test]
    fn notification_tag_roundtrips() {
        let tag = notification_tag("primary", ChatId(-1001234));
        assert_eq!(tag, "account:primary:chat:-1001234");
        assert_eq!(
            parse_notification_tag(&tag),
            Some(("account:primary", ChatId(-1001234)))
        );
        assert_eq!(parse_notification_tag("account:x:chat:nope"), None);
        assert_eq!(parse_notification_tag("no-chat-here"), None);
    }

    #[test]
    fn aumid_key_is_under_classes_appusermodelid() {
        assert_eq!(
            aumid_registry_key("org.shinycake.quill"),
            r"Software\Classes\AppUserModelId\org.shinycake.quill"
        );
    }

    #[test]
    fn native_backend_spawns_no_command() {
        let notification = OsNotification {
            chat_id: ChatId(7),
            title: "Ada".into(),
            body: "hello".into(),
        };
        let native = cfg!(any(target_os = "macos", target_os = "windows"));
        assert_eq!(current_backend() == NotifyBackend::Native, native);
        if native {
            assert!(build_notification_command(&notification).is_none());
        }
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

    fn sound_input(app_active: bool) -> SoundInput {
        SoundInput {
            app_active,
            chat_muted: false,
            use_default_sound: true,
            chat_sound_id: 0,
            scope_sound_id: None,
        }
    }

    #[test]
    fn focused_window_never_plays_sound() {
        assert!(decide_notification_sound(&sound_input(true)).is_none());
    }

    #[test]
    fn muted_chat_never_plays_sound() {
        let mut input = sound_input(false);
        input.chat_muted = true;
        assert!(decide_notification_sound(&input).is_none());
    }

    #[test]
    fn unknown_scope_plays_default_tone() {
        // `use_default_sound` with scope settings not yet fetched → app default.
        assert_eq!(
            decide_notification_sound(&sound_input(false)),
            Some(NotificationSoundKind::Default)
        );
    }

    #[test]
    fn disabled_chat_sound_is_silent() {
        // Explicit exception sound 0 = disabled (schema line 3350).
        let mut input = sound_input(false);
        input.use_default_sound = false;
        input.chat_sound_id = 0;
        assert!(decide_notification_sound(&input).is_none());
    }

    #[test]
    fn custom_chat_sound_resolves() {
        let mut input = sound_input(false);
        input.use_default_sound = false;
        input.chat_sound_id = 99;
        assert_eq!(
            decide_notification_sound(&input),
            Some(NotificationSoundKind::Custom(99))
        );
    }

    #[test]
    fn scope_sound_minus_one_is_default_tone() {
        // `-1` = app-dependent default (schema line 3368).
        let mut input = sound_input(false);
        input.scope_sound_id = Some(-1);
        assert_eq!(
            decide_notification_sound(&input),
            Some(NotificationSoundKind::Default)
        );
    }

    #[test]
    fn scope_sound_zero_is_silent() {
        let mut input = sound_input(false);
        input.scope_sound_id = Some(0);
        assert!(decide_notification_sound(&input).is_none());
    }

    #[test]
    fn scope_custom_sound_resolves() {
        let mut input = sound_input(false);
        input.scope_sound_id = Some(42);
        assert_eq!(
            decide_notification_sound(&input),
            Some(NotificationSoundKind::Custom(42))
        );
    }

    #[test]
    fn action_ids_roundtrip() {
        for action in NotificationAction::BUTTONS {
            assert_eq!(NotificationAction::from_id(Some(action.id())), action);
        }
        assert_eq!(NotificationAction::from_id(None), NotificationAction::Open);
        assert_eq!(
            NotificationAction::from_id(Some("default")),
            NotificationAction::Open
        );
        assert_eq!(
            NotificationAction::from_id(Some("garbage")),
            NotificationAction::Open
        );
        assert_eq!(NotificationAction::MarkRead.label(), "Mark as read");
    }

    #[test]
    fn locked_apps_offer_no_buttons() {
        assert!(action_buttons(true).is_empty());
        assert_eq!(
            action_buttons(false),
            vec![("reply", "Reply"), ("read", "Mark as read")]
        );
    }

    #[test]
    fn notify_send_output_picks_the_action() {
        assert_eq!(
            parse_notify_send_output("reply\n"),
            Some(NotificationAction::Reply)
        );
        assert_eq!(
            parse_notify_send_output("read"),
            Some(NotificationAction::MarkRead)
        );
        assert_eq!(
            parse_notify_send_output("default\n"),
            Some(NotificationAction::Open)
        );
        assert_eq!(parse_notify_send_output(""), None);
        assert_eq!(parse_notify_send_output("closed"), None);
    }

    fn reaction_input<'a>(source: ReactionNotificationSource) -> ReactionNotifyInput<'a> {
        ReactionNotifyInput {
            chat_id: ChatId(7),
            chat_title: "Ada",
            source,
            sender_name: Some("Grace"),
            sender_is_contact: true,
            emoji: Some("\u{2764}"),
            show_preview: true,
            chat_muted: false,
            app_active: false,
            open_chat: None,
        }
    }

    #[test]
    fn reaction_source_gates_the_notification() {
        assert!(
            decide_reaction_notify(&reaction_input(ReactionNotificationSource::None)).is_none()
        );
        let all = decide_reaction_notify(&reaction_input(ReactionNotificationSource::All)).unwrap();
        assert_eq!(all.title, "Ada");
        assert_eq!(all.body, "Grace reacted \u{2764} to your message");
        let mut stranger = reaction_input(ReactionNotificationSource::Contacts);
        stranger.sender_is_contact = false;
        assert!(decide_reaction_notify(&stranger).is_none());
        stranger.sender_is_contact = true;
        assert!(decide_reaction_notify(&stranger).is_some());
    }

    #[test]
    fn reaction_respects_mute_focus_and_preview() {
        let mut muted = reaction_input(ReactionNotificationSource::All);
        muted.chat_muted = true;
        assert!(decide_reaction_notify(&muted).is_none());
        let mut open = reaction_input(ReactionNotificationSource::All);
        open.app_active = true;
        open.open_chat = Some(ChatId(7));
        assert!(decide_reaction_notify(&open).is_none());
        let mut hidden = reaction_input(ReactionNotificationSource::All);
        hidden.show_preview = false;
        assert_eq!(
            decide_reaction_notify(&hidden).unwrap().body,
            "New reaction to your message"
        );
        let mut custom = reaction_input(ReactionNotificationSource::All);
        custom.emoji = None;
        assert_eq!(
            decide_reaction_notify(&custom).unwrap().body,
            "Grace reacted to your message"
        );
    }
}

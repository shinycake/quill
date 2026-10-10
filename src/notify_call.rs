//! System notification for an incoming call.
//!
//! tdesktop (`calls/calls_instance.cpp`, `Instance::handleCallUpdate`
//! -> `Notifications::Manager` call notification): when a call rings in and
//! Telegram is not in front, the system shows a notification with the
//! caller's name, and the call can be answered or declined from it.
//!
//! The decision and the wording are pure. Dispatch reuses the backends of
//! [`crate::notify`]: native GPUI notifications on macOS and Windows,
//! `notify-send --wait` with actions on Linux. The tag carries the call id
//! (not a chat id), so it never collides with a message notification.

use crate::notify::{NotificationCommand, NotifyBackend, current_backend};

/// What the user picked on a call notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallNotificationAction {
    /// The body itself: bring the call window forward.
    Open,
    Accept,
    Decline,
}

impl CallNotificationAction {
    /// Buttons offered, in order.
    pub const BUTTONS: [CallNotificationAction; 2] = [
        CallNotificationAction::Accept,
        CallNotificationAction::Decline,
    ];

    /// Stable id carried through the platform response; also the
    /// `notify-send --action` name on Linux (`default` is the body click).
    pub fn id(self) -> &'static str {
        match self {
            CallNotificationAction::Open => "default",
            CallNotificationAction::Accept => "accept",
            CallNotificationAction::Decline => "decline",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CallNotificationAction::Open => "Open",
            CallNotificationAction::Accept => "Accept",
            CallNotificationAction::Decline => "Decline",
        }
    }

    /// Inverse of [`CallNotificationAction::id`]; a body activation (no id)
    /// and unknown ids open the call window rather than answering by
    /// accident.
    pub fn from_id(id: Option<&str>) -> CallNotificationAction {
        match id {
            Some("accept") => CallNotificationAction::Accept,
            Some("decline") => CallNotificationAction::Decline,
            _ => CallNotificationAction::Open,
        }
    }
}

/// One call notification to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallNotification {
    pub call_id: i32,
    pub title: String,
    pub body: String,
    /// Accept and Decline buttons. Off while the app is locked: answering
    /// must not bypass the passcode.
    pub buttons: bool,
}

/// Pure inputs for [`decide_call_notify`].
pub struct CallNotifyInput<'a> {
    pub call_id: i32,
    pub caller: &'a str,
    pub is_video: bool,
    /// One of Quill's windows is the active one.
    pub app_active: bool,
    /// The call that was notified last, if any.
    pub last_notified: Option<i32>,
    /// The passcode lock is up.
    pub locked: bool,
}

/// Whether a ringing incoming call gets a system notification: only while
/// no Quill window is active (the call window already shows it otherwise),
/// and once per call. A locked app hides the caller's name.
pub fn decide_call_notify(input: &CallNotifyInput) -> Option<CallNotification> {
    if input.app_active || input.last_notified == Some(input.call_id) {
        return None;
    }
    let kind = if input.is_video {
        "Incoming video call"
    } else {
        "Incoming voice call"
    };
    let (title, body) = if input.locked {
        ("Quill".to_string(), kind.to_string())
    } else {
        (input.caller.to_string(), kind.to_string())
    };
    Some(CallNotification {
        call_id: input.call_id,
        title,
        body,
        buttons: !input.locked,
    })
}

/// The `(id, label)` buttons to attach.
pub fn call_buttons(notification: &CallNotification) -> Vec<(&'static str, &'static str)> {
    if !notification.buttons {
        return Vec::new();
    }
    CallNotificationAction::BUTTONS
        .iter()
        .map(|action| (action.id(), action.label()))
        .collect()
}

/// GPUI notification tag for a call: a new ring replaces the old toast, and
/// the response carries the id back.
pub fn call_notification_tag(account: &str, call_id: i32) -> String {
    format!("account:{account}:call:{call_id}")
}

/// Inverse of [`call_notification_tag`]: `(account key, call id)`. The
/// account key keeps its `account:` prefix.
pub fn parse_call_notification_tag(tag: &str) -> Option<(&str, i32)> {
    let (account, call) = tag.rsplit_once(":call:")?;
    Some((account, call.parse().ok()?))
}

/// The Linux command: `notify-send --wait` prints the picked action name.
pub fn linux_call_command(notification: &CallNotification) -> NotificationCommand {
    let mut args = vec![
        "--app-name=Quill".to_string(),
        "--category=call.incoming".to_string(),
        "--urgency=critical".to_string(),
        "--wait".to_string(),
        "--action=default=Open".to_string(),
    ];
    if notification.buttons {
        args.push("--action=accept=Accept".to_string());
        args.push("--action=decline=Decline".to_string());
    }
    // `--` ends option parsing so a caller named `--action=x=y` stays a title.
    args.push("--".to_string());
    args.push(notification.title.clone());
    args.push(notification.body.clone());
    NotificationCommand {
        program: "notify-send".to_string(),
        args,
        report_click: true,
    }
}

/// The command for this platform; `None` where GPUI shows the notification
/// natively (macOS, Windows) or nothing can.
pub fn build_call_command(notification: &CallNotification) -> Option<NotificationCommand> {
    match current_backend() {
        NotifyBackend::NotifySend => Some(linux_call_command(notification)),
        NotifyBackend::Native | NotifyBackend::Unsupported => None,
    }
}

/// Map `notify-send --wait` stdout (the picked action name) to an action.
pub fn parse_call_output(stdout: &str) -> Option<CallNotificationAction> {
    stdout.lines().map(str::trim).find_map(|line| match line {
        "default" | "accept" | "decline" => Some(CallNotificationAction::from_id(Some(line))),
        _ => None,
    })
}

/// Run the command to completion and report the picked action. A missing
/// notification daemon yields `None`; never panics.
pub fn run_call_command(command: &NotificationCommand) -> Option<CallNotificationAction> {
    let output = std::process::Command::new(&command.program)
        .args(&command.args)
        .output()
        .ok()?;
    parse_call_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::{
        CallNotificationAction, CallNotifyInput, call_buttons, call_notification_tag,
        decide_call_notify, linux_call_command, parse_call_notification_tag, parse_call_output,
    };

    fn input(app_active: bool, last: Option<i32>, locked: bool) -> CallNotifyInput<'static> {
        CallNotifyInput {
            call_id: 7,
            caller: "Ada Park",
            is_video: false,
            app_active,
            last_notified: last,
            locked,
        }
    }

    #[test]
    fn rings_the_notification_when_no_window_is_active() {
        let n = decide_call_notify(&input(false, None, false)).expect("notify");
        assert_eq!(n.title, "Ada Park");
        assert_eq!(n.body, "Incoming voice call");
        assert!(n.buttons);
    }

    #[test]
    fn video_calls_say_so() {
        let mut i = input(false, None, false);
        i.is_video = true;
        assert_eq!(
            decide_call_notify(&i).expect("notify").body,
            "Incoming video call"
        );
    }

    #[test]
    fn stays_quiet_when_a_window_is_active_or_already_notified() {
        assert_eq!(decide_call_notify(&input(true, None, false)), None);
        assert_eq!(decide_call_notify(&input(false, Some(7), false)), None);
        assert!(decide_call_notify(&input(false, Some(6), false)).is_some());
    }

    #[test]
    fn a_locked_app_hides_the_caller_and_the_buttons() {
        let n = decide_call_notify(&input(false, None, true)).expect("notify");
        assert_eq!(n.title, "Quill");
        assert!(!n.title.contains("Ada"));
        assert!(call_buttons(&n).is_empty());
        let command = linux_call_command(&n);
        assert!(!command.args.iter().any(|a| a.contains("accept")));
    }

    #[test]
    fn buttons_are_accept_then_decline() {
        let n = decide_call_notify(&input(false, None, false)).expect("notify");
        assert_eq!(
            call_buttons(&n),
            vec![("accept", "Accept"), ("decline", "Decline")]
        );
    }

    #[test]
    fn tags_round_trip_and_stay_apart_from_chat_tags() {
        let tag = call_notification_tag("main", 42);
        assert_eq!(
            parse_call_notification_tag(&tag),
            Some(("account:main", 42))
        );
        assert_eq!(crate::notify::parse_notification_tag(&tag), None);
        let chat = crate::notify::notification_tag("main", crate::ids::ChatId(5));
        assert_eq!(parse_call_notification_tag(&chat), None);
    }

    #[test]
    fn action_ids_map_back_and_a_plain_click_never_answers() {
        for action in CallNotificationAction::BUTTONS {
            assert_eq!(CallNotificationAction::from_id(Some(action.id())), action);
        }
        assert_eq!(
            CallNotificationAction::from_id(None),
            CallNotificationAction::Open
        );
        assert_eq!(
            CallNotificationAction::from_id(Some("nonsense")),
            CallNotificationAction::Open
        );
    }

    #[test]
    fn notify_send_output_maps_to_actions() {
        assert_eq!(
            parse_call_output("accept\n"),
            Some(CallNotificationAction::Accept)
        );
        assert_eq!(
            parse_call_output("\ndecline"),
            Some(CallNotificationAction::Decline)
        );
        assert_eq!(
            parse_call_output("default"),
            Some(CallNotificationAction::Open)
        );
        assert_eq!(parse_call_output(""), None);
        assert_eq!(parse_call_output("reply"), None);
    }

    #[test]
    fn the_linux_command_ends_options_before_the_caller_name() {
        let mut i = input(false, None, false);
        i.caller = "--action=accept=Hack";
        let n = decide_call_notify(&i).expect("notify");
        let command = linux_call_command(&n);
        let dashes = command.args.iter().position(|a| a == "--").expect("--");
        assert_eq!(command.args[dashes + 1], "--action=accept=Hack");
        assert!(command.report_click);
    }
}

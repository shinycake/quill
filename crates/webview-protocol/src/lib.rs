//! Wire types between Quill (the host) and `quill-webview`, the helper
//! process that owns the system web view a Telegram mini app runs in.
//!
//! The two talk over the helper's stdin/stdout, one JSON object per line
//! ([`encode`] / [`decode`]). The host sends [`HostCommand`]s; the helper
//! answers with [`HelperEvent`]s. Everything a mini app says reaches the
//! host as an opaque `(event, data)` pair in [`HelperEvent::WebApp`]: the
//! helper never interprets bot content, it only caps its size
//! ([`MAX_WEB_APP_EVENT_BYTES`]). The host validates and decides.
//!
//! The helper's window is a trusted HTML shell (header with title, back,
//! menu and close; the bottom buttons; popups) around a sandboxed
//! `<iframe>` holding the bot page. Shell-level clicks come back as
//! [`ShellEvent`]s; presentation changes go out as commands the shell
//! applies.

use serde::{Deserialize, Serialize};

/// Longest `data` payload of one mini-app event the helper forwards. The
/// Telegram bridge's biggest legitimate payloads (a `web_app_data_send`
/// of 4096 bytes, a popup) fit many times over.
pub const MAX_WEB_APP_EVENT_BYTES: usize = 64 * 1024;

/// Longest shell → helper message (`window.ipc.postMessage`) accepted at
/// all; longer ones are dropped before parsing.
pub const MAX_IPC_MESSAGE_BYTES: usize = MAX_WEB_APP_EVENT_BYTES + 1024;

/// The bridge's theme, as hex colors (`#rrggbb`), in the key names
/// `Telegram.WebApp.themeParams` uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub bg_color: String,
    pub secondary_bg_color: String,
    pub header_bg_color: String,
    pub bottom_bar_bg_color: String,
    pub section_bg_color: String,
    pub section_separator_color: String,
    pub text_color: String,
    pub accent_text_color: String,
    pub section_header_text_color: String,
    pub subtitle_text_color: String,
    pub destructive_text_color: String,
    pub hint_color: String,
    pub link_color: String,
    pub button_color: String,
    pub button_text_color: String,
    /// `light` or `dark`, the bridge's `colorScheme`.
    pub color_scheme: String,
}

/// One entry of the header's menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItem {
    /// Comes back in [`ShellEvent::Menu`].
    pub id: String,
    pub label: String,
    /// Drawn in the attention color (tdesktop's "Remove from menu").
    #[serde(default)]
    pub attention: bool,
}

/// The state of the main or secondary bottom button
/// (`web_app_setup_main_button`, `web_app_setup_secondary_button`), already
/// validated by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BottomButton {
    pub visible: bool,
    pub active: bool,
    pub text: String,
    /// `#rrggbb`; empty means the theme's button color.
    pub color: String,
    /// `#rrggbb`; empty means the theme's button text color.
    pub text_color: String,
    pub progress: bool,
    pub shine: bool,
    /// `left`, `right`, `top` or `bottom` relative to the main button
    /// (secondary button only).
    pub position: String,
}

/// One button of a shell popup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupButton {
    pub id: String,
    pub text: String,
    /// `default`, `ok`, `close`, `cancel` or `destructive`.
    pub kind: String,
}

/// A popup drawn by the shell over the frame (`web_app_open_popup`, the
/// write-access and clipboard consent prompts, the close confirmation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Popup {
    /// Comes back in [`ShellEvent::PopupClosed`].
    pub id: String,
    pub title: String,
    pub message: String,
    pub buttons: Vec<PopupButton>,
}

/// Host → helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostCommand {
    /// Point the frame at the mini app and set the chrome up.
    Load {
        url: String,
        title: String,
        theme: ThemeColors,
        menu: Vec<MenuItem>,
    },
    SetTitle {
        title: String,
    },
    SetTheme {
        theme: ThemeColors,
    },
    SetMenu {
        menu: Vec<MenuItem>,
    },
    MainButton {
        button: BottomButton,
    },
    SecondaryButton {
        button: BottomButton,
    },
    BackButton {
        visible: bool,
    },
    /// `#rrggbb`, or empty to go back to the theme's header color.
    HeaderColor {
        color: String,
    },
    /// `#rrggbb`, or empty to go back to the theme's background.
    BackgroundColor {
        color: String,
    },
    /// `#rrggbb`, or empty to go back to the theme's bottom bar color.
    BottomBarColor {
        color: String,
    },
    /// Whether closing the window first asks "Changes may not be saved".
    ClosingConfirmation {
        needed: bool,
    },
    ShowPopup {
        popup: Popup,
    },
    /// Deliver a bridge event to the mini app
    /// (`Telegram.WebView.receiveEvent`).
    Emit {
        event: String,
        data: serde_json::Value,
    },
    /// A short note at the bottom of the shell.
    Toast {
        text: String,
    },
    Reload,
    /// Close the window and exit.
    Close,
}

/// Clicks on the trusted shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShellEvent {
    /// The close button, or the window's close box. `force` is set once
    /// the "close anyway" confirmation was accepted.
    Close {
        force: bool,
    },
    Back,
    Menu {
        id: String,
    },
    Reload,
    PopupClosed {
        id: String,
        /// The pressed button's id; empty when dismissed.
        button: String,
    },
    /// The frame finished loading (`iframe.onload`).
    FrameLoaded,
}

/// Helper → host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HelperEvent {
    /// The window and web view exist; commands may follow.
    Ready,
    /// The mini app posted a bridge event. `data` is the raw JSON the app
    /// sent (an object, or an empty string), unparsed.
    WebApp {
        event: String,
        data: String,
    },
    Shell {
        event: ShellEvent,
    },
    /// The page (or something inside it) asked to open a URL outside the
    /// frame: `window.open`, a navigation the frame may not make, a
    /// download. The helper refused it; the host decides what to do.
    OpenExternal {
        url: String,
    },
    /// The window is gone (the helper exits right after).
    Closed,
    /// The helper could not do its job (no WebView2 runtime, no
    /// WebKitGTK); the message is for the log and the status bar.
    Error {
        message: String,
    },
}

/// One message as a line of JSON (serde escapes raw newlines, so the
/// output never spans lines).
pub fn encode<T: Serialize>(message: &T) -> String {
    let mut line = serde_json::to_string(message).unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    line
}

/// Parse one line.
pub fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(line.trim_end_matches(['\r', '\n']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip_as_single_lines() {
        let command = HostCommand::Emit {
            event: "theme_changed".into(),
            data: serde_json::json!({ "theme_params": { "bg_color": "#ffffff" } }),
        };
        let line = encode(&command);
        assert_eq!(line.matches('\n').count(), 1);
        assert!(line.ends_with('\n'));
        let back: HostCommand = decode(&line).unwrap();
        assert_eq!(back, command);
    }

    #[test]
    fn events_round_trip_with_raw_newlines_escaped() {
        let event = HelperEvent::WebApp {
            event: "web_app_data_send".into(),
            data: "{\"data\":\"line1\\nline2\"}".into(),
        };
        let line = encode(&event);
        assert_eq!(line.matches('\n').count(), 1);
        let back: HelperEvent = decode(&line).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn tagged_representation_is_stable() {
        let line = encode(&ShellEvent::Close { force: true });
        assert_eq!(line.trim(), r#"{"kind":"close","force":true}"#);
        let event: HelperEvent = decode(r#"{"kind":"shell","event":{"kind":"back"}}"#).unwrap();
        assert_eq!(
            event,
            HelperEvent::Shell {
                event: ShellEvent::Back
            }
        );
    }

    #[test]
    fn unknown_kinds_are_errors_not_panics() {
        assert!(decode::<HelperEvent>(r#"{"kind":"launch_missiles"}"#).is_err());
        assert!(decode::<HostCommand>("not json").is_err());
    }
}

//! The popups a mini app's helper shows: which one an answer is for, and
//! what each one was asking.

/// One popup shown to the helper. A stage that waits for the person holds
/// the id of the popup it opened and acts only on that popup's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PopupId(u64);

impl PopupId {
    /// The popup numbered `seq` by the window that shows them.
    pub(super) fn new(seq: u64) -> Self {
        Self(seq)
    }

    /// The id the helper knows the popup by.
    pub(super) fn wire(self) -> String {
        format!("p{}", self.0)
    }

    /// The popup a helper answer names; `None` for an id this host never
    /// made.
    pub(super) fn from_wire(wire: &str) -> Option<Self> {
        wire.strip_prefix('p')?.parse().ok().map(Self)
    }
}

/// What a shell popup was for, so its answer can be acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PopupPurpose {
    /// `web_app_open_popup`: the app gets `popup_closed`.
    App,
    /// "Allow messaging": yes calls `allowBotToSendMessages`.
    WriteAccess,
    /// "Paste from clipboard": yes reads the clipboard for the app.
    Clipboard { req_id: String },
    /// "Close anyway" after `web_app_setup_closing_behavior`.
    CloseConfirm,
    /// `web_app_request_emoji_status_access`: "Allow" grants it.
    EmojiStatusAccess,
    /// `web_app_set_emoji_status`: "Confirm" sets the status.
    EmojiStatus,
    /// `web_app_request_phone`: "Share" sends the contact card.
    Phone,
}

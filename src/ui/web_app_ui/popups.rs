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

/// What a shell popup was for, so its answer can be acted on. Only the
/// popups no stage waits on are here; a stage that opened its popup holds
/// the [`PopupId`] and says it [`owns`](PopupOwner) it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PopupPurpose {
    /// `web_app_open_popup`: the app gets `popup_closed`.
    App,
    /// "Paste from clipboard": yes reads the clipboard for the app.
    Clipboard { req_id: String },
    /// "Close anyway" after `web_app_setup_closing_behavior`.
    CloseConfirm,
}

/// The stage whose popup an answer is for, found by asking each stage
/// whether it holds the id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PopupOwner {
    /// `WriteAccess::Asking`: "Allow messaging".
    WriteAccess,
    /// The emoji status access prompt.
    EmojiAccess,
    /// The emoji status confirmation.
    EmojiStatus,
    /// The phone share prompt.
    Phone,
}

/// A popup just shown, and the ones it replaced: the helper shows one at a
/// time, so each of those never gets `popup_closed` and is answered as
/// dismissed by whoever showed the new one.
pub(super) struct Shown {
    pub(super) id: PopupId,
    pub(super) replaced: Vec<PopupId>,
}

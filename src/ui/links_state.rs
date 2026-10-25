//! Deep links and bot link confirmations.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::ids::ChatId;

pub(crate) struct LinkUi {
    /// `parity:platform-deep-links`: TDLib's info / error text for the
    /// deep link, shown in a dialog (`DialogKind::DeepLinkInfo`).
    pub(super) deep_link_dialog: Option<String>,
    pub(super) deep_link_invite: Option<quill::state::DeepLinkState>,
    /// A typed link that needs the `Window` (`deep_link_routes`).
    pub(super) pending_deep_link_ui: Option<quill::deep_link_types::DeepLinkUi>,
    /// `parity:platform-deep-links`: resolved chat + action waiting for
    /// render (which owns the `Window`) to open it.
    pub(super) pending_deep_link_open: Option<(ChatId, quill::state::DeepLinkAction)>,
    /// The deep-link flow in progress was started by a step the UI driver
    /// injected (`QuillApp::note_deep_link_origin`); taken when the flow
    /// ends or hands a chat to render.
    pub(super) deep_link_from_agent: bool,
    /// `pending_deep_link_open` came from an agent's step: render re-checks
    /// the open as part of that step.
    pub(super) pending_deep_link_open_from_agent: bool,
    /// B1: password prompt for `inlineKeyboardButtonTypeCallbackWithPassword`.
    pub(super) callback_password_dialog: Option<CallbackPasswordDialog>,
    /// B1: `loginUrlInfoRequestConfirmation` domain/url awaiting user consent.
    pub(super) login_url_confirm: Option<LoginUrlConfirm>,
}

impl LinkUi {
    pub(super) fn new() -> Self {
        Self {
            deep_link_dialog: None,
            deep_link_invite: None,
            pending_deep_link_ui: None,
            pending_deep_link_open: None,
            deep_link_from_agent: false,
            pending_deep_link_open_from_agent: false,
            callback_password_dialog: None,
            login_url_confirm: None,
        }
    }
}

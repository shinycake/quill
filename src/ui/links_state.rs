//! Deep links and bot link confirmations.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::ids::ChatId;

/// A chat a link resolved to, with what to do there, waiting for render
/// to open it. Who asked travels with it, so a later open cannot inherit
/// an earlier one's origin.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PendingOpen {
    pub(super) chat_id: ChatId,
    pub(super) action: quill::state::DeepLinkAction,
    /// An agent's step queued the link: render re-checks the open as part
    /// of that step.
    pub(super) from_agent: bool,
}

impl PendingOpen {
    /// An open the user asked for.
    pub(super) fn user(chat_id: ChatId, action: quill::state::DeepLinkAction) -> Self {
        Self {
            chat_id,
            action,
            from_agent: false,
        }
    }
}

pub(crate) struct LinkUi {
    /// `parity:platform-deep-links`: TDLib's info / error text for the
    /// deep link, shown in a dialog (`DialogKind::DeepLinkInfo`).
    pub(super) deep_link_dialog: Option<String>,
    pub(super) deep_link_invite: Option<quill::state::DeepLinkState>,
    /// A typed link that needs the `Window` (`deep_link_routes`).
    pub(super) pending_deep_link_ui: Option<quill::deep_link_types::DeepLinkUi>,
    /// `parity:platform-deep-links`: resolved chat + action waiting for
    /// render (which owns the `Window`) to open it.
    pub(super) pending_deep_link_open: Option<PendingOpen>,
    /// The link in `QuillApp::pending_deep_link` when a step the UI driver
    /// injected queued it (`QuillApp::queue_deep_link`).
    pub(super) agent_pending_link: Option<String>,
    /// B1: password prompt for `inlineKeyboardButtonTypeCallbackWithPassword`.
    pub(super) callback_password_dialog: Option<CallbackPasswordDialog>,
    /// B1: `loginUrlInfoRequestConfirmation` domain/url awaiting user consent.
    pub(super) login_url_confirm: Option<LoginUrlConfirm>,
}

impl crate::ui::on_screen::Farewell for LinkUi {}

impl LinkUi {
    pub(super) fn new() -> Self {
        Self {
            deep_link_dialog: None,
            deep_link_invite: None,
            pending_deep_link_ui: None,
            pending_deep_link_open: None,
            agent_pending_link: None,
            callback_password_dialog: None,
            login_url_confirm: None,
        }
    }
}

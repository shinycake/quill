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

/// A link waiting for the deep-link pump (`QuillApp::pending_deep_link`),
/// with who queued it. One value, so a later link replaces its origin too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueuedLink {
    pub(super) link: String,
    /// The account it was clicked in (`None`: from outside Quill, the
    /// launch argument or the OS, which whichever account is on screen
    /// takes). A link clicked in an account's chat names that account's
    /// chats and bots: another account never follows it.
    pub(super) account: Option<quill::ids::AccountKey>,
    /// The chat window it was clicked in, or whose request it answers
    /// (`None`: the main window).
    pub(super) window: Option<ChatId>,
    /// A step the UI driver injected queued it (`guard::injecting`): it
    /// is checked as the agent's, not followed as the user's.
    pub(super) by_agent: bool,
}

impl QueuedLink {
    /// A link from outside Quill: the launch argument, the OS, a second
    /// launch.
    pub(crate) fn outside(link: String) -> Self {
        Self {
            link,
            account: None,
            window: None,
            by_agent: false,
        }
    }

    /// Whether the account on screen (`None`: none yet) may follow it.
    pub(super) fn for_screen(&self, on_screen: Option<&quill::ids::AccountKey>) -> bool {
        match (&self.account, on_screen) {
            (None, _) => true,
            (Some(asked), Some(on_screen)) => asked == on_screen,
            (Some(_), None) => false,
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
    /// The chat window the link flow in progress was asked from. Taken
    /// with the flow's state at each step, as the agent origin is.
    pub(super) flow_window: Option<ChatId>,
    /// The chat window the resolved link waiting for render
    /// (`pending_deep_link_ui`, `pending_deep_link_open`) was clicked in:
    /// the boxes it opens belong to it.
    pub(super) pending_flow_window: Option<ChatId>,
    /// B1: password prompt for `inlineKeyboardButtonTypeCallbackWithPassword`.
    pub(super) callback_password_dialog: Option<CallbackPasswordDialog>,
    /// B1: `loginUrlInfoRequestConfirmation` domain/url awaiting user consent.
    pub(super) login_url_confirm: Option<LoginUrlConfirm>,
}

/// The language link box keeps its pack in the session
/// (`settings.language_link`), which goes to the background with the
/// account: without a close it would come back on switch-back.
impl crate::ui::on_screen::Farewell for LinkUi {
    fn farewell(
        app: &mut QuillApp,
        _: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        if app
            .session()
            .is_some_and(|session| session.settings.language_link.is_some())
        {
            app.close_language_box(cx);
        }
    }
}

impl LinkUi {
    pub(super) fn new() -> Self {
        Self {
            deep_link_dialog: None,
            deep_link_invite: None,
            pending_deep_link_ui: None,
            pending_deep_link_open: None,
            flow_window: None,
            pending_flow_window: None,
            callback_password_dialog: None,
            login_url_confirm: None,
        }
    }
}

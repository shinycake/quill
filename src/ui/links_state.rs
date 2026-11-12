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

/// What a resolved link waits for the frame to do.
pub(super) enum ResolvedStep {
    /// `parity:platform-deep-links`: resolved chat + action waiting for
    /// render (which owns the `Window`) to open it.
    Open(PendingOpen),
    /// A typed link that needs the `Window` (`deep_link_routes`).
    Ui(quill::deep_link_types::DeepLinkUi),
}

/// A resolved link waiting for render, with the chat window it was clicked
/// in (`None`: the main window): the boxes it opens belong to it. One value,
/// so a link never waits with another's window, and a newer one replaces it
/// whole.
pub(super) struct ResolvedLink {
    pub(super) window: Option<ChatId>,
    pub(super) step: ResolvedStep,
}

#[cfg(test)]
impl ResolvedLink {
    pub(super) fn open(open: PendingOpen, window: Option<ChatId>) -> Self {
        Self {
            window,
            step: ResolvedStep::Open(open),
        }
    }

    pub(super) fn ui(ui: quill::deep_link_types::DeepLinkUi, window: Option<ChatId>) -> Self {
        Self {
            window,
            step: ResolvedStep::Ui(ui),
        }
    }
}

/// The invite box of the deep-link flow: closed, or open for the invite of
/// the flow generation it shows. The invite itself is the session flow's
/// stage (`DeepLinkState::InvitePreview`), which stays until the user
/// decides; this only says that its box is up, and for which flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum InviteBox {
    #[default]
    Closed,
    Open {
        generation: u64,
    },
}

impl InviteBox {
    pub(super) fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }

    /// The generation of the invite the box is up for, if it is.
    pub(super) fn generation(&self) -> Option<u64> {
        match self {
            Self::Open { generation } => Some(*generation),
            Self::Closed => None,
        }
    }

    /// The box is up for the invite of `generation`.
    pub(super) fn show(&mut self, generation: u64) {
        *self = Self::Open { generation };
    }

    /// The invite is answered or dropped: the generation it was up for.
    pub(super) fn take(&mut self) -> Option<u64> {
        let generation = self.generation();
        *self = Self::Closed;
        generation
    }
}

pub(crate) struct LinkUi {
    /// The invite preview box of the deep-link flow (the flow itself, with
    /// its origin window, is the session's).
    pub(super) invite_box: InviteBox,
    /// A resolved link waiting for the frame of its window to run it.
    pub(super) resolved: Option<ResolvedLink>,
    /// B1: password prompt for `inlineKeyboardButtonTypeCallbackWithPassword`.
    pub(super) callback_password_dialog: Option<CallbackPasswordDialog>,
    /// B1: `loginUrlInfoRequestConfirmation` domain/url awaiting user consent.
    pub(super) login_url_confirm: Option<LoginUrlConfirm>,
    /// The text of a bot's `show_alert` callback answer, while its box is up.
    pub(super) bot_alert: Option<String>,
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
        // The running flow lives in the session, which stays alive in the
        // background: without this a Join box left unanswered, or a chat
        // still resolving, popped up on the switch back (`pump_deep_link`).
        // Kit `close_all_dialogs` runs no `on_close`, so the invite's
        // cancel never runs on a switch.
        if let Some(session) = app.session_mut() {
            session.chats_state.end_deep_link_flow();
        }
    }
}

impl LinkUi {
    /// The chat a resolved link waits to open, if it is that kind.
    #[cfg(test)]
    pub(super) fn waiting_open(&self) -> Option<&PendingOpen> {
        match self.resolved.as_ref().map(|link| &link.step) {
            Some(ResolvedStep::Open(open)) => Some(open),
            _ => None,
        }
    }

    pub(super) fn new() -> Self {
        Self {
            invite_box: InviteBox::Closed,
            resolved: None,
            callback_password_dialog: None,
            login_url_confirm: None,
            bot_alert: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn the_invite_box_answers_once_for_its_generation() {
        let mut invite = InviteBox::default();
        assert!(!invite.is_open());
        invite.show(4);
        assert_eq!(invite.generation(), Some(4));
        assert_eq!(invite.take(), Some(4));
        assert_eq!(invite.take(), None, "answered once");
        assert!(!invite.is_open());
    }
}

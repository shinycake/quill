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

/// Who started the flow in progress: the chat window it was asked from
/// (`None`: the main window). Whether a step the agent UI driver injected
/// asked is the session flow's own (`DeepLinkFlow::from_agent`). It lives
/// and dies with the running flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct FlowOrigin {
    pub(super) window: Option<ChatId>,
}

/// Where the deep-link flow in progress stands in this window's UI, with
/// its origin.
#[derive(Default)]
pub(super) enum LinkFlow {
    #[default]
    Idle,
    /// The session resolves the link. The origin is taken with the flow's
    /// state at each pump and put back only while the flow goes on, so every
    /// terminal state leaves none behind.
    Running { origin: FlowOrigin },
    /// An invite preview waits for the user's answer (`DeepLinkInvite`).
    Asking { invite: quill::state::DeepLinkState },
}

impl LinkFlow {
    /// A flow asked from `from` starts; an unanswered invite gives way to it.
    pub(super) fn start(&mut self, origin: FlowOrigin) {
        *self = Self::Running { origin };
    }

    /// The origin of the running flow, which is no longer running: a step
    /// that goes on puts it back with [`Self::resume`].
    pub(super) fn take_origin(&mut self) -> Option<FlowOrigin> {
        match self {
            Self::Running { origin } => {
                let origin = *origin;
                *self = Self::Idle;
                Some(origin)
            }
            Self::Idle | Self::Asking { .. } => None,
        }
    }

    /// The flow goes on with its origin; an invite being asked stays.
    pub(super) fn resume(&mut self, origin: Option<FlowOrigin>) {
        if !matches!(self, Self::Asking { .. }) {
            *self = Self::Running {
                origin: origin.unwrap_or_default(),
            };
        }
    }

    /// The invite preview the user is asked about.
    pub(super) fn invite(&self) -> Option<&quill::state::DeepLinkState> {
        match self {
            Self::Asking { invite } => Some(invite),
            Self::Idle | Self::Running { .. } => None,
        }
    }

    pub(super) fn ask(&mut self, invite: quill::state::DeepLinkState) {
        *self = Self::Asking { invite };
    }

    /// The invite is answered or dropped.
    pub(super) fn take_invite(&mut self) -> Option<quill::state::DeepLinkState> {
        match std::mem::take(self) {
            Self::Asking { invite } => Some(invite),
            other => {
                *self = other;
                None
            }
        }
    }
}

pub(crate) struct LinkUi {
    /// `parity:platform-deep-links`: TDLib's info / error text for the
    /// deep link, shown in a dialog (`DialogKind::DeepLinkInfo`).
    pub(super) deep_link_dialog: Option<String>,
    /// The deep-link flow in progress: its origin window, or the invite
    /// preview it asks about.
    pub(super) flow: LinkFlow,
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
            deep_link_dialog: None,
            flow: LinkFlow::Idle,
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
    use quill::state::DeepLinkState;

    fn invite() -> DeepLinkState {
        DeepLinkState::ShowText("preview".into())
    }

    #[core::prelude::v1::test]
    fn the_origin_is_taken_with_the_step_and_put_back_while_the_flow_goes_on() {
        let mut flow = LinkFlow::default();
        let origin = FlowOrigin {
            window: Some(ChatId(7)),
        };
        flow.start(origin);
        assert_eq!(flow.take_origin(), Some(origin));
        assert_eq!(flow.take_origin(), None, "a terminal step leaves none");
        flow.resume(Some(origin));
        assert_eq!(
            flow.take_origin(),
            Some(origin),
            "a window's flow stays its"
        );
    }

    #[core::prelude::v1::test]
    fn an_invite_being_asked_survives_the_pump_and_gives_way_to_a_new_flow() {
        let mut flow = LinkFlow::default();
        flow.ask(invite());
        assert_eq!(flow.take_origin(), None);
        flow.resume(None);
        assert!(flow.invite().is_some(), "the pump does not drop the invite");
        flow.start(FlowOrigin::default());
        assert!(flow.invite().is_none(), "a new flow replaces it");
        flow.ask(invite());
        assert!(flow.take_invite().is_some());
        assert!(flow.take_invite().is_none());
    }
}

//! Screenshot demo registry.
//!
//! Every `--screenshot-demo <kind>` fixture is a [`DemoSpec`] that registers
//! itself next to its setup code with [`register_demos!`] (a link-time
//! `inventory` collection), so adding a demo touches no shared file. See
//! docs/decisions/codex-refactor-1.md.

use super::app::QuillApp;
use super::connect_ui::ConnectUiStatus;
use super::demo::seed_ready_chats_session;
use gpui_kit::{Context, Window};
use quill::composer::ComposerAttachment;
use quill::diagnostics::MemorySink;
use quill::state::Session;
use quill::telegram::envelope::AuthorizationState;
use std::sync::Arc;

/// Builds the injected session a signed-in demo starts from.
pub(crate) type DemoSeed = fn(Arc<MemorySink>) -> Session;
/// Runs once after `QuillApp::new_with_demo` has built the app.
pub(crate) type DemoSetup = fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>);
/// Composer attachments the demo starts with.
pub(crate) type DemoAttachments = fn() -> Vec<ComposerAttachment>;
/// Signed-out demos: connect status, status note and auth state.
pub(crate) type DemoSignedOut = fn() -> (ConnectUiStatus, String, AuthorizationState);

/// How a demo app starts.
pub(crate) enum DemoStart {
    /// Signed in (`DemoReadyChats`, `Ready`) with an injected session.
    Ready { seed: DemoSeed, note: &'static str },
    /// A sign-in or connection screen: no session.
    SignedOut(DemoSignedOut),
}

/// One screenshot demo. Build it with [`DemoSpec::chats`],
/// [`DemoSpec::ready`] or [`DemoSpec::signed_out`] plus the builder methods,
/// and register it with [`register_demos!`].
pub(crate) struct DemoSpec {
    /// The `--screenshot-demo` argument; also names the `.quill-ready-<kind>`
    /// marker file.
    pub(super) kind: &'static str,
    pub(super) start: DemoStart,
    pub(super) setup: Option<DemoSetup>,
    pub(super) attachments: Option<DemoAttachments>,
    /// Show the sign-in fields without a live client.
    pub(super) auth_inputs: bool,
    pub(super) window_title: &'static str,
    /// Install the tray icon for the demo window.
    pub(super) tray: bool,
    /// Wait before the capture / ready marker.
    pub(super) ready_after_ms: u64,
    /// Default time the window stays open (`QUILL_DEMO_LINGER_MS` overrides).
    pub(super) linger_ms: u64,
}

impl DemoSpec {
    const fn new(kind: &'static str, start: DemoStart) -> Self {
        Self {
            kind,
            start,
            setup: None,
            attachments: None,
            auth_inputs: false,
            window_title: "Quill",
            tray: false,
            ready_after_ms: 1500,
            linger_ms: 3500,
        }
    }

    /// Signed in over the standard chat-list fixture with the default
    /// status note.
    pub(super) const fn chat_list(kind: &'static str) -> Self {
        Self::chats(
            kind,
            "screenshot demo — Ready chat list (injected updates, no live Telegram)",
        )
    }

    /// Signed in over the standard chat-list fixture.
    pub(super) const fn chats(kind: &'static str, note: &'static str) -> Self {
        Self::ready(kind, seed_ready_chats_session, note)
    }

    /// Signed in over the session `seed` builds.
    pub(super) const fn ready(kind: &'static str, seed: DemoSeed, note: &'static str) -> Self {
        Self::new(kind, DemoStart::Ready { seed, note })
    }

    /// A signed-out screen with the sign-in fields shown.
    pub(super) const fn signed_out(kind: &'static str, start: DemoSignedOut) -> Self {
        Self::new(kind, DemoStart::SignedOut(start)).auth_inputs(true)
    }

    pub(super) const fn setup(self, setup: DemoSetup) -> Self {
        Self {
            setup: Some(setup),
            ..self
        }
    }

    pub(super) const fn attachments(self, attachments: DemoAttachments) -> Self {
        Self {
            attachments: Some(attachments),
            ..self
        }
    }

    pub(super) const fn auth_inputs(self, auth_inputs: bool) -> Self {
        Self {
            auth_inputs,
            ..self
        }
    }

    pub(super) const fn window_title(self, window_title: &'static str) -> Self {
        Self {
            window_title,
            ..self
        }
    }

    pub(super) const fn tray(self) -> Self {
        Self { tray: true, ..self }
    }

    pub(super) const fn timing(self, ready_after_ms: u64, linger_ms: u64) -> Self {
        Self {
            ready_after_ms,
            linger_ms,
            ..self
        }
    }
}

inventory::collect!(DemoSpec);

/// Registers screenshot demos: `register_demos![DemoSpec::chats(..), ..];`.
macro_rules! register_demos {
    ($($spec:expr),* $(,)?) => {
        $(::inventory::submit! { $spec })*
    };
}
pub(crate) use register_demos;

/// A registered screenshot demo (forced UI surface for screenshot proof: no
/// live Telegram, no real credentials).
#[derive(Clone, Copy)]
pub struct ScreenshotDemo(&'static DemoSpec);

impl ScreenshotDemo {
    /// The demo registered as `kind`.
    pub(crate) fn from_kind(kind: &str) -> Option<Self> {
        inventory::iter::<DemoSpec>
            .into_iter()
            .find(|spec| spec.kind == kind)
            .map(Self)
    }

    /// The demo registered as `kind`; panics when there is none (tests).
    #[cfg(test)]
    pub(crate) fn named(kind: &str) -> Self {
        Self::from_kind(kind).unwrap_or_else(|| panic!("no screenshot demo '{kind}'"))
    }

    /// Every registered demo, sorted by kind.
    pub(crate) fn all() -> Vec<Self> {
        let mut all: Vec<Self> = inventory::iter::<DemoSpec>.into_iter().map(Self).collect();
        all.sort_unstable_by_key(|demo| demo.kind());
        all
    }

    pub(crate) fn kind(self) -> &'static str {
        self.0.kind
    }

    pub(crate) fn window_title(self) -> &'static str {
        self.0.window_title
    }

    pub(crate) fn installs_tray(self) -> bool {
        self.0.tray
    }

    pub(crate) fn ready_after_ms(self) -> u64 {
        self.0.ready_after_ms
    }

    pub(crate) fn linger_ms(self) -> u64 {
        self.0.linger_ms
    }

    pub(super) fn auth_inputs(self) -> bool {
        self.0.auth_inputs
    }

    /// Session seed, connect status, status note and auth state.
    pub(super) fn start(
        self,
    ) -> (
        Option<DemoSeed>,
        ConnectUiStatus,
        String,
        AuthorizationState,
    ) {
        match self.0.start {
            DemoStart::Ready { seed, note } => (
                Some(seed),
                ConnectUiStatus::DemoReadyChats,
                note.into(),
                AuthorizationState::Ready,
            ),
            DemoStart::SignedOut(start) => {
                let (status, note, auth) = start();
                (None, status, note, auth)
            }
        }
    }

    pub(super) fn attachments(self) -> Vec<ComposerAttachment> {
        self.0.attachments.map_or_else(Vec::new, |make| make())
    }

    pub(super) fn setup(self, app: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>) {
        if let Some(setup) = self.0.setup {
            setup(app, window, cx);
        }
    }
}

impl PartialEq for ScreenshotDemo {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind()
    }
}

impl Eq for ScreenshotDemo {}

impl std::fmt::Debug for ScreenshotDemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.kind())
    }
}

#[cfg(test)]
mod tests {
    use super::ScreenshotDemo;

    #[test]
    fn kinds_are_unique() {
        let all = ScreenshotDemo::all();
        let mut kinds: Vec<&str> = all.iter().map(|demo| demo.kind()).collect();
        kinds.dedup();
        assert_eq!(kinds.len(), all.len(), "a demo kind is registered twice");
    }

    #[test]
    fn kinds_are_cli_names() {
        for demo in ScreenshotDemo::all() {
            let kind = demo.kind();
            assert!(
                !kind.is_empty()
                    && kind
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{kind}"
            );
        }
    }
}

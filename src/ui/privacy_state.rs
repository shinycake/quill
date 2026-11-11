//! Privacy and security screens: rules, blocked users, sessions and websites.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::*;

pub(crate) struct PrivacyState {
    /// B13: transient state of the privacy / security extras.
    pub(super) extra: super::privacy_extra::PrivacyUi,
    /// Slice S3: per-rule editor overlay target (Privacy screen).
    pub(super) editor: Option<PrivacyEditorTarget>,
    /// Slice S3: exception list overlay — the rule and always/never kind.
    pub(super) exceptions: Option<(PrivacyEditorTarget, PrivacyExceptionKind)>,
    /// Slice S3: add-exception contact picker inside the exceptions overlay.
    pub(super) exception_picker_open: bool,
    /// Slice S3: block-user contact picker inside the Privacy overlay.
    pub(super) block_picker_open: bool,
    /// The blocked-users rows, which page near their end (`scroll_load`).
    pub(super) blocked_scroll: ScrollHandle,
    pub(super) blocked_asked: super::scroll_load::Asked,
    /// Slice S3: two-step Unblock confirm on the Privacy screen.
    pub(super) unblock_confirm: Option<i64>,
    /// Linking another device by its login QR (`device_qr`).
    pub(super) device_link: super::device_qr::DeviceLink,
    /// Slice A3: pending terminate confirmation on the sessions overlay
    /// (TGX `TerminateSessionQuestion` / `TerminateIncompleteSessionQuestion`
    /// / `AreYouSureSessions`).
    pub(super) sessions_confirm: Option<SessionsConfirm>,
    /// Slice A4: Connected Websites overlay (TGX `SettingsWebsitesController`
    /// / `WebSessionsTitle` "Logged In with Telegram").
    pub(super) websites_open: bool,
    /// Slice A4: pending disconnect confirmation on the websites overlay
    /// (TGX `TerminateWebSessionQuestion` / `DisconnectAllWebsitesHint`).
    pub(super) websites_confirm: Option<WebsitesConfirm>,
    /// Passkeys: the list, its delete confirmation and the about box.
    pub(super) passkeys: super::security::passkeys::PasskeysUi,
    /// The last seen rule waiting on the one-time warning (tdesktop
    /// `LastSeenPrivacyController::confirmSave`).
    pub(super) last_seen_warning: Option<quill::privacy::PrivacyRuleDetail>,
}

/// A passkey ceremony still running with the OS is cancelled
/// (`close_passkey_ceremony`); the rest is flags and inputs.
impl crate::ui::on_screen::Farewell for PrivacyState {
    fn farewell(
        app: &mut QuillApp,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<QuillApp>,
    ) {
        app.close_passkey_ceremony(window, cx);
    }
}

impl PrivacyState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            extra: super::privacy_extra::PrivacyUi::new(window, cx),
            editor: None,
            exceptions: None,
            exception_picker_open: false,
            block_picker_open: false,
            blocked_scroll: ScrollHandle::new(),
            blocked_asked: Default::default(),
            unblock_confirm: None,
            device_link: Default::default(),
            sessions_confirm: None,
            websites_open: false,
            websites_confirm: None,
            passkeys: Default::default(),
            last_seen_warning: None,
        }
    }
}

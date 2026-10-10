//! Privacy and security screens: rules, blocked users, sessions and websites.

use super::*;
use gpui_kit::*;

pub(crate) struct PrivacyState {
    /// Slice S3: Privacy settings overlay (TGX Settings → Privacy).
    pub(super) open: bool,
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
    /// Slice S3: two-step Unblock confirm on the Privacy screen.
    pub(super) unblock_confirm: Option<i64>,
    /// Slice A3: Active Sessions overlay (TGX Settings → Devices /
    /// `SettingsSessionsController`).
    pub(super) sessions_open: bool,
    pub(super) device_qr_scanner: Option<super::device_qr::DeviceQrScanner>,
    pub(super) device_login_qr: Option<zeroize::Zeroizing<String>>,
    pub(super) device_link_notice: Option<&'static str>,
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
}

impl PrivacyState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            // Slice S3: privacy screen state.
            open: false,
            extra: super::privacy_extra::PrivacyUi::new(window, cx),
            editor: None,
            exceptions: None,
            exception_picker_open: false,
            block_picker_open: false,
            unblock_confirm: None,
            sessions_open: false,
            device_qr_scanner: None,
            device_login_qr: None,
            device_link_notice: None,
            sessions_confirm: None,
            websites_open: false,
            websites_confirm: None,
        }
    }
}

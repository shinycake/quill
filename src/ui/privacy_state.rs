//! Privacy and security screens: rules, blocked users, sessions and websites.
#![deny(clippy::indexing_slicing)]

use super::*;
use gpui_kit::*;

pub(crate) struct PrivacyState {
    /// B13: transient state of the privacy / security extras.
    pub(super) extra: super::privacy_extra::PrivacyUi,
    /// What the Privacy page has open over itself: the rule editor, its
    /// exceptions list and picker, the block picker, the Unblock question
    /// or the "Last seen" warning (one at a time).
    pub(super) layer: super::privacy_layer::PrivacyLayer,
    /// The blocked-users rows, which page near their end (`scroll_load`).
    pub(super) blocked_scroll: ScrollHandle,
    pub(super) blocked_asked: super::scroll_load::Asked,
    /// The "link a device" flow: scanning, or a link waiting for consent.
    pub(super) device_link: super::device_qr::DeviceLink,
    /// The box over the sessions list: a session's details (and its rename
    /// form) or the terminate question (TGX `TerminateSessionQuestion` /
    /// `TerminateIncompleteSessionQuestion` / `AreYouSureSessions`).
    pub(super) devices: super::device_layer::DeviceLayer,
    /// Slice A4: Connected Websites overlay (TGX `SettingsWebsitesController`
    /// / `WebSessionsTitle` "Logged In with Telegram") and its pending
    /// disconnect confirmation (TGX `TerminateWebSessionQuestion` /
    /// `DisconnectAllWebsitesHint`).
    pub(super) websites: super::device_layer::WebsitesScreen,
    /// Passkeys: the list, its delete confirmation and the about box.
    pub(super) passkeys: super::security::passkeys::PasskeysUi,
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
            layer: Default::default(),
            blocked_scroll: ScrollHandle::new(),
            blocked_asked: Default::default(),
            device_link: Default::default(),
            devices: Default::default(),
            websites: Default::default(),
            passkeys: Default::default(),
        }
    }
}

//! `ready-privacy-premium`: a free account's privacy rule editor over the
//! Privacy page (injected, no live Telegram). `QUILL_DEMO_PRIVACY` picks it:
//! unset or `lastseen` for Last seen with its Subscribe row, `warning` for the
//! one-time last seen warning, `voices` for Voice messages.
#![deny(clippy::indexing_slicing)]

use super::{PrivacyEditorTarget, apply_ready_privacy};
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use quill::telegram::requests::PrivacyWho;
use quill::telegram::requests_privacy::PrivacySettingKey;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-privacy-premium",
        "screenshot demo — privacy Premium upsells of a free account (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_privacy_premium),
];

impl QuillApp {
    fn demo_privacy_premium(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_PRIVACY").unwrap_or_default();
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        apply_ready_privacy(session, &self.demo_ui.sink, &self.demo_ui.seq);
        session.payments.premium_option = Some(false);
        let key = match mode.as_str() {
            "voices" => PrivacySettingKey::AllowVoiceMessages,
            _ => PrivacySettingKey::ShowStatus,
        };
        if mode == "warning" {
            session.settings.privacy.insert(
                key,
                PrivacyKeyState::Ready(PrivacyRuleDetail {
                    who: Some(PrivacyWho::Everybody),
                    ..Default::default()
                }),
            );
            self.privacy.last_seen_warning = Some(PrivacyRuleDetail {
                who: Some(PrivacyWho::Contacts),
                ..Default::default()
            });
        }
        self.open_settings(Some(quill::settings_catalog::Page::Privacy));
        self.privacy.editor = Some(PrivacyEditorTarget::Rule(key));
        cx.notify();
    }
}

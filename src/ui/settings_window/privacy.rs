//! The Privacy page (tdesktop "Privacy and Security"): security, who can
//! see and contact you, blocked users, then archive, contacts, content and
//! payments. Each rule row opens its editor over the page.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::rows::{group, link_row};
use crate::ui::navigation::NavigationAction as Nav;
use crate::ui::privacy::{PrivacyEditorTarget, new_chat_privacy_value, privacy_key_value};
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::telegram::requests_privacy::PrivacySettingKey;

impl QuillApp {
    pub(super) fn privacy_page(&mut self, cx: &mut Context<Self>) -> Div {
        let password_check = self
            .session()
            .is_some_and(|s| s.settings.privacy_data.check_password_suggested)
            .then(|| self.password_check_card(cx));
        let security: Vec<AnyElement> =
            [self.nav_row(Nav::Passcode, tr(keys::SETTINGS_PASSCODE_TITLE), None, cx)]
                .into_iter()
                .chain(self.passkeys_settings_row(cx))
                .chain([
                    self.nav_row(
                        Nav::TwoFa,
                        tr(keys::SETTINGS_CLOUD_PASSWORD_START_TITLE),
                        None,
                        cx,
                    ),
                    self.nav_row(Nav::Websites, tr(keys::SETTINGS_LOGGED_IN), None, cx),
                ])
                .collect();
        let seen: Vec<AnyElement> = PrivacySettingKey::visibility_rows()
            .into_iter()
            .map(|key| self.privacy_rule_link(key, cx))
            .collect();
        let new_chat = self
            .session()
            .map(new_chat_privacy_value)
            .unwrap_or_default();
        let contact = vec![
            self.privacy_rule_link(PrivacySettingKey::AllowCalls, cx),
            self.privacy_rule_link(PrivacySettingKey::PeerToPeer, cx),
            self.privacy_rule_link(PrivacySettingKey::AllowVoiceMessages, cx),
            link_row(
                "privacy-rule-new-chat",
                tr(keys::SETTINGS_MESSAGES_PRIVACY),
                Some(new_chat.into()),
                cx.listener(|this, _, _, cx| this.open_new_chat_editor(cx)),
                cx,
            ),
            self.privacy_rule_link(PrivacySettingKey::AllowChatInvites, cx),
        ];
        div()
            .flex()
            .flex_col()
            .gap_6()
            .w_full()
            .children(password_check)
            .child(group(Some(tr(keys::Q_SETTINGS_PRIVACY_SECURITY)), security))
            .child(group(Some(tr(keys::Q_PREFS_WHO_CAN_SEE)), seen))
            .child(group(Some(tr(keys::Q_PREFS_WHO_CAN_CONTACT_ME)), contact))
            .children(super::pages::cards([self.privacy_blocked_section(cx)]))
            .children(super::pages::cards([
                self.archive_privacy_section(cx),
                self.privacy_frequent_contacts_section(cx),
            ]))
            .children(super::pages::cards([
                self.privacy_security_section(cx),
                self.privacy_payments_section(cx),
            ]))
    }

    /// One rule: its name and who it allows now; opens the rule's editor.
    fn privacy_rule_link(&self, key: PrivacySettingKey, cx: &mut Context<Self>) -> AnyElement {
        let value = self
            .session()
            .map(|s| privacy_key_value(s, key))
            .unwrap_or_default();
        link_row(
            SharedString::from(format!("privacy-rule-{}", key.td_type())),
            key.label(),
            Some(value.into()),
            cx.listener(move |this, _, _, cx| {
                this.privacy.editor = Some(PrivacyEditorTarget::Rule(key));
                this.privacy.exception_picker_open = false;
                cx.notify();
            }),
            cx,
        )
    }
}

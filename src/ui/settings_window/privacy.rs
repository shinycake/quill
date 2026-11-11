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
use quill::settings_catalog::Page;
use quill::telegram::requests_privacy::PrivacySettingKey;

impl QuillApp {
    /// A password typed in the Privacy page's check card stays only while
    /// that page is on screen: leaving the page or closing Settings empties
    /// the field, as the 2FA box empties its own on close (`close_twofa`).
    pub(in crate::ui) fn sync_check_password(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.open && self.settings.page == Some(Page::Privacy) {
            return;
        }
        let field = self.twofa.check_password.clone();
        if !field.read(cx).value().is_empty() {
            field.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    pub(super) fn privacy_page(&mut self, cx: &mut Context<Self>) -> Div {
        let password_check = self
            .session()
            .is_some_and(|s| s.settings.privacy_data.check_password_suggested)
            .then(|| {
                let field = self.twofa.check_password.clone();
                self.password_check_card(&field, cx)
            });
        let passkeys = self.passkeys_settings_row(cx);
        let passcode = self.nav_row(Nav::Passcode, tr(keys::SETTINGS_PASSCODE_TITLE), None, cx);
        let two_step = self.nav_row(
            Nav::TwoFa,
            tr(keys::SETTINGS_CLOUD_PASSWORD_START_TITLE),
            None,
            cx,
        );
        // Without a passkeys row or a password check, the two-step row
        // stands in for them.
        let two_step =
            self.setting_stand_in(passkeys.is_none(), &["privacy.passkeys"], two_step, cx);
        let two_step = self.setting_stand_in(
            password_check.is_none(),
            &["privacy.check_password"],
            two_step,
            cx,
        );
        let security: Vec<AnyElement> = [self.setting_spot(
            &[
                "privacy.passcode",
                "privacy.auto_lock",
                "privacy.system_unlock",
            ],
            passcode,
            cx,
        )]
        .into_iter()
        .chain(passkeys.map(|row| self.setting_spot(&["privacy.passkeys"], row, cx)))
        .chain([
            self.setting_spot(&["privacy.two_step"], two_step, cx),
            self.setting_spot(
                &["privacy.websites"],
                self.nav_row(Nav::Websites, tr(keys::SETTINGS_LOGGED_IN), None, cx),
                cx,
            ),
        ])
        .collect();
        let password_check =
            password_check.map(|card| self.setting_spot(&["privacy.check_password"], card, cx));
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
            self.setting_spot(
                &["privacy.messages", "privacy.paid_messages"],
                link_row(
                    "privacy-rule-new-chat",
                    tr(keys::SETTINGS_MESSAGES_PRIVACY),
                    Some(new_chat.into()),
                    cx.listener(|this, _, _, cx| this.open_new_chat_editor(cx)),
                    cx,
                ),
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
        let row = link_row(
            SharedString::from(format!("privacy-rule-{}", key.td_type())),
            key.label(),
            Some(value.into()),
            cx.listener(move |this, _, _, cx| {
                this.privacy.editor = Some(PrivacyEditorTarget::Rule(key));
                this.privacy.exception_picker_open = false;
                cx.notify();
            }),
            cx,
        );
        self.setting_spot(rule_settings(key), row, cx)
    }
}

/// The catalog entries a rule's row stands for: some of them are switches
/// inside the rule's editor.
fn rule_settings(key: PrivacySettingKey) -> &'static [&'static str] {
    match key {
        PrivacySettingKey::ShowPhoneNumber => &["privacy.phone_number", "privacy.find_by_number"],
        PrivacySettingKey::ShowStatus => &["privacy.last_seen", "privacy.read_time"],
        PrivacySettingKey::ShowProfilePhoto => &["privacy.profile_photo"],
        PrivacySettingKey::ShowBio => &["privacy.bio"],
        PrivacySettingKey::ShowBirthdate => &["privacy.birthdate"],
        PrivacySettingKey::AutosaveGifts => {
            &["privacy.gifts", "privacy.gift_icon", "privacy.gift_types"]
        }
        PrivacySettingKey::ShowProfileAudio => &["privacy.saved_music"],
        PrivacySettingKey::ShowLinkInForwardedMessages => &["privacy.forwarded"],
        PrivacySettingKey::AllowCalls => &["privacy.calls"],
        PrivacySettingKey::PeerToPeer => &["privacy.peer_to_peer"],
        PrivacySettingKey::AllowVoiceMessages => &["privacy.voice_messages"],
        PrivacySettingKey::AllowChatInvites => &["privacy.group_invites"],
        PrivacySettingKey::AllowFindingByPhoneNumber | PrivacySettingKey::AllowUnpaidMessages => {
            &[]
        }
    }
}

//! Premium upsells of the privacy editor (tdesktop
//! settings_privacy_controllers.cpp): the Subscribe rows under Last seen and
//! Voice messages, the toast of a locked voice message choice, and the
//! one-time warning before a free account hides its last seen.
#![deny(clippy::indexing_slicing)]

use crate::ui::ai_compose::marked_link;
use crate::ui::app::QuillApp;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};
use quill::privacy::prefs::{
    last_seen_warning_due, last_seen_warning_seen, set_last_seen_warning_seen,
};
use quill::privacy::{PrivacyKeyState, PrivacyRuleDetail};
use quill::telegram::requests_privacy::PrivacySettingKey;

/// One "Subscribe to Telegram Premium" row and the note under it.
pub(super) struct SubscribeRow {
    pub(super) id: &'static str,
    pub(super) label: Key,
    pub(super) about: Key,
    pub(super) icon: Option<IconName>,
    /// The `/start` ref of the Premium page it opens.
    pub(super) premium_ref: &'static str,
}

/// Last seen's row (`lng_edit_lastseen_subscribe`, ref `lastseen`).
pub(super) const LAST_SEEN_SUBSCRIBE: SubscribeRow = SubscribeRow {
    id: "privacy-lastseen-subscribe",
    label: keys::EDIT_LASTSEEN_SUBSCRIBE,
    about: keys::EDIT_LASTSEEN_SUBSCRIBE_ABOUT,
    icon: None,
    premium_ref: "lastseen",
};

/// Voice messages' row (`lng_messages_privacy_premium_button`).
pub(super) const VOICE_SUBSCRIBE: SubscribeRow = SubscribeRow {
    id: "privacy-voices-subscribe",
    label: keys::MESSAGES_PRIVACY_PREMIUM_BUTTON,
    about: keys::MESSAGES_PRIVACY_PREMIUM_ABOUT,
    icon: Some(IconName::Star),
    premium_ref: VOICE_PREMIUM_REF,
};

const VOICE_PREMIUM_REF: &str = "voice_restrictions_require_premium";

impl QuillApp {
    pub(super) fn privacy_premium(&self) -> bool {
        self.session().is_some_and(|s| s.my_is_premium())
    }

    pub(super) fn privacy_subscribe_row(
        &self,
        row: SubscribeRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let premium_ref = row.premium_ref;
        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_1()
            .child(
                div().debug_selector(move || row.id.to_string()).child(
                    Button::new(row.id)
                        .label(tr(row.label))
                        .ghost()
                        .small()
                        .when_some(row.icon, |button, icon| button.icon(icon))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_premium_from(premium_ref, cx);
                        })),
                ),
            )
            .child(
                div()
                    .text_xs()
                    .px_2()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(row.about)),
            )
            .into_any_element()
    }

    /// A free account chose to restrict voice messages: the toast whose
    /// "Telegram Premium" opens the Premium page (tdesktop
    /// `VoicesPrivacyController::premiumClickedCallback`).
    pub(super) fn voice_privacy_premium_toast(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let link = format!("**{}**", tr(keys::SETTINGS_PRIVACY_PREMIUM_LINK));
        let marked = marked_link(tr(keys::SETTINGS_PRIVACY_PREMIUM), "link", &link);
        let toast =
            Self::marked_toast("privacy-voices-premium-toast", marked, cx, |this, _, cx| {
                this.open_premium_from(VOICE_PREMIUM_REF, cx);
            })
            .icon(IconName::Lock);
        cx.defer_in(window, move |_, window, cx| {
            window.push_notification(toast, cx);
        });
    }

    /// Save `detail` as the `key` rule, asking first when it is the first
    /// time a free account hides its last seen.
    pub(super) fn save_privacy_rule(
        &mut self,
        key: PrivacySettingKey,
        detail: PrivacyRuleDetail,
        cx: &mut Context<Self>,
    ) {
        if key == PrivacySettingKey::ShowStatus
            && last_seen_warning_due(&detail, self.privacy_premium(), last_seen_warning_seen())
        {
            self.privacy.last_seen_warning = Some(detail);
            cx.notify();
            return;
        }
        self.send_privacy_rule(key, detail, cx);
    }

    /// Live: `setUserPrivacySettingRules`; demo: store the rule.
    fn send_privacy_rule(
        &mut self,
        key: PrivacySettingKey,
        detail: PrivacyRuleDetail,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let result = { live.driver.set_privacy_rules(key, detail) };
            if let Err(err) = result {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_PRIVACY_UPDATE,
                    &err,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.settings
                .privacy
                .insert(key, PrivacyKeyState::Ready(detail));
        }
        cx.notify();
    }

    fn confirm_last_seen_warning(&mut self, cx: &mut Context<Self>) {
        set_last_seen_warning_seen(true);
        if let Some(detail) = self.privacy.last_seen_warning.take() {
            self.send_privacy_rule(PrivacySettingKey::ShowStatus, detail, cx);
        }
        cx.notify();
    }

    fn dismiss_last_seen_warning(&mut self, cx: &mut Context<Self>) {
        self.privacy.last_seen_warning = None;
        cx.notify();
    }

    fn build_last_seen_warning(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::LastSeenWarning, |this, _, cx| {
                this.dismiss_last_seen_warning(cx)
            });
        app.update(cx, |_, cx| {
            let footer = div()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("lastseen-warning-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.dismiss_last_seen_warning(cx);
                            this.close_kit_dialog_if_done(DialogKind::LastSeenWarning, window, cx);
                        })),
                )
                .child(
                    Button::new("lastseen-warning-continue")
                        .label(tr(keys::CONTINUE))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_last_seen_warning(cx);
                            this.close_kit_dialog_if_done(DialogKind::LastSeenWarning, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .w(px(380.))
                .content(|content, _, _| {
                    content.child(
                        div()
                            .id("lastseen-warning")
                            .text_sm()
                            .child(tr(keys::EDIT_PRIVACY_LASTSEEN_WARNING)),
                    )
                })
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// Before a free account first hides its last seen.
    LastSeenWarning => DialogSpec::new(
        3796,
        |app| app.privacy.last_seen_warning.is_some(),
        QuillApp::build_last_seen_warning,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "premium_rows_tests.rs"]
mod tests;

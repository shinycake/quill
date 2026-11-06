//! The bottom of the Settings list: Telegram FAQ, Telegram Features,
//! Ask a Question, and the version line with its changelog link
//! (tdesktop `settings_main.cpp`, `BuildHelpSection`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::settings_window::rows::{group, icon_row};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The Help group under About: FAQ, Features, the privacy policy and
    /// Ask a Question.
    pub(super) fn settings_help_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let web = |id: &'static str, icon, title, url: &'static str, cx: &mut Context<Self>| {
            icon_row(
                id,
                icon,
                title,
                IconName::ExternalLink,
                cx.listener(move |this, _, _, cx| this.open_link(url, cx)),
                cx,
            )
        };
        let faq = web(
            "settings-faq",
            IconName::CircleQuestionMark,
            tr(keys::SETTINGS_FAQ),
            quill::settings_account::FAQ_URL,
            cx,
        );
        let features = web(
            "settings-features",
            IconName::Lightbulb,
            tr(keys::SETTINGS_FEATURES),
            quill::settings_account::FEATURES_URL,
            cx,
        );
        let privacy = web(
            "settings-privacy-policy",
            IconName::Shield,
            tr(keys::Q_PREFS_PRIVACY_POLICY),
            quill::settings_account::PRIVACY_POLICY_URL,
            cx,
        );
        let ask = icon_row(
            "settings-ask-question",
            IconName::MessageCircle,
            tr(keys::SETTINGS_ASK_QUESTION),
            IconName::ChevronRight,
            cx.listener(|this, _, _, cx| {
                this.show_settings_sub(super::settings_window::SubPage::AskQuestion, cx);
            }),
            cx,
        );
        group(
            Some(tr(keys::Q_SETTINGS_ABOUT_SEC_HELP)),
            [
                self.setting_spot(&["about.faq"], faq, cx),
                self.setting_spot(&["about.features"], features, cx),
                self.setting_spot(&["about.ask_question"], ask, cx),
                self.setting_spot(&["about.privacy_policy"], privacy, cx),
            ],
        )
        .into_any_element()
    }

    /// The "Ask a Question" subpage: volunteers answer slowly, so offer
    /// the FAQ first (tdesktop `OpenAskQuestionConfirm`).
    pub(super) fn ask_question_page(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id("ask-question-note")
                    .text_sm()
                    .child(quill::settings_account::ASK_QUESTION_NOTE),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("ask-question-faq")
                            .label(tr(keys::SETTINGS_FAQ_BUTTON))
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_link(quill::settings_account::FAQ_URL, cx)
                            })),
                    )
                    .child(
                        Button::new("ask-question-volunteer")
                            .label(tr(keys::SETTINGS_ASK_OK))
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.ask_support(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn ask_support(&mut self, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.open_support_chat());
        match sent {
            Some(Ok(_)) => {
                self.close_settings();
            }
            Some(Err(_)) => self.set_status_note(
                StatusNote::error(tr(keys::Q_PREFS_COULDNT_REACH_TELEGRAM_SUPPORT_TRY)),
                cx,
            ),
            None => self.set_status_note(
                StatusNote::silent(tr(keys::Q_PREFS_SIGN_IN_TO_WRITE_TO)),
                cx,
            ),
        }
        cx.notify();
    }
}

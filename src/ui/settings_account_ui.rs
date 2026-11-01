//! The bottom of the Settings list: Telegram FAQ, Telegram Features,
//! Ask a Question, and the version line with its changelog link
//! (tdesktop `settings_main.cpp`, `BuildHelpSection`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    /// Help rows and the version footer, below the Settings list.
    pub(super) fn settings_help_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mt_2()
            .child(
                Button::new("settings-faq")
                    .label(tr(keys::SETTINGS_FAQ))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_link(quill::settings_account::FAQ_URL, cx)
                    })),
            )
            .child(
                Button::new("settings-features")
                    .label(tr(keys::SETTINGS_FEATURES))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_link(quill::settings_account::FEATURES_URL, cx)
                    })),
            )
            .child(
                Button::new("settings-privacy-policy")
                    .label(tr(keys::Q_PREFS_PRIVACY_POLICY))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_link(quill::settings_account::PRIVACY_POLICY_URL, cx)
                    })),
            )
            .child(
                Button::new("settings-ask-question")
                    .label(tr(keys::SETTINGS_ASK_QUESTION))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_settings_sub(super::settings_window::SubPage::AskQuestion, cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .pt_2()
                    .child(
                        div()
                            .id("settings-version")
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(quill::settings_account::current_version_line()),
                    )
                    .child(
                        Button::new("settings-changelog")
                            .label(tr(keys::Q_PREFS_WHATS_NEW))
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| this.open_whats_new(cx))),
                    ),
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

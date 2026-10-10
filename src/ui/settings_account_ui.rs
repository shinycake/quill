//! The bottom of the Settings list: Telegram FAQ, Telegram Features,
//! Ask a Question, and the version line with its changelog link
//! (tdesktop `settings_main.cpp`, `BuildHelpSection`).

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;

/// The Settings subpage that asks before writing to Telegram Support.
pub(super) const ASK_QUESTION_PAGE: &str = "Ask a Question";

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
                    .label("Telegram FAQ")
                    .ghost()
                    .on_click(
                        cx.listener(|_, _, _, cx| cx.open_url(quill::settings_account::FAQ_URL)),
                    ),
            )
            .child(
                Button::new("settings-features")
                    .label("Telegram Features")
                    .ghost()
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.open_url(quill::settings_account::FEATURES_URL)
                    })),
            )
            .child(
                Button::new("settings-privacy-policy")
                    .label("Privacy Policy")
                    .ghost()
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.open_url(quill::settings_account::PRIVACY_POLICY_URL)
                    })),
            )
            .child(
                Button::new("settings-ask-question")
                    .label(ASK_QUESTION_PAGE)
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings.page = Some(ASK_QUESTION_PAGE);
                        cx.notify();
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
                            .label("What's new")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.open_url(quill::settings_account::CHANGELOG_URL)
                            })),
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
                            .label("Go to FAQ")
                            .outline()
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.open_url(quill::settings_account::FAQ_URL)
                            })),
                    )
                    .child(
                        Button::new("ask-question-volunteer")
                            .label("Ask a Volunteer")
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ask_support(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn ask_support(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.open_support_chat());
        match sent {
            Some(Ok(_)) => {
                self.settings.open = false;
                self.settings.page = None;
                window.close_dialog(cx);
            }
            Some(Err(_)) => self.set_status_note("Couldn't reach Telegram Support; try again.", cx),
            None => self.set_status_note("Sign in to write to Telegram Support.", cx),
        }
        cx.notify();
    }
}

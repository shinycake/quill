//! The "Is +X still your number?" block at the top of Settings (tdesktop
//! `SetupValidatePhoneNumberSuggestion`). It shows while TDLib has
//! `suggestedActionCheckPhoneNumber` pending. Yes hides the suggestion;
//! No explains that the number is changed in the official app.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::group_panels::format_phone;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chatlist_suggestions::{ACTION_PHONE, Suggestion};
use quill::lang::{keys, tr};

impl QuillApp {
    /// The prompt, or `None` when TDLib has no phone suggestion pending.
    pub(super) fn settings_phone_suggestion(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self
            .session()?
            .chat_list
            .suggestions
            .actions
            .contains(ACTION_PHONE)
        {
            return None;
        }
        self.settings_phone_suggestion_body(cx)
    }

    /// `settings_phone_suggestion` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn settings_phone_suggestion_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        if !session.chat_list.suggestions.actions.contains(ACTION_PHONE) {
            return None;
        }
        let phone = session
            .my_user_id
            .and_then(|me| session.user(me))
            .map(|user| format_phone(&user.phone_number))
            .unwrap_or_default();
        let (muted, border) = (cx.theme().muted_foreground, cx.theme().border);
        let note = self.settings.phone_change_note;
        Some(
            div()
                .id("settings-phone-suggestion")
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .mb_2()
                .rounded_md()
                .border_1()
                .border_color(border)
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .child(quill::phone_suggestion::title(&phone)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(quill::phone_suggestion::ABOUT),
                )
                .child(
                    div().flex().gap_2().pt_1().children([
                        Button::new("settings-phone-yes")
                            .label(tr(keys::BOX_YES))
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_suggestion(&Suggestion::CheckPhone, cx);
                            }))
                            .into_any_element(),
                        Button::new("settings-phone-no")
                            .label(tr(keys::BOX_NO))
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.phone_change_note = true;
                                cx.notify();
                            }))
                            .into_any_element(),
                        Button::new("settings-phone-learn-more")
                            .label(tr(keys::Q_PREFS_LEARN_MORE))
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_link(quill::phone_suggestion::LEARN_MORE_URL, cx)
                            }))
                            .into_any_element(),
                    ]),
                )
                .when(note, |this| {
                    this.child(
                        div()
                            .pt_1()
                            .text_xs()
                            .child(quill::phone_suggestion::CHANGE_NOTE),
                    )
                })
                .into_any_element(),
        )
    }
}

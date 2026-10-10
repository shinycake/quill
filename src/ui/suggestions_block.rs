//! The suggestions block on top of the chat list (tdesktop
//! `Dialogs::TopBarSuggestionContent`); what it shows and says is decided
//! in `quill::chatlist_suggestions`.

use super::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chatlist_suggestions::{Suggestion, copy, pick};

impl QuillApp {
    fn current_suggestion(&self) -> Option<Suggestion> {
        let facts = &self.session()?.suggestions;
        let today = quill::local_time::civil_local(quill::local_time::now_unix());
        pick(facts, today.month, today.day)
    }

    /// Dismiss: `hideSuggestedAction` / `hideContactCloseBirthdays`.
    fn dismiss_suggestion(&mut self, suggestion: &Suggestion, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.hide_suggestion(suggestion).is_err() {
                    self.connection.status_note = "Could not hide the suggestion".into();
                }
            }
            None => {
                if let Some(demo) = self.demo_session.as_mut() {
                    match quill::chatlist_suggestions::dismiss_action(suggestion) {
                        Some(action) => {
                            demo.suggestions.actions.remove(action);
                        }
                        None => demo.suggestions.birthdays_hidden = true,
                    }
                }
            }
        }
        cx.notify();
    }

    /// The tap on the card itself.
    fn open_suggestion(
        &mut self,
        suggestion: &Suggestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use super::navigation::NavigationAction as Nav;
        match suggestion {
            Suggestion::Birthdays(ids) => {
                if let Some(user_id) = ids.first() {
                    self.open_user_chat(*user_id, window, cx);
                }
            }
            Suggestion::SetBirthdate | Suggestion::SetProfilePhoto => {
                self.navigate(Nav::Profile, window, cx)
            }
            Suggestion::CheckPassword => {
                self.settings.open = true;
                self.navigate(Nav::Privacy, window, cx);
            }
            Suggestion::UpgradePremium => self.navigate(Nav::Premium, window, cx),
            Suggestion::CheckPhone => {}
        }
    }

    /// Settings > Contacts: contacts whose birthday is yesterday, today or
    /// tomorrow, from `updateContactCloseBirthdays`. A tap opens the chat.
    pub(super) fn birthday_contacts_block(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let list = &session.suggestions.close_birthdays;
        if list.is_empty() {
            return None;
        }
        let today = quill::local_time::civil_local(quill::local_time::now_unix());
        let muted = cx.theme().muted_foreground;
        let mut block = div()
            .id("birthday-contacts")
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(muted)
                    .child("BIRTHDAYS"),
            );
        for entry in list {
            let user_id = entry.user_id;
            let name = session
                .user(user_id)
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}"));
            let when = quill::chatlist_suggestions::birthday_label(entry, today.month, today.day);
            block = block.child(
                div()
                    .id(("birthday-contact", user_id as u64))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("{name}, {when}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_user_chat(user_id, window, cx);
                    }))
                    .child(div().text_sm().truncate().child(name))
                    .child(div().text_xs().text_color(muted).child(when)),
            );
        }
        Some(
            block
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Contacts celebrating yesterday, today or tomorrow."),
                )
                .into_any_element(),
        )
    }

    pub(super) fn suggestion_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let suggestion = self.current_suggestion()?;
        let session = self.session()?;
        let names: Vec<String> = match &suggestion {
            Suggestion::Birthdays(ids) => ids
                .iter()
                .filter_map(|id| session.user(*id).map(|user| user.display_name()))
                .collect(),
            _ => Vec::new(),
        };
        let phone = session
            .my_user_id
            .and_then(|me| session.user(me))
            .map(|user| user.phone_number.clone())
            .unwrap_or_default();
        let (title, about) = copy(&suggestion, &names, &phone);
        let theme = cx.theme();
        let (muted, border, fill) = (
            theme.muted_foreground,
            theme.border,
            theme.accent.opacity(0.1),
        );
        let phone_check = matches!(suggestion, Suggestion::CheckPhone);
        let tap = suggestion.clone();
        let dismiss = suggestion.clone();
        let keep = suggestion.clone();
        let body = div()
            .id("suggestion-body")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_0p5()
            .when(!phone_check, |this| this.cursor_pointer())
            .child(div().text_sm().font_semibold().child(title))
            .child(div().text_xs().text_color(muted).child(about))
            .when(!phone_check, |this| {
                this.on_click(cx.listener(move |this, _, window, cx| {
                    this.open_suggestion(&tap, window, cx);
                }))
            })
            .when(phone_check, |this| {
                this.child(
                    div()
                        .flex()
                        .gap_2()
                        .pt_1()
                        .child(
                            Button::new("suggestion-phone-yes")
                                .label("Yes")
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.dismiss_suggestion(&keep, cx);
                                })),
                        )
                        .child(
                            Button::new("suggestion-phone-no")
                                .label("No")
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.connection.status_note = "Please change your phone number in the official Telegram app on your phone as soon as possible.".into();
                                    cx.notify();
                                })),
                        ),
                )
            });
        Some(
            div()
                .id("suggestion-card")
                .flex()
                .items_start()
                .gap_2()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(border)
                .bg(fill)
                .child(body)
                .child(
                    Button::new("suggestion-dismiss")
                        .icon(IconName::X)
                        .ghost()
                        .small()
                        .tooltip("Hide")
                        .accessibility_label("Hide suggestion")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dismiss_suggestion(&dismiss, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

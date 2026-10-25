//! The suggestions block on top of the chat list (tdesktop
//! `Dialogs::TopBarSuggestionContent`); what it shows and says is decided
//! in `quill::chatlist_suggestions`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::chatlist_suggestions::{Suggestion, copy, pick};
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    fn current_suggestion(&self) -> Option<Suggestion> {
        let session = self.session()?;
        let today = quill::local_time::civil_local(quill::local_time::now_unix());
        pick(
            &session.chat_list.suggestions,
            today.month,
            today.day,
            session.premium_possible(),
        )
    }

    /// Dismiss: `hideSuggestedAction` / `hideContactCloseBirthdays`. The
    /// birthday card says where gifts can still be sent.
    pub(super) fn dismiss_suggestion(&mut self, suggestion: &Suggestion, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.hide_suggestion(suggestion).is_err() {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_MISC_COULD_NOT_HIDE_THE_SUGGESTION,
                    )));
                    cx.notify();
                    return;
                }
            }
            None => {
                if let Some(demo) = self.demo_session.as_mut() {
                    match quill::chatlist_suggestions::dismiss_action(suggestion) {
                        Some(action) => {
                            demo.chat_list.suggestions.actions.remove(action);
                        }
                        None => demo.chat_list.suggestions.birthdays_hidden = true,
                    }
                }
            }
        }
        if matches!(suggestion, Suggestion::Birthdays(_)) {
            self.connection.set_note(StatusNote::info(tr(
                keys::DIALOGS_SUGGESTIONS_BIRTHDAY_CONTACT_DISMISS,
            )));
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
            // tdesktop: one contact opens the gift picker for them, more
            // open the gift recipient list.
            Suggestion::Birthdays(ids) => match ids.as_slice() {
                [user_id] => self.open_send_gift_to_user(*user_id, window, cx),
                _ => self.open_gift_recipients(window, cx),
            },
            Suggestion::SetBirthdate | Suggestion::SetProfilePhoto => {
                self.navigate(Nav::Profile, window, cx)
            }
            Suggestion::CheckPassword => {
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
        let list = &session.chat_list.suggestions.close_birthdays;
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
                    .child(tr(keys::GIFT_SUBTITLE_BIRTHDAYS).to_uppercase()),
            );
        for entry in list {
            let user_id = entry.user_id;
            let name = session
                .user(user_id)
                .map(|user| user.display_name())
                .unwrap_or_else(|| {
                    tr_with(keys::Q_MISC_USER, &[("user_id", &user_id.to_string())])
                });
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
                        .child(tr(keys::Q_MISC_CONTACTS_CELEBRATING_YESTERDAY_TODAY_OR)),
                )
                .into_any_element(),
        )
    }

    pub(super) fn suggestion_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // Running gift auctions with a bid of the user take the slot
        // first (tdesktop ranks them above every suggestion but the
        // unreviewed-login one, `Priority::GiftAuctions`).
        if let Some(bar) = self.gift_auction_bar(cx) {
            return Some(bar);
        }
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
            .map(|user| super::group_panels::format_phone(&user.phone_number))
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
            .debug_selector(|| "suggestion-body".into())
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_0p5()
            .when(!phone_check, |this| {
                this.cursor_pointer()
                    .role(gpui_kit::Role::Button)
                    .aria_label(title.clone())
            })
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
                                .label(tr(keys::BOX_YES))
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.dismiss_suggestion(&keep, cx);
                                })),
                        )
                        .child(
                            Button::new("suggestion-phone-no")
                                .label(tr(keys::BOX_NO))
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.connection.set_note(StatusNote::silent(
                                        quill::phone_suggestion::CHANGE_NOTE,
                                    ));
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
                        .tooltip(tr(keys::Q_MISC_HIDE))
                        .accessibility_label(tr(keys::Q_MISC_HIDE_SUGGESTION))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dismiss_suggestion(&dismiss, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

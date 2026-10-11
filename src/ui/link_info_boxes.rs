//! Boxes for `t.me/giftcode/<code>` and `t.me/setlanguage/<id>` links
//! (`parity:deeplink-giftcode`, `parity:deeplink-premium-language`).
//!
//! The gift code box mirrors tdesktop's `GiftCodeBox`: who made the code,
//! who it is for, what it gives, why, and when. An unused code gets a
//! "Use Link" button; only that click sends `applyPremiumGiftCode`. The
//! language box shows the pack and says Quill ships English only; it never
//! changes a setting.
use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::deep_link_types::{gift_duration, gift_reason, language_progress};
use quill::state::{GiftCodeLookup, LanguageLinkLookup, Session};
use quill::telegram::envelope::MessageSender;
use std::cell::RefCell;
use std::rc::Rc;

fn sender_name(session: Option<&Session>, sender: MessageSender) -> String {
    match sender {
        MessageSender::User { user_id } => session
            .and_then(|s| s.users.get(&user_id))
            .map(|u| u.display_name())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "A Telegram user".into()),
        MessageSender::Chat { chat_id } => session
            .and_then(|s| s.chats.get(&chat_id))
            .map(|c| c.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "A channel".into()),
    }
}

fn info_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .gap_4()
        .text_sm()
        .child(div().text_color(text_muted()).child(label))
        .child(div().text_right().child(value))
}

fn finish(
    dialog: Dialog,
    title: &'static str,
    body: AnyElement,
    footer: AnyElement,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Dialog {
    let body = Rc::new(RefCell::new(Some(body)));
    dialog
        .overlay(true)
        .title(crate::ui::shell::dialog_title(title))
        .content(crate::ui::shell::scrollable_dialog_content(
            move |content, _, _| {
                content.child(
                    body.borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element()),
                )
            },
        ))
        .footer(footer)
        .on_close(on_close)
}

impl QuillApp {
    fn link_info_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        }
    }

    /// `giftcode` link: open the box and check the code. Nothing is applied.
    pub(super) fn open_gift_code_link(&mut self, code: String, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.request_gift_code_info(&code).is_err() {
                    self.connection.status_note = "could not check the gift code".into();
                }
            }
            None => {
                self.connection.status_note = "gift codes need a signed-in account".into();
            }
        }
        cx.notify();
    }

    /// `setlanguage` link: look the pack up and show it.
    pub(super) fn open_language_pack_link(&mut self, id: String, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.request_language_pack_info(&id).is_err() {
                    self.connection.status_note = "could not look up the language".into();
                }
            }
            None => {
                self.connection.status_note = "language links need a signed-in account".into();
            }
        }
        cx.notify();
    }

    fn close_gift_code_box(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.link_info_session_mut() {
            session.payments.gift_code = None;
        }
        cx.notify();
    }

    fn close_language_box(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.link_info_session_mut() {
            session.settings.language_link = None;
        }
        cx.notify();
    }

    /// The Apply button: the only place that sends `applyPremiumGiftCode`.
    fn apply_gift_code_clicked(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.apply_gift_code().is_err()
        {
            self.connection.status_note = "could not use the gift code".into();
        }
        cx.notify();
    }

    fn gift_code_body(session: Option<&Session>, lookup: &GiftCodeLookup) -> AnyElement {
        let mut body = div().flex().flex_col().gap_2();
        if lookup.loading {
            return body
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("Checking the gift code…"),
                )
                .into_any_element();
        }
        let Some(info) = lookup.info.as_ref() else {
            let reason = lookup
                .error
                .clone()
                .unwrap_or_else(|| "This gift code link has expired.".into());
            return body
                .child(div().id("gift-code-error").text_sm().child(reason))
                .into_any_element();
        };
        let used = info.is_used();
        body = body.child(div().text_sm().text_color(text_muted()).child(if used {
            "This link was used to activate a Telegram Premium subscription."
        } else {
            "This link activates a Telegram Premium subscription."
        }));
        if let Some(creator) = info.creator {
            body = body.child(info_row("From", sender_name(session, creator)));
            body = body.child(info_row(
                "To",
                if info.user_id > 0 {
                    sender_name(
                        session,
                        MessageSender::User {
                            user_id: info.user_id,
                        },
                    )
                } else {
                    "No recipient".into()
                },
            ));
        }
        body = body.child(info_row(
            "Gift",
            format!(
                "Telegram Premium for {}",
                gift_duration(info.month_count, info.day_count)
            ),
        ));
        if info.creator.is_some() {
            body = body.child(info_row(
                "Reason",
                gift_reason(info.is_from_giveaway, info.user_id > 0 || used).into(),
            ));
        }
        if info.creation_date > 0 {
            body = body.child(info_row(
                "Date",
                quill::local_time::full_stamp(&quill::local_time::civil_local(info.creation_date)),
            ));
        }
        if used {
            body = body.child(div().id("gift-code-used").text_sm().child(format!(
                "This link was used on {}.",
                quill::local_time::full_stamp(&quill::local_time::civil_local(info.use_date))
            )));
        }
        if lookup.applied {
            body = body.child(
                div()
                    .id("gift-code-applied")
                    .text_sm()
                    .child("Telegram Premium is now active on your account."),
            );
        }
        if let Some(error) = lookup.error.clone() {
            body = body.child(
                div()
                    .id("gift-code-error")
                    .text_sm()
                    .text_color(danger_dark())
                    .child(error),
            );
        }
        body.into_any_element()
    }

    pub(super) fn build_gift_code_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GiftCodeLink, |this, _, cx| {
                this.close_gift_code_box(cx);
            });
        app.update(cx, |this, cx| {
            let session = this.session();
            let lookup = session.and_then(|s| s.payments.gift_code.clone());
            let Some(lookup) = lookup else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Gift Link"))
                    .on_close(on_close.clone());
            };
            let body = Self::gift_code_body(session, &lookup);
            let can_apply = !lookup.loading
                && !lookup.applied
                && lookup.info.as_ref().is_some_and(|i| !i.is_used());
            let applying = lookup.applying;
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("gift-code-close")
                        .label(if lookup.applied { "Done" } else { "Close" })
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_gift_code_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::GiftCodeLink, window, cx);
                        })),
                )
                .when(can_apply, |footer| {
                    footer.child(
                        Button::new("gift-code-apply")
                            .label("Use Link")
                            .loading(applying)
                            .disabled(applying)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.apply_gift_code_clicked(cx)),
                            ),
                    )
                })
                .into_any_element();
            finish(dialog, "Gift Link", body, footer, on_close)
        })
    }

    fn language_body(lookup: &LanguageLinkLookup) -> AnyElement {
        let mut body = div().flex().flex_col().gap_2();
        if lookup.loading {
            return body
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("Looking up the language…"),
                )
                .into_any_element();
        }
        let Some(info) = lookup.info.as_ref() else {
            return body
                .child(
                    div().id("language-link-error").text_sm().child(
                        lookup
                            .error
                            .clone()
                            .unwrap_or_else(|| "This language link is not valid.".into()),
                    ),
                )
                .into_any_element();
        };
        let title = if info.native_name.is_empty() {
            info.name.clone()
        } else {
            info.native_name.clone()
        };
        body = body.child(div().text_lg().font_semibold().child(title));
        if !info.name.is_empty() && info.name != info.native_name {
            body = body.child(info_row("Language", info.name.clone()));
        }
        if let Some(progress) =
            language_progress(info.total_string_count, info.translated_string_count)
        {
            body = body.child(info_row("Progress", progress));
        }
        if info.is_beta {
            body = body.child(info_row("Status", "Beta".into()));
        }
        body = body.child(
            div()
                .text_sm()
                .text_color(text_muted())
                .child("Quill is available in English only for now, so this language is not applied. Nothing was changed."),
        );
        body.into_any_element()
    }

    pub(super) fn build_language_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::LanguageLink, |this, _, cx| {
                this.close_language_box(cx);
            });
        app.update(cx, |this, cx| {
            let lookup = this
                .session()
                .and_then(|s| s.settings.language_link.clone());
            let Some(lookup) = lookup else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Language"))
                    .on_close(on_close.clone());
            };
            let body = Self::language_body(&lookup);
            let footer = div()
                .flex()
                .justify_end()
                .child(
                    Button::new("language-link-close")
                        .label("Close")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_language_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::LanguageLink, window, cx);
                        })),
                )
                .into_any_element();
            finish(dialog, "Language", body, footer, on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// `giftcode` link: code details and the Use Link button.
    GiftCodeLink => DialogSpec::new(
        2910,
        |app| app.session().is_some_and(|s| s.payments.gift_code.is_some()),
        QuillApp::build_gift_code_dialog,
    ),

    /// `setlanguage` link: the pack's details; nothing is switched.
    LanguageLink => DialogSpec::new(
        2920,
        |app| app.session().is_some_and(|s| s.settings.language_link.is_some()),
        QuillApp::build_language_link_dialog,
    ),
}

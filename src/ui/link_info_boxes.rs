//! Boxes for `t.me/giftcode/<code>` and `t.me/setlanguage/<id>` links
//! (`parity:deeplink-giftcode`, `parity:deeplink-premium-language`).
//!
//! The gift code box mirrors tdesktop's `GiftCodeBox`: who made the code,
//! who it is for, what it gives, why, and when. An unused code gets a
//! "Use Link" button; only that click sends `applyPremiumGiftCode`. The
//! language box is tdesktop's "Change language?" box: the pack, how much
//! of it is translated, and an Apply Language button that switches the
//! interface the same way Settings → Language does.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_base::{Disableable, StyledExt};
use gpui_kit::component::Sizable;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::deep_link_types::{gift_duration, gift_reason, language_progress};
use quill::lang::{keys, tr, tr_with};
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
            .unwrap_or_else(|| tr(keys::Q_REST_A_TELEGRAM_USER).into()),
        MessageSender::Chat { chat_id } => session
            .and_then(|s| s.chats.get(&chat_id))
            .map(|c| c.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| tr(keys::Q_REST_A_CHANNEL).into()),
    }
}

pub(super) fn info_row(label: impl Into<SharedString>, value: String) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .gap_4()
        .text_sm()
        .child(div().text_color(text_muted()).child(label.into()))
        .child(div().text_right().child(value))
}

pub(super) fn finish(
    dialog: Dialog,
    title: impl Into<SharedString>,
    body: AnyElement,
    footer: AnyElement,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Dialog {
    finish_tracked(None, dialog, title, body, footer, on_close)
}

/// `finish` with the body's scroll region tracked by `scroll`, for a long
/// list that loads its next page near the end (`ui/scroll_load.rs`).
pub(super) fn finish_tracked(
    scroll: Option<ScrollHandle>,
    dialog: Dialog,
    title: impl Into<SharedString>,
    body: AnyElement,
    footer: AnyElement,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Dialog {
    let body = Rc::new(RefCell::new(Some(body)));
    dialog
        .overlay(true)
        .title(crate::ui::shell::dialog_title(title))
        .content(crate::ui::shell::scrollable_dialog_content_tracked(
            scroll,
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

/// The "Open Gift Link" button under a Premium gift code card
/// (tdesktop `PremiumGift::button`, `ResolveGiftCode`). `None` for other
/// cards and for a code TDLib did not share.
pub(super) fn gift_code_open_button(
    row_id: u64,
    card: &quill::telegram::envelope::GiftCard,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    if card.kind != quill::telegram::envelope::GiftCardKind::GiftCode || card.gift_code.0.is_empty()
    {
        return None;
    }
    let code = card.gift_code.0.clone();
    Some(
        Button::new(("gift-code-open", row_id))
            .label(tr(keys::Q_REST_OPEN_GIFT_LINK))
            .small()
            .ghost()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_gift_code_link(code.clone(), cx);
            }))
            .into_any_element(),
    )
}

impl QuillApp {
    fn link_info_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// `giftcode` link: open the box and check the code. Nothing is applied.
    pub(super) fn open_gift_code_link(&mut self, code: String, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.request_gift_code_info(&code).is_err() {
                    self.connection
                        .set_note(StatusNote::error(tr(keys::Q_REST_COULD_NOT_CHECK_THE_GIFT)));
                }
            }
            None => {
                self.connection
                    .set_note(StatusNote::info(tr(keys::Q_REST_GIFT_CODES_NEED_A_SIGNED)));
            }
        }
        cx.notify();
    }

    /// `setlanguage` link: look the pack up and show it.
    pub(super) fn open_language_pack_link(&mut self, id: String, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.request_language_pack_info(&id).is_err() {
                    self.connection
                        .set_note(StatusNote::error(tr(keys::Q_REST_COULD_NOT_LOOK_UP_THE)));
                }
            }
            None => {
                self.connection.set_note(StatusNote::info(tr(
                    keys::Q_REST_LANGUAGE_LINKS_NEED_A_SIGNED,
                )));
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
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_REST_COULD_NOT_USE_THE_GIFT)));
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
                        .child(tr(keys::Q_REST_CHECKING_THE_GIFT_CODE)),
                )
                .into_any_element();
        }
        let Some(info) = lookup.info.as_ref() else {
            let reason = lookup
                .error
                .clone()
                .unwrap_or_else(|| tr(keys::Q_REST_THIS_GIFT_CODE_LINK_HAS).into());
            return body
                .child(div().id("gift-code-error").text_sm().child(reason))
                .into_any_element();
        };
        let used = info.is_used();
        body = body.child(div().text_sm().text_color(text_muted()).child(if used {
            tr(keys::Q_REST_THIS_LINK_WAS_USED_TO)
        } else {
            tr(keys::Q_REST_THIS_LINK_ACTIVATES_A_TELEGRAM)
        }));
        if let Some(creator) = info.creator {
            body = body.child(info_row(
                tr(keys::GIFT_LINK_LABEL_FROM),
                sender_name(session, creator),
            ));
            body = body.child(info_row(
                tr(keys::GIFT_LINK_LABEL_TO),
                if info.user_id > 0 {
                    sender_name(
                        session,
                        MessageSender::User {
                            user_id: info.user_id,
                        },
                    )
                } else {
                    tr(keys::GIFT_LINK_LABEL_TO_UNCLAIMED).into()
                },
            ));
        }
        body = body.child(info_row(
            tr(keys::GIFT_LINK_LABEL_GIFT),
            tr_with(
                keys::Q_REST_TELEGRAM_PREMIUM_FOR,
                &[(
                    "day_count",
                    &(gift_duration(info.month_count, info.day_count)).to_string(),
                )],
            ),
        ));
        if info.creator.is_some() {
            body = body.child(info_row(
                tr(keys::GIFT_LINK_LABEL_REASON),
                gift_reason(info.is_from_giveaway, info.user_id > 0 || used).into(),
            ));
        }
        if info.creation_date > 0 {
            body = body.child(info_row(
                tr(keys::GIFT_LINK_LABEL_DATE),
                quill::local_time::full_stamp(&quill::local_time::civil_local(info.creation_date)),
            ));
        }
        if used {
            body = body.child(
                div().id("gift-code-used").text_sm().child(tr_with(
                    keys::GIFT_LINK_USED_FOOTER,
                    &[(
                        "date",
                        &(quill::local_time::full_stamp(&quill::local_time::civil_local(
                            info.use_date,
                        )))
                        .to_string(),
                    )],
                )),
            );
        }
        if lookup.applied {
            body = body.child(
                div()
                    .id("gift-code-applied")
                    .text_sm()
                    .child(tr(keys::Q_REST_TELEGRAM_PREMIUM_IS_NOW_ACTIVE)),
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
                    .title(crate::ui::shell::dialog_title(tr(keys::GIFT_LINK_TITLE)))
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
                        .label(if lookup.applied {
                            tr(keys::BOX_DONE)
                        } else {
                            tr(keys::CLOSE)
                        })
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_gift_code_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::GiftCodeLink, window, cx);
                        })),
                )
                .when(can_apply, |footer| {
                    footer.child(
                        Button::new("gift-code-apply")
                            .label(tr(keys::GIFT_LINK_USE))
                            .loading(applying)
                            .disabled(applying)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.apply_gift_code_clicked(cx)),
                            ),
                    )
                })
                .into_any_element();
            finish(dialog, tr(keys::GIFT_LINK_TITLE), body, footer, on_close)
        })
    }

    fn language_body(lookup: &LanguageLinkLookup, current: &str) -> AnyElement {
        let body = div().flex().flex_col().gap_2();
        if lookup.loading {
            return body
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_LANGUAGE_LINK_LOOKING_UP)),
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
                            .map(SharedString::from)
                            .unwrap_or_else(|| tr(keys::LANGUAGE_NOT_FOUND).into()),
                    ),
                )
                .into_any_element();
        };
        let title = if info.native_name.is_empty() {
            info.name.clone()
        } else {
            info.native_name.clone()
        };
        let mut body = body.child(div().text_lg().font_semibold().child(title.clone()));
        if !info.name.is_empty() && info.name != info.native_name {
            body = body.child(info_row(tr(keys::LANGUAGES), info.name.clone()));
        }
        if let Some(progress) =
            language_progress(info.total_string_count, info.translated_string_count)
        {
            body = body.child(info_row(tr(keys::Q_LANGUAGE_LINK_PROGRESS), progress));
        }
        if info.is_beta {
            body = body.child(info_row(
                tr(keys::Q_LANGUAGE_LINK_STATUS),
                tr(keys::Q_LANGUAGE_LINK_BETA).to_string(),
            ));
        }
        let note = if info.id.eq_ignore_ascii_case(current) {
            tr(keys::Q_LANGUAGE_LINK_ALREADY).to_string()
        } else {
            tr_with(keys::Q_LANGUAGE_LINK_ABOUT, &[("lang_name", &title)])
        };
        body.child(div().text_sm().text_color(text_muted()).child(note))
            .into_any_element()
    }

    /// Apply Language: switch to the linked pack and close the box.
    fn apply_language_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let info = self
            .session()
            .and_then(|s| s.settings.language_link.as_ref())
            .and_then(|lookup| lookup.info.clone());
        self.close_language_box(cx);
        self.close_kit_dialog_if_done(DialogKind::LanguageLink, window, cx);
        if let Some(info) = info {
            self.choose_language(info, cx);
        }
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
                    .title(crate::ui::shell::dialog_title(tr(
                        keys::LANGUAGE_SWITCH_TITLE,
                    )))
                    .on_close(on_close.clone());
            };
            let current = quill::lang::current_pack_id();
            let body = Self::language_body(&lookup, &current);
            let can_apply = lookup
                .info
                .as_ref()
                .is_some_and(|info| !info.id.eq_ignore_ascii_case(&current));
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("language-link-close")
                        .label(if can_apply {
                            tr(keys::CANCEL)
                        } else {
                            tr(keys::CLOSE)
                        })
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_language_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::LanguageLink, window, cx);
                        })),
                )
                .when(can_apply, |footer| {
                    footer.child(
                        Button::new("language-link-apply")
                            .label(tr(keys::LANGUAGE_SWITCH_APPLY))
                            .primary()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_language_link(window, cx);
                            })),
                    )
                })
                .into_any_element();
            finish(
                dialog,
                tr(keys::LANGUAGE_SWITCH_TITLE),
                body,
                footer,
                on_close,
            )
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

    /// `setlanguage` link: the pack's details and Apply Language.
    LanguageLink => DialogSpec::new(
        2920,
        |app| app.session().is_some_and(|s| s.settings.language_link.is_some()),
        QuillApp::build_language_link_dialog,
    ),
}

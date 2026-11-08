//! The box over a collectible username or phone number (tdesktop
//! `ui/boxes/collectible_info_box.cpp`): who owns it, when it was bought on
//! Fragment and for how much, "Learn More" to Fragment and a copy button.
//!
//! A tap on the username or phone row of a profile asks TDLib
//! `getCollectibleItemInfo`; the box opens when the answer arrives. A
//! username that turns out not to be a collectible one is copied as a link
//! instead (tdesktop `ShowCollectibleUsername`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::earn::ton_icon;
use super::message_text::format_unix_date_time;
use super::shell::{DialogKind, QuillShell};
use crate::ui::os_open::open_link_in;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::collectible_info::{
    CollectibleKind, CollectibleLookup, copy_label, crypto_text, fiat_text, format_entity,
    info_prefix, title_parts,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use std::cell::RefCell;
use std::rc::Rc;

/// The error text `COLLECTIBLE_NOT_FOUND` leaves in a failed lookup.
const NOT_FOUND: &str = "COLLECTIBLE_NOT_FOUND";

impl QuillApp {
    /// Ask for a tapped collectible item; the box opens with the answer.
    pub(in crate::ui) fn open_collectible_info(
        &mut self,
        entity: &str,
        owner_name: &str,
        owner_chat: Option<ChatId>,
        copy_text: String,
        cx: &mut Context<Self>,
    ) {
        let lookup = CollectibleLookup::new(entity, owner_name, owner_chat, copy_text);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.request_collectible_info(lookup);
        }
        cx.notify();
    }

    /// A lookup that failed: a username that is not a collectible one is
    /// copied as a link, everything else stays silent.
    pub(super) fn pump_collectible_info(&mut self, cx: &mut Context<Self>) {
        let failed = self
            .session()
            .and_then(|s| s.payments.hub.collectible.as_ref())
            .is_some_and(|lookup| lookup.failed.is_some());
        if !failed {
            return;
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let Some(lookup) = live.driver.session.payments.hub.collectible.take() else {
            return;
        };
        if lookup.kind == CollectibleKind::Username
            && lookup.failed.as_deref() == Some(NOT_FOUND)
            && !lookup.copy_text.is_empty()
        {
            self.copy_profile_text(&lookup.copy_text, tr(keys::STICKERS_COPIED), cx);
        }
        cx.notify();
    }

    fn close_collectible_info(&mut self, cx: &mut Context<Self>) {
        if let Some(hub) = self.session_hub_mut() {
            hub.collectible = None;
        }
        cx.notify();
    }

    fn session_hub_mut(&mut self) -> Option<&mut quill::premium_hub::PremiumHub> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.payments.hub)
        } else {
            self.demo_session.as_mut().map(|s| &mut s.payments.hub)
        }
    }

    /// The copy button and the header: tdesktop's `copyCallback`.
    fn copy_collectible(&mut self, lookup: &CollectibleLookup, link: bool, cx: &mut Context<Self>) {
        let formatted = format_entity(lookup.kind, &lookup.entity);
        let text = if lookup.copy_text.is_empty() || !link {
            formatted
        } else {
            lookup.copy_text.clone()
        };
        let toast = match (lookup.kind, link) {
            (CollectibleKind::Phone, _) => tr(keys::COLLECTIBLE_PHONE_COPIED),
            (CollectibleKind::Username, true) => tr(keys::STICKERS_COPIED),
            (CollectibleKind::Username, false) => tr(keys::USERNAME_TEXT_COPIED),
        };
        self.copy_profile_text(&text, toast, cx);
    }

    pub(super) fn build_collectible_info_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CollectibleInfo, |this, _, cx| {
                this.close_collectible_info(cx);
            });
        app.update(cx, |this, cx| {
            let Some(lookup) = this
                .session()
                .and_then(|s| s.payments.hub.collectible.clone())
            else {
                return dialog.on_close(on_close);
            };
            let Some(info) = lookup.info.clone() else {
                return dialog.on_close(on_close);
            };
            let formatted = format_entity(lookup.kind, &lookup.entity);
            let photo = lookup.owner_chat.and_then(|chat| {
                let roots = this.media_display_roots();
                this.session()
                    .and_then(|s| s.chat_photo_path(chat))
                    .and_then(|path| quill::local_path::sandboxed_display_path(path, &roots))
            });

            // "{entity} is a collectible … that belongs to": the entity
            // copies itself on a click.
            let (before, after) = title_parts(lookup.kind);
            let header_text = format!("{before}{formatted}{after}");
            let range = before.len()..before.len() + formatted.len();
            let header_lookup = lookup.clone();
            let owner = cx.entity().downgrade();
            let header = InteractiveText::new(
                "collectible-header",
                StyledText::new(header_text).with_highlights(vec![(
                    range.clone(),
                    HighlightStyle {
                        color: Some(cx.theme().primary),
                        ..Default::default()
                    },
                )]),
            )
            .on_click(vec![range], move |_, _, cx| {
                let lookup = header_lookup.clone();
                let _ = owner.update(cx, |this, cx| this.copy_collectible(&lookup, false, cx));
            });

            let pill = div()
                .flex()
                .items_center()
                .gap_2()
                .pl_1()
                .pr_3()
                .py_1()
                .rounded_full()
                .bg(cx.theme().muted)
                .child(chat_avatar(&lookup.owner_name, photo.as_deref(), 24.))
                .child(
                    div()
                        .text_sm()
                        .max_w(px(220.))
                        .truncate()
                        .child(lookup.owner_name.clone()),
                );

            // "This username was bought on Fragment on {date} for {price}".
            let prefix = info_prefix(lookup.kind);
            let date = format_unix_date_time(info.purchase_date);
            let sentence = tr_with(
                keys::Q_MONEY_FRAGMENT_ON_FOR,
                &[("prefix", prefix), ("date", &date.to_string())],
            );
            let bold = HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            };
            let fragment_range = prefix.len()
                ..prefix.len() + tr(keys::INFO_MOBILE_CONTEXT_MENU_FRAGMENT_ABOUT_LINK).len();
            let sentence = StyledText::new(sentence).with_highlights(vec![(fragment_range, bold)]);
            let price_color = cx.theme().foreground;
            let mut price = div().flex().items_center().justify_center().gap_1();
            if info.crypto_currency == "TON" {
                price = price.child(ton_icon(16., price_color));
            }
            price = price
                .child(div().font_semibold().child(crypto_text(&info)))
                .child(format!("({})", fiat_text(&info)));
            let info_block = div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .text_sm()
                .text_center()
                .child(sentence)
                .child(price);

            let learn_url = info.url.clone();
            let copy_lookup = lookup.clone();
            let body = div()
                .id("collectible-info")
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .child(div().text_sm().text_center().child(header))
                .child(pill)
                .child(info_block)
                .child(
                    Button::new("collectible-learn-more")
                        .label(tr(keys::COLLECTIBLE_LEARN_MORE))
                        .ghost()
                        .w_full()
                        .on_click({
                            let app = app.downgrade();
                            move |_, _, cx| open_link_in(&app, &learn_url, cx)
                        }),
                )
                .child(
                    Button::new("collectible-copy")
                        .label(copy_label(lookup.kind))
                        .primary()
                        .w_full()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.copy_collectible(&copy_lookup, true, cx);
                            this.close_collectible_info(cx);
                            this.close_kit_dialog_if_done(DialogKind::CollectibleInfo, window, cx);
                        })),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(formatted))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close)
        })
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "collectible_info_ui_tests.rs"]
mod tests;

crate::ui::shell::register_dialogs! {
    /// A collectible username or phone number bought on Fragment.
    CollectibleInfo => DialogSpec::new(
        4025,
        |app| {
            app.session().is_some_and(|s| {
                s.payments
                    .hub
                    .collectible
                    .as_ref()
                    .is_some_and(|lookup| lookup.info.is_some())
            })
        },
        QuillApp::build_collectible_info_dialog,
    ),
}

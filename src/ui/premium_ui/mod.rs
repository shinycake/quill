//! Stars balance and history, received gifts, the Premium explainer and the
//! gift/giveaway chat cards (tdesktop `settings_credits*`,
//! `info/peer_gifts/*`, `settings_premium*`, `history_view_service_box`).
//!
//! Showing, hiding and converting the user's own gifts are confirmed in the
//! dialog; pin, upgrade, transfer, resale and collections are in
//! `ui/gift_manage_ui.rs`. Buying Stars is not offered here; subscribing to
//! Premium follows the app config (`purchase.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::{fill_muted, success};
use super::message_text::format_unix_date_time;
use super::scroll_load::{loading_row, watch};
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::premium_hub::{PremiumHub, ReceivedGift, StarTx, TxFilter, group_digits};
use quill::state::Session;
use quill::telegram::envelope::{
    GiftCard, GiftCardKind, MessageSender, ServiceAction, StickerContent,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const GRID_TILE: f32 = 132.0;

/// A gift/giveaway sticker (or a fallback glyph until it is downloaded).
pub(in crate::ui) fn sticker_tile(
    id: (&'static str, u64),
    sticker: Option<&StickerContent>,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    side: f32,
    fallback: &'static str,
) -> AnyElement {
    let frame = div()
        .id(id)
        .size(px(side))
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    let path = sticker.and_then(|s| {
        [s.display_file_id(), s.thumb_file_id]
            .into_iter()
            .flatten()
            .filter_map(|id| files.get(&id.0).and_then(|f| f.usable_path()))
            .find_map(|p| sandboxed_display_path(p, media_roots))
    });
    match path {
        Some(path) => frame
            .child(
                img(super::image_budget::sized_media(
                    &path,
                    (px(side), px(side)),
                    sticker.map(|s| (s.width, s.height)),
                    super::image_budget::Fit::Contain,
                ))
                .size(px(side))
                .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
        None => frame
            .text_size(px(side * 0.45))
            .child(fallback)
            .into_any_element(),
    }
}

fn sender_name(session: &Session, sender: Option<MessageSender>) -> String {
    match sender {
        Some(sender) => session.sender_label(sender),
        None => tr(keys::Q_MONEY_UNKNOWN).to_string(),
    }
}

fn fact_row(label: &str, value: String, cx: &App) -> Div {
    div()
        .flex()
        .justify_between()
        .gap_3()
        .text_sm()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().font_medium().text_right().child(value))
}

impl QuillApp {
    fn hub_mut(&mut self) -> Option<&mut PremiumHub> {
        if let Some(live) = self.live.as_mut() {
            Some(&mut live.driver.session.payments.hub)
        } else {
            self.demo_session.as_mut().map(|s| &mut s.payments.hub)
        }
    }

    // ---- Stars ----------------------------------------------------------

    pub(super) fn open_stars(&mut self, cx: &mut Context<Self>) {
        if quill::agent::ui_driver::guard::refuse_hard_if_injecting(
            quill::agent::ui_driver::guard::HardSurface::Payments,
        ) {
            return;
        }
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.payments.hub.stars_open = true;
            // "View Statistics" and "Earn Stars from Mini Apps" gates.
            let _ = live.driver.probe_self_star_stats();
            let _ = live.driver.ensure_earn_app_config();
            live.driver.maybe_fetch_star_transactions()
        });
        if let Some(Err(_)) = sent {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULD_NOT_LOAD_STARS)));
        }
        cx.notify();
    }

    pub(super) fn close_stars(&mut self, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.stars_open = false;
            hub.tx_selected = None;
        }
        cx.notify();
    }

    fn set_tx_filter(&mut self, filter: TxFilter, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.set_star_transactions_filter(filter);
        } else if let Some(hub) = self.hub_mut() {
            hub.filter = filter;
        }
        cx.notify();
    }

    fn select_tx(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.tx_selected = id;
        }
        cx.notify();
    }

    pub(super) fn build_stars_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(app, shell, DialogKind::Stars, |this, _, cx| {
            this.close_stars(cx);
        });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::Q_MONEY_STARS)));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let hub = &session.payments.hub;
            let muted = cx.theme().muted_foreground;
            let mut body = div().flex().flex_col().gap_3();
            if let Some(tx) = hub.selected_tx() {
                body = body.child(tx_details(session, tx, cx));
            } else {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .py_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(tr(keys::Q_MONEY_YOUR_BALANCE)),
                        )
                        .child(div().text_size(px(30.)).font_semibold().child(format!(
                            "\u{2B50} {}",
                            hub.balance.map_or("\u{2013}".to_string(), |b| b.label())
                        ))),
                );
                let mut tabs = div().flex().gap_1();
                for filter in TxFilter::ALL {
                    let active = hub.filter == filter;
                    let button = Button::new(("stars-filter", filter as u64))
                        .label(filter.label())
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_tx_filter(filter, cx);
                        }));
                    tabs = tabs.child(if active {
                        button.primary()
                    } else {
                        button.ghost()
                    });
                }
                body = body.child(tabs);
                if let Some(err) = hub.tx_error.as_ref() {
                    body = body.child(div().text_sm().child(err.clone()));
                }
                if hub.tx_loading && hub.transactions.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_LOADING_TRANSACTIONS)),
                    );
                } else if hub.transactions.is_empty() && hub.tx_loaded {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_NO_TRANSACTIONS_YET)),
                    );
                }
                for (ix, tx) in hub.transactions.iter().enumerate() {
                    body = body.child(tx_row(session, ix, tx, cx));
                }
                if !hub.tx_offset.is_empty() && hub.tx_loading {
                    body = body.child(loading_row("stars-loading", cx));
                }
                body = body.child(star_packs::star_packs_section(session, cx));
                // tdesktop `BuildCreditsButtons`: stats, gift, earn.
                body = body.children(this.stars_stats_button(cx));
                body = body.child(super::stars_gift_ui::gift_stars_button(cx));
                body = body.children(this.stars_earn_button(cx));
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_YOU_PAY_FOR_STARS_IN)),
                );
            }
            let scroll = this.payments.stars_scroll.clone();
            let body = watch(body, cx, Self::stars_more);
            dialog
                .content(crate::ui::shell::scrollable_dialog_content_tracked(
                    Some(scroll),
                    {
                        let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                        move |content, _, _| {
                            let body = body
                                .borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element());
                            content.child(body)
                        }
                    },
                ))
                .on_close(on_close)
        })
    }

    // ---- Gifts ----------------------------------------------------------

    /// Open the received gifts of `owner` (a user or a channel).
    pub(super) fn open_gifts(&mut self, owner: MessageSender, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.open_received_gifts(owner));
        if let Some(Err(_)) = sent {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MONEY_COULD_NOT_LOAD_GIFTS)));
        }
        cx.notify();
    }

    /// "My gifts" from Settings.
    pub(super) fn open_my_gifts(&mut self, cx: &mut Context<Self>) {
        if let Some(me) = self.session().and_then(|s| s.my_user_id) {
            self.open_gifts(MessageSender::User { user_id: me }, cx);
        }
    }

    pub(super) fn close_gifts(&mut self, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.gifts_open = false;
            hub.gift_selected = None;
            hub.gift_convert_confirm = None;
        }
        cx.notify();
    }

    fn select_gift(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.gift_selected = id;
            hub.gift_convert_confirm = None;
        }
        cx.notify();
    }

    fn ask_convert_gift(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.gift_convert_confirm = id;
        }
        cx.notify();
    }

    fn toggle_gift(&mut self, id: String, saved: bool, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.toggle_gift_saved(&id, saved));
        if let Some(Err(_)) = sent {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_UPDATE_THE_GIFT,
            )));
        }
        cx.notify();
    }

    fn convert_gift(&mut self, id: String, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.convert_gift_to_stars(&id));
        if let Some(Err(_)) = sent {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_MONEY_COULD_NOT_CONVERT_THE_GIFT,
            )));
        }
        if let Some(hub) = self.hub_mut() {
            hub.gift_convert_confirm = None;
        }
        cx.notify();
    }

    pub(super) fn build_gifts_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ReceivedGifts, |this, _, cx| {
                this.close_gifts(cx);
            });
        app.update(cx, |this, cx| {
            let roots = this.media_display_roots();
            let Some(session) = this.session() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let hub = &session.payments.hub;
            let mine = hub.gifts_are_mine(session.my_user_id);
            let title = if mine {
                tr(keys::Q_MONEY_MY_GIFTS).to_string()
            } else {
                match hub.gifts_owner {
                    Some(owner) => tr_with(
                        keys::Q_MONEY_GIFTS_OF,
                        &[("sender_label", &(session.sender_label(owner)).to_string())],
                    ),
                    None => tr(keys::Q_MONEY_GIFTS).to_string(),
                }
            };
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title));
            let muted = cx.theme().muted_foreground;
            let mut body = div().flex().flex_col().gap_3();
            if let Some(note) = session.payments.gifts_manage.note.as_ref() {
                body = body.child(div().text_sm().child(note.clone()));
            }
            if let Some(gift) = hub.selected_gift() {
                let owner_actions =
                    (mine || gift.can_delete(session.gifts_deletable())).then(|| {
                        let typed = this.payments.gift_price_input.read(cx).value().to_string();
                        super::gift_owner_actions::gift_owner_actions(
                            session,
                            gift,
                            mine,
                            &this.payments.gift_price_input,
                            &typed,
                            cx,
                        )
                    });
                body = body.child(gift_details(session, gift, mine, owner_actions, &roots, cx));
            } else {
                body = body.child(super::gift_manage_ui::gift_collection_tabs(
                    session,
                    mine,
                    &this.payments.gift_collection_input,
                    cx,
                ));
                if let Some(err) = hub.gifts_error.as_ref() {
                    body = body.child(div().text_sm().child(err.clone()));
                }
                if hub.gifts_loading && hub.gifts.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_LOADING_GIFTS)),
                    );
                } else if hub.gifts.is_empty() && hub.gifts_loaded {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(keys::Q_MONEY_NO_GIFTS_YET)),
                    );
                }
                if hub.gifts_total > 0 {
                    body = body.child(div().text_xs().text_color(muted).child(format!(
                        "{} gift{}",
                        hub.gifts_total,
                        if hub.gifts_total == 1 { "" } else { "s" }
                    )));
                }
                let mut grid = div().flex().flex_wrap().gap_2();
                for (ix, gift) in hub.gifts.iter().enumerate() {
                    grid = grid.child(gift_tile(session, ix, gift, mine, &roots, cx));
                }
                body = body.child(grid);
                if !hub.gifts_offset.is_empty() && hub.gifts_loading {
                    body = body.child(loading_row("gifts-loading", cx));
                }
            }
            let scroll = this.payments.gifts_scroll.clone();
            let body = watch(body, cx, Self::gifts_more);
            dialog
                .content(crate::ui::shell::scrollable_dialog_content_tracked(
                    Some(scroll),
                    {
                        let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                        move |content, _, _| {
                            let body = body
                                .borrow_mut()
                                .take()
                                .unwrap_or_else(|| div().into_any_element());
                            content.child(body)
                        }
                    },
                ))
                .on_close(on_close)
        })
    }

    // ---- Premium --------------------------------------------------------
}

fn tx_row(session: &Session, ix: usize, tx: &StarTx, cx: &mut Context<QuillApp>) -> AnyElement {
    let id = tx.id.clone();
    star_tx_row(
        session,
        ("stars-tx", ix as u64),
        tx,
        move |this, cx| this.select_tx(Some(id.clone()), cx),
        cx,
    )
}

/// One Stars history row; `on_click` opens its receipt.
pub(in crate::ui) fn star_tx_row(
    session: &Session,
    id: impl Into<ElementId>,
    tx: &StarTx,
    on_click: impl Fn(&mut QuillApp, &mut Context<QuillApp>) + 'static,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut sub = Vec::new();
    if let Some(peer) = tx.peer {
        sub.push(session.sender_label(peer));
    }
    if !tx.detail.is_empty() {
        sub.push(tx.detail.clone());
    }
    sub.push(format_unix_date_time(tx.date as i64));
    if tx.is_refund {
        sub.push(tr(keys::Q_MONEY_REFUND).to_string());
    }
    div()
        .id(id)
        .flex()
        .justify_between()
        .items_center()
        .gap_3()
        .py_1p5()
        .cursor_pointer()
        .role(gpui_kit::Role::Button)
        .aria_label(tr_with(
            keys::Q_MONEY_STARS_2,
            &[
                ("title", &tx.title.to_string()),
                ("signed_label", &(tx.amount.signed_label()).to_string()),
            ],
        ))
        .tab_index(0)
        .on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .min_w_0()
                .child(div().text_sm().font_semibold().child(tx.title.clone()))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(sub.join(" \u{00B7} ")),
                ),
        )
        .child(
            div()
                .flex_none()
                .font_semibold()
                .text_color(if tx.is_incoming() {
                    success().into()
                } else {
                    cx.theme().foreground
                })
                .child(format!("{} \u{2B50}", tx.amount.signed_label())),
        )
        .into_any_element()
}

fn tx_details(session: &Session, tx: &StarTx, cx: &mut Context<QuillApp>) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            Button::new("stars-back")
                .label(tr(keys::Q_MONEY_BACK_2))
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.select_tx(None, cx))),
        )
        .child(star_tx_facts(session, tx, cx))
        .into_any_element()
}

/// A Stars transaction's receipt: the amount, what it was and its facts.
pub(in crate::ui) fn star_tx_facts(
    session: &Session,
    tx: &StarTx,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let id = tx.id.clone();
    let mut col = div().flex().flex_col().gap_2();
    col = col.child(
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .py_2()
            .child(
                div()
                    .text_size(px(28.))
                    .font_semibold()
                    .text_color(if tx.is_incoming() {
                        success().into()
                    } else {
                        cx.theme().foreground
                    })
                    .child(format!("{} \u{2B50}", tx.amount.signed_label())),
            )
            .child(div().font_semibold().child(tx.title.clone())),
    );
    if !tx.detail.is_empty() {
        col = col.child(fact_row(tr(keys::Q_MONEY_DETAILS), tx.detail.clone(), cx));
    }
    if let Some(peer) = tx.peer {
        col = col.child(fact_row(
            tr(keys::Q_MONEY_WITH),
            session.sender_label(peer),
            cx,
        ));
    }
    col = col.child(fact_row(
        tr(keys::GIFT_RESALE_DATE),
        format_unix_date_time(tx.date as i64),
        cx,
    ));
    if tx.is_refund {
        col = col.child(fact_row(
            tr(keys::Q_MONEY_STATUS),
            tr(keys::Q_MONEY_REFUNDED).to_string(),
            cx,
        ));
    }
    col.child(fact_row(tr(keys::Q_MONEY_TRANSACTION_ID), id.clone(), cx))
        .child(
            Button::new("stars-copy-id")
                .label(tr(keys::Q_MONEY_COPY_TRANSACTION_ID))
                .ghost()
                .small()
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(id.clone()));
                })),
        )
        .into_any_element()
}

fn gift_tile(
    session: &Session,
    ix: usize,
    gift: &ReceivedGift,
    mine: bool,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let id = gift.id.clone();
    let bg = match gift.gift.backdrop {
        Some((center, edge)) => div().bg(linear_gradient(
            180.,
            linear_color_stop(rgb(center), 0.),
            linear_color_stop(rgb(edge), 1.),
        )),
        None => div().bg(fill_muted().opacity(0.35)),
    };
    bg.id(("gift-tile", ix as u64))
        .relative()
        .w(px(GRID_TILE))
        .h(px(GRID_TILE + 28.))
        .rounded_lg()
        .overflow_hidden()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .cursor_pointer()
        .role(gpui_kit::Role::Button)
        .aria_label(tr_with(
            keys::Q_MONEY_GIFT,
            &[("headline", &(gift.headline()).to_string())],
        ))
        .tab_index(0)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_gift(Some(id.clone()), cx);
        }))
        .when(gift.is_pinned, |t| {
            t.child(
                div()
                    .absolute()
                    .top_1()
                    .left_1()
                    .text_xs()
                    .child("\u{1F4CC}"),
            )
        })
        .when(mine && !gift.is_saved, |t| {
            t.child(
                div()
                    .absolute()
                    .top_1()
                    .right_1()
                    .px_1()
                    .rounded_full()
                    .bg(gpui_kit::black().opacity(0.5))
                    .text_color(gpui_kit::white())
                    .text_xs()
                    .child(tr(keys::Q_MONEY_HIDDEN)),
            )
        })
        .child(sticker_tile(
            ("gift-tile-sticker", ix as u64),
            gift.gift.sticker.as_ref(),
            &session.media.files,
            roots,
            84.,
            "\u{1F381}",
        ))
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(if gift.gift.backdrop.is_some() {
                    gpui_kit::white()
                } else {
                    cx.theme().foreground
                })
                .child(gift.headline()),
        )
        .into_any_element()
}

fn gift_details(
    session: &Session,
    gift: &ReceivedGift,
    mine: bool,
    owner_actions: Option<AnyElement>,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let hub = &session.payments.hub;
    let mut col = div().flex().flex_col().gap_2();
    col = col
        .child(
            Button::new("gift-back")
                .label(tr(keys::Q_MONEY_BACK_2))
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.select_gift(None, cx))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(sticker_tile(
                    ("gift-detail-sticker", 0),
                    gift.gift.sticker.as_ref(),
                    &session.media.files,
                    roots,
                    128.,
                    "\u{1F381}",
                ))
                .child(div().text_lg().font_semibold().child(gift.headline())),
        );
    let from = if gift.is_private && !mine {
        tr(keys::Q_MONEY_HIDDEN_SENDER).to_string()
    } else {
        sender_name(session, gift.sender)
    };
    col = col.child(fact_row(tr(keys::Q_MONEY_FROM), from, cx));
    col = col.child(fact_row(
        tr(keys::GIFT_RESALE_DATE),
        format_unix_date_time(gift.date as i64),
        cx,
    ));
    if !gift.gift.upgraded && gift.gift.star_count > 0 {
        col = col.child(fact_row(
            tr(keys::Q_MONEY_VALUE),
            tr_with(
                keys::Q_MONEY_STARS_3,
                &[(
                    "star_count",
                    &(group_digits(gift.gift.star_count)).to_string(),
                )],
            ),
            cx,
        ));
    }
    if let Some(value) = gift.gift.collectible.as_deref() {
        col = col.children(super::gift_value_ui::gift_value_row(session, value, cx));
    }
    if gift.was_refunded {
        col = col.child(fact_row(
            tr(keys::Q_MONEY_STATUS),
            tr(keys::Q_MONEY_REFUNDED).to_string(),
            cx,
        ));
    }
    if !gift.text.is_empty() {
        col = col.child(
            div()
                .p_2()
                .rounded_md()
                .bg(cx.theme().secondary)
                .text_sm()
                .child(gift.text.clone()),
        );
    }
    if mine {
        col = col.child(div().text_xs().text_color(muted).child(if gift.is_saved {
            tr(keys::Q_MONEY_SHOWN_ON_YOUR_PROFILE)
        } else {
            tr(keys::Q_MONEY_HIDDEN_FROM_YOUR_PROFILE)
        }));
        let id = gift.id.clone();
        let saved = gift.is_saved;
        col = col.child(
            Button::new("gift-toggle-saved")
                .label(if saved {
                    tr(keys::Q_MONEY_HIDE_FROM_MY_PROFILE)
                } else {
                    tr(keys::Q_MONEY_SHOW_ON_MY_PROFILE)
                })
                .ghost()
                .disabled(hub.gift_mutating)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_gift(id.clone(), !saved, cx);
                })),
        );
        if gift.can_convert() {
            let id = gift.id.clone();
            if hub.gift_convert_confirm.as_deref() == Some(gift.id.as_str()) {
                col = col.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().secondary)
                        .child(div().text_sm().child(tr_with(
                            keys::Q_LEFTOVERS_CONVERT_GIFT_SURE,
                            &[("count", &gift.sell_star_count.to_string())],
                        )))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new("gift-convert-yes")
                                        .label(tr(keys::Q_MONEY_CONVERT))
                                        .danger()
                                        .disabled(hub.gift_mutating)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.convert_gift(id.clone(), cx);
                                        })),
                                )
                                .child(
                                    Button::new("gift-convert-no")
                                        .label(tr(keys::Q_MONEY_KEEP_GIFT))
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.ask_convert_gift(None, cx);
                                        })),
                                ),
                        ),
                );
            } else {
                let id = gift.id.clone();
                col = col.child(
                    Button::new("gift-convert")
                        .label(tr_with(
                            keys::Q_MONEY_CONVERT_TO_STARS,
                            &[("sell_star_count", &gift.sell_star_count.to_string())],
                        ))
                        .ghost()
                        .disabled(hub.gift_mutating)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask_convert_gift(Some(id.clone()), cx);
                        })),
                );
            }
        }
    }
    if let Some(actions) = owner_actions {
        col = col.child(actions);
    }
    if let Some(err) = hub.gifts_error.as_ref() {
        col = col.child(div().text_sm().child(err.clone()));
    }
    col.into_any_element()
}

use crate::ui::status_note::StatusNote;
pub(super) use gift_card::{gift_card_element, gift_card_of};
mod gift_card;
mod grams;
mod grams_demo;
mod grams_withdraw;
mod limit_box;
mod limit_demo;
mod limit_notices;
mod open_premium;
mod paging;
mod preview;
mod preview_demo;
mod promo;
mod public_links;
mod purchase;
mod star_packs;

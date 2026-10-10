//! Stars balance and history, received gifts, the Premium explainer and the
//! gift/giveaway chat cards (tdesktop `settings_credits*`,
//! `info/peer_gifts/*`, `settings_premium*`, `history_view_service_box`).
//!
//! Read-only apart from showing/hiding and converting the user's own gifts
//! (both confirmed in the dialog). Buying Stars, sending gifts and
//! subscribing to Premium are never offered here.

use super::app::QuillApp;
use super::chat_theme::{fill_muted, success};
use super::message_text::format_unix_date_time;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::local_path::sandboxed_display_path;
use quill::premium_hub::{PREMIUM_BOT_URL, PremiumHub, ReceivedGift, StarTx, TxFilter};
use quill::state::Session;
use quill::telegram::envelope::{
    GiftCard, GiftCardKind, MessageSender, ParsedFile, ServiceAction, StickerContent,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const GRID_TILE: f32 = 132.0;

/// A gift/giveaway sticker (or a fallback glyph until it is downloaded).
fn sticker_tile(
    id: (&'static str, u64),
    sticker: Option<&StickerContent>,
    files: &HashMap<i32, ParsedFile>,
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
        None => "Unknown".to_string(),
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
            Some(&mut live.driver.session.hub)
        } else {
            self.demo_session.as_mut().map(|s| &mut s.hub)
        }
    }

    // ---- Stars ----------------------------------------------------------

    pub(super) fn open_stars(&mut self, cx: &mut Context<Self>) {
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.hub.stars_open = true;
            live.driver.maybe_fetch_star_transactions()
        });
        if let Some(Err(_)) = sent {
            self.status_note = "could not load Stars".into();
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
                .title(crate::ui::shell::dialog_title("Stars"));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let hub = &session.hub;
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
                        .child(div().text_xs().text_color(muted).child("Your balance"))
                        .child(
                            div()
                                .text_size(px(30.))
                                .font_semibold()
                                .child(format!(
                                    "\u{2B50} {}",
                                    hub.balance.map_or("\u{2013}".to_string(), |b| b.label())
                                )),
                        ),
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
                    tabs = tabs.child(if active { button.primary() } else { button.ghost() });
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
                            .child("Loading transactions\u{2026}"),
                    );
                } else if hub.transactions.is_empty() && hub.tx_loaded {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("No transactions yet."),
                    );
                }
                for (ix, tx) in hub.transactions.iter().enumerate() {
                    body = body.child(tx_row(session, ix, tx, cx));
                }
                if !hub.tx_offset.is_empty() {
                    body = body.child(
                        Button::new("stars-load-more")
                            .label(if hub.tx_loading { "Loading\u{2026}" } else { "Load more" })
                            .ghost()
                            .disabled(hub.tx_loading)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.fetch_more_star_transactions();
                                }
                                cx.notify();
                            })),
                    );
                }
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Buying Stars and withdrawals happen in the Telegram app or @PremiumBot. Quill only shows your balance and history."),
                );
            }
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
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
            self.status_note = "could not load gifts".into();
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
            self.status_note = "could not update the gift".into();
        }
        cx.notify();
    }

    fn convert_gift(&mut self, id: String, cx: &mut Context<Self>) {
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.convert_gift_to_stars(&id));
        if let Some(Err(_)) = sent {
            self.status_note = "could not convert the gift".into();
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
            let hub = &session.hub;
            let mine = hub.gifts_are_mine(session.my_user_id);
            let title = if mine {
                "My gifts".to_string()
            } else {
                match hub.gifts_owner {
                    Some(owner) => format!("Gifts of {}", session.sender_label(owner)),
                    None => "Gifts".to_string(),
                }
            };
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title));
            let muted = cx.theme().muted_foreground;
            let mut body = div().flex().flex_col().gap_3();
            if let Some(gift) = hub.selected_gift() {
                body = body.child(gift_details(session, gift, mine, &roots, cx));
            } else {
                if let Some(err) = hub.gifts_error.as_ref() {
                    body = body.child(div().text_sm().child(err.clone()));
                }
                if hub.gifts_loading && hub.gifts.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Loading gifts\u{2026}"),
                    );
                } else if hub.gifts.is_empty() && hub.gifts_loaded {
                    body = body.child(div().text_sm().text_color(muted).child("No gifts yet."));
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
                if !hub.gifts_offset.is_empty() {
                    body = body.child(
                        Button::new("gifts-load-more")
                            .label(if hub.gifts_loading {
                                "Loading\u{2026}"
                            } else {
                                "Load more"
                            })
                            .ghost()
                            .disabled(hub.gifts_loading)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.fetch_more_received_gifts();
                                }
                                cx.notify();
                            })),
                    );
                }
            }
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    // ---- Premium --------------------------------------------------------

    pub(super) fn open_premium(&mut self, cx: &mut Context<Self>) {
        let sent = self.live.as_mut().map(|live| {
            live.driver.session.hub.premium_open = true;
            live.driver.maybe_fetch_premium()
        });
        if let Some(Err(_)) = sent {
            self.status_note = "could not load Premium".into();
        }
        cx.notify();
    }

    pub(super) fn close_premium(&mut self, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.premium_open = false;
        }
        cx.notify();
    }

    pub(super) fn build_premium_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PremiumFeatures, |this, _, cx| {
                this.close_premium(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Telegram Premium"));
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let hub = &session.hub;
            let muted = cx.theme().muted_foreground;
            let subscribed =
                hub.premium_state.as_ref().is_some_and(|s| s.is_subscribed) || session.is_premium();
            let mut body = div().flex().flex_col().gap_3();
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .py_2()
                    .child(div().text_size(px(34.)).child("\u{2B50}"))
                    .child(div().text_lg().font_semibold().child(if subscribed {
                        "You have Telegram Premium"
                    } else {
                        "Telegram Premium"
                    }))
                    .child(
                        div().text_sm().text_color(muted).text_center().child(
                            hub.premium_state
                                .as_ref()
                                .map(|s| s.text.clone())
                                .filter(|t| !t.is_empty())
                                .unwrap_or_else(|| "Extra features and higher limits.".to_string()),
                        ),
                    ),
            );
            if !subscribed {
                body = body.child(
                    Button::new("premium-get")
                        .label("Get Premium")
                        .primary()
                        .on_click(cx.listener(|_, _, _, cx| cx.open_url(PREMIUM_BOT_URL))),
                );
                body = body.child(div().text_xs().text_color(muted).text_center().child(
                    "Subscriptions are managed through @PremiumBot. Quill never takes payment.",
                ));
            }
            if let Some(err) = hub.premium_error.as_ref() {
                body = body.child(div().text_sm().child(err.clone()));
            }
            if hub.premium_loading && hub.premium.is_none() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("Loading features\u{2026}"),
                );
            }
            if let Some(info) = hub.premium.as_ref() {
                for feature in &info.features {
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(div().text_sm().font_semibold().child(feature.title))
                            .child(div().text_xs().text_color(muted).child(feature.about)),
                    );
                }
                if !info.limits.is_empty() {
                    body = body.child(div().pt_2().font_semibold().child("Limits"));
                    for limit in &info.limits {
                        body = body.child(fact_row(
                            limit.title,
                            format!("{} \u{2192} {}", limit.default_value, limit.premium_value),
                            cx,
                        ));
                    }
                }
            }
            dialog
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }
}

fn tx_row(session: &Session, ix: usize, tx: &StarTx, cx: &mut Context<QuillApp>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let id = tx.id.clone();
    let mut sub = Vec::new();
    if let Some(peer) = tx.peer {
        sub.push(session.sender_label(peer));
    }
    if !tx.detail.is_empty() {
        sub.push(tx.detail.clone());
    }
    sub.push(format_unix_date_time(tx.date as i64));
    if tx.is_refund {
        sub.push("Refund".to_string());
    }
    div()
        .id(("stars-tx", ix as u64))
        .flex()
        .justify_between()
        .items_center()
        .gap_3()
        .py_1p5()
        .cursor_pointer()
        .role(gpui_kit::Role::Button)
        .aria_label(format!("{}, {} Stars", tx.title, tx.amount.signed_label()))
        .tab_index(0)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.select_tx(Some(id.clone()), cx);
        }))
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
    let id = tx.id.clone();
    let mut col = div().flex().flex_col().gap_2();
    col = col
        .child(
            Button::new("stars-back")
                .label("\u{2039} Back")
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.select_tx(None, cx))),
        )
        .child(
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
        col = col.child(fact_row("Details", tx.detail.clone(), cx));
    }
    if let Some(peer) = tx.peer {
        col = col.child(fact_row("With", session.sender_label(peer), cx));
    }
    col = col.child(fact_row("Date", format_unix_date_time(tx.date as i64), cx));
    if tx.is_refund {
        col = col.child(fact_row("Status", "Refunded".to_string(), cx));
    }
    col.child(fact_row("Transaction ID", id.clone(), cx))
        .child(
            Button::new("stars-copy-id")
                .label("Copy transaction ID")
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
        .aria_label(format!("Gift {}", gift.headline()))
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
                    .child("Hidden"),
            )
        })
        .child(sticker_tile(
            ("gift-tile-sticker", ix as u64),
            gift.gift.sticker.as_ref(),
            &session.files,
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
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let hub = &session.hub;
    let mut col = div().flex().flex_col().gap_2();
    col = col
        .child(
            Button::new("gift-back")
                .label("\u{2039} Back")
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
                    &session.files,
                    roots,
                    128.,
                    "\u{1F381}",
                ))
                .child(div().text_lg().font_semibold().child(gift.headline())),
        );
    let from = if gift.is_private && !mine {
        "Hidden sender".to_string()
    } else {
        sender_name(session, gift.sender)
    };
    col = col.child(fact_row("From", from, cx));
    col = col.child(fact_row(
        "Date",
        format_unix_date_time(gift.date as i64),
        cx,
    ));
    if !gift.gift.upgraded && gift.gift.star_count > 0 {
        col = col.child(fact_row(
            "Value",
            format!(
                "{} Stars",
                quill::premium_hub::group_digits(gift.gift.star_count)
            ),
            cx,
        ));
    }
    if gift.was_refunded {
        col = col.child(fact_row("Status", "Refunded".to_string(), cx));
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
            "Shown on your profile."
        } else {
            "Hidden from your profile."
        }));
        let id = gift.id.clone();
        let saved = gift.is_saved;
        col = col.child(
            Button::new("gift-toggle-saved")
                .label(if saved {
                    "Hide from my profile"
                } else {
                    "Show on my profile"
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
                        .child(div().text_sm().child(format!(
                            "Convert this gift to {} Stars? The gift will be removed from your profile. This cannot be undone.",
                            gift.sell_star_count
                        )))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new("gift-convert-yes")
                                        .label("Convert")
                                        .danger()
                                        .disabled(hub.gift_mutating)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.convert_gift(id.clone(), cx);
                                        })),
                                )
                                .child(
                                    Button::new("gift-convert-no")
                                        .label("Keep gift")
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
                        .label(format!("Convert to {} Stars", gift.sell_star_count))
                        .ghost()
                        .disabled(hub.gift_mutating)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ask_convert_gift(Some(id.clone()), cx);
                        })),
                );
            }
        }
    }
    if let Some(err) = hub.gifts_error.as_ref() {
        col = col.child(div().text_sm().child(err.clone()));
    }
    col.into_any_element()
}

/// The inner action's (sender, receiver) of a gift card message.
fn card_parties(action: &ServiceAction) -> (Option<MessageSender>, Option<MessageSender>) {
    match action {
        ServiceAction::Gift {
            sender, receiver, ..
        }
        | ServiceAction::UpgradedGift {
            sender, receiver, ..
        } => (*sender, *receiver),
        _ => (None, None),
    }
}

/// The card of a gift/giveaway service message, if it has one.
pub(super) fn gift_card_of(
    content: &quill::telegram::envelope::MessageContent,
) -> Option<(&ServiceAction, &GiftCard)> {
    match content {
        quill::telegram::envelope::MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::WithCard { action, card } => Some((action.as_ref(), card.as_ref())),
            _ => None,
        },
        _ => None,
    }
}

/// The bubble card under a gift/giveaway pill (tdesktop service box).
pub(super) fn gift_card_element(
    row_id: u64,
    inner: &ServiceAction,
    card: &GiftCard,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let on_backdrop = card.backdrop.is_some();
    let fg = if on_backdrop {
        gpui_kit::white()
    } else {
        cx.theme().foreground
    };
    let sub_fg = if on_backdrop {
        gpui_kit::white().opacity(0.8)
    } else {
        muted
    };
    let fallback = match card.kind {
        GiftCardKind::Premium | GiftCardKind::GiftCode => "\u{2B50}",
        GiftCardKind::Stars => "\u{2B50}",
        GiftCardKind::Giveaway | GiftCardKind::GiveawayWinners => "\u{1F389}",
        _ => "\u{1F381}",
    };
    let me = session.and_then(|s| s.my_user_id);
    let (_, receiver) = card_parties(inner);
    let to_me = matches!(receiver, Some(MessageSender::User { user_id }) if Some(user_id) == me);
    let base = match card.backdrop {
        Some((center, edge)) => div().bg(linear_gradient(
            180.,
            linear_color_stop(rgb(center), 0.),
            linear_color_stop(rgb(edge), 1.),
        )),
        None => div().bg(cx.theme().secondary.opacity(0.85)),
    };
    base.id(("gift-card", row_id))
        .w(px(250.))
        .rounded_xl()
        .p_3()
        .flex()
        .flex_col()
        .items_center()
        .gap_1p5()
        .text_color(fg)
        .role(gpui_kit::Role::Group)
        .aria_label(format!("{}, {}", card.title, card.subtitle))
        .child(sticker_tile(
            ("gift-card-sticker", row_id),
            card.sticker.as_ref(),
            files,
            roots,
            110.,
            fallback,
        ))
        .child(
            div()
                .font_semibold()
                .text_center()
                .child(card.title.clone()),
        )
        .child(
            div()
                .text_sm()
                .text_center()
                .text_color(sub_fg)
                .child(card.subtitle.clone()),
        )
        .when(!card.text.is_empty(), |t| {
            t.child(div().text_sm().text_center().child(card.text.clone()))
        })
        .children(card.facts.iter().map(|(label, value)| {
            div()
                .w_full()
                .flex()
                .justify_between()
                .text_xs()
                .child(div().text_color(sub_fg).child(label.clone()))
                .child(div().font_medium().child(value.clone()))
        }))
        .when(to_me && !card.received_gift_id.is_empty(), |t| {
            t.child(
                Button::new(("gift-card-view", row_id))
                    .label("View my gifts")
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.open_my_gifts(cx))),
            )
        })
        .into_any_element()
}

crate::ui::shell::register_dialogs! {
    /// Stars balance and transaction history.
    Stars => DialogSpec::new(
        3800,
        |app| app.session().is_some_and(|s| s.hub.stars_open),
        QuillApp::build_stars_dialog,
    ),

    /// Received gifts of a user or channel.
    ReceivedGifts => DialogSpec::new(
        3900,
        |app| app.session().is_some_and(|s| s.hub.gifts_open),
        QuillApp::build_gifts_dialog,
    ),

    /// Read-only Premium features explainer.
    PremiumFeatures => DialogSpec::new(
        4000,
        |app| app.session().is_some_and(|s| s.hub.premium_open),
        QuillApp::build_premium_dialog,
    ),
}

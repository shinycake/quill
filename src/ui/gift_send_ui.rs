//! "Send a gift": the gifts `getAvailableGifts` lists for the open chat's
//! user or channel, a message, "hide my name" and the optional prepaid
//! upgrade (tdesktop `boxes/star_gift_box.cpp`, `GiftBox` / `SendGiftBox`).
//! A gift is paid with Stars from the balance, only by the button that names
//! the total. Telegram Premium gift options are paid in the payment
//! checkout (`ui/invoice_checkout.rs`), not from the Stars balance.

use super::app::QuillApp;
use super::chat_theme::fill_muted;
use super::gift_manage_ui::{hint, panel, stars};
use super::navigation::NavigationAction;
use super::premium_ui::sticker_tile;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::gift_manage::{AvailableGift, SendGate};
use quill::gift_market::{auction_tile, resale_tile_price};
use quill::local_time::now_unix;
use quill::state::Session;
use std::cell::RefCell;
use std::rc::Rc;

const TILE_W: f32 = 132.0;

impl QuillApp {
    /// Open the dialog for the open chat's user or channel.
    pub(super) fn open_send_gift_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat = self.session().and_then(|s| s.open_chat);
        let opened = chat.and_then(|chat| {
            self.live
                .as_mut()
                .map(|live| live.driver.open_send_gift(chat))
        });
        match opened {
            Some(Ok(())) => {
                self.payments
                    .gift_message_input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.payments.gift_hide_name = false;
                self.payments.gift_pay_upgrade = false;
            }
            _ => {
                self.connection.status_note =
                    "Open a chat with a person or a channel to choose who gets the gift.".into();
            }
        }
        cx.notify();
    }

    fn close_send_gift_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.close_send_gift();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.payments.gifts_manage.send.open = false;
        }
        cx.notify();
    }

    fn pick_send_gift(&mut self, id: Option<i64>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.select_send_gift(id);
        }
        let is_self = self
            .session()
            .and_then(|s| s.payments.gifts_manage.send.recipient.as_ref())
            .is_some_and(|r| r.is_self);
        self.payments
            .gift_message_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.payments.gift_hide_name = is_self;
        self.payments.gift_pay_upgrade = false;
        cx.notify();
    }

    fn submit_send_gift(&mut self, gift_id: i64, cx: &mut Context<Self>) {
        let text = self
            .payments
            .gift_message_input
            .read(cx)
            .value()
            .to_string();
        let (hide, upgrade) = (self.payments.gift_hide_name, self.payments.gift_pay_upgrade);
        let sent = self
            .live
            .as_mut()
            .map(|live| live.driver.send_star_gift(gift_id, &text, hide, upgrade));
        if let Some(Err(_)) = sent {
            self.connection.status_note = "could not send the gift".into();
        }
        cx.notify();
    }

    pub(super) fn build_send_gift_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SendGift, |this, _, cx| {
                this.close_send_gift_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let roots = this.media_display_roots();
            let Some(session) = this.session() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let send = &session.payments.gifts_manage.send;
            let title = match send.recipient.as_ref() {
                Some(r) if r.is_self => "Send a gift to yourself".to_string(),
                Some(r) => format!("Send a gift to {}", r.name),
                None => "Send a gift".to_string(),
            };
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title));
            let muted = cx.theme().muted_foreground;
            let mut body = div().flex().flex_col().gap_3();
            if let Some(done) = send.done.as_ref() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(super::chat_theme::success())
                        .child(done.clone()),
                );
            }
            if let Some(error) = send.error.as_ref() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                );
            }
            if let Some(selected) = send.selected_gift() {
                body = body.child(selected_gift_panel(this, session, selected, &roots, cx));
            } else {
                if !send.premium_options.is_empty() {
                    body = body.child(premium_section(this, session, cx));
                }
                body = body.child(div().text_sm().font_semibold().child("Gifts for Stars"));
                if let Some(balance) = session.payments.hub.balance {
                    body = body.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("Your balance: {}", stars(balance.stars))),
                    );
                }
                if send.gifts_loading {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Loading gifts\u{2026}"),
                    );
                } else if send.gifts.is_empty() && send.gifts_loaded {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("No gifts are available."),
                    );
                }
                let mut grid = div().flex().flex_wrap().gap_2();
                for (ix, gift) in send.gifts.iter().enumerate() {
                    grid = grid.child(gift_tile(session, ix, gift, &roots, cx));
                }
                body = body.child(grid).child(
                    Button::new("gift-by-name")
                        .label("Buy a collectible by name\u{2026}")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_send_gift_dialog(cx);
                            this.navigate(NavigationAction::Gift, window, cx);
                        })),
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
}

/// What a tile does besides sending: an auction or the resale list.
enum TileMarket {
    None,
    /// Badge ("auction" / "soon") and line ("Join" / "View").
    Auction(&'static str, &'static str),
    /// Sold out with listings on resale: the lowest price line.
    Resale(String),
}

fn gift_tile(
    session: &Session,
    ix: usize,
    gift: &AvailableGift,
    roots: &[std::path::PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let send = &session.payments.gifts_manage.send;
    let now = now_unix();
    let gate = send
        .recipient
        .as_ref()
        .map(|r| gift.gate(r, session.payments.premium_option == Some(true), now))
        .unwrap_or(SendGate::SoldOut);
    let to_channel = send.recipient.as_ref().is_some_and(|r| r.is_channel);
    let market = match gift.auction.as_ref() {
        // Auction bids go to people only (`placeGiftAuctionBid.user_id`).
        Some(info) if !to_channel => {
            let (badge, line) = auction_tile(info.start_date, gift.sold_out(), now);
            TileMarket::Auction(badge, line)
        }
        Some(_) => TileMarket::None,
        None if gift.sold_out() && gift.resale_count > 0 => TileMarket::Resale(resale_tile_price(
            gift.min_resale_star_count,
            gift.resale_count,
        )),
        None => TileMarket::None,
    };
    let ready = gate == SendGate::Ready;
    let clickable = ready || !matches!(market, TileMarket::None);
    let id = gift.id;
    let reason = match (&market, gate) {
        (TileMarket::None, SendGate::Auction) => Some("Not for channels"),
        (TileMarket::None, gate) => gate.reason(),
        _ => None,
    };
    let (badge, price, line) = match &market {
        TileMarket::Auction(badge, line) => (
            Some((*badge, cx.theme().warning)),
            div().text_sm().font_semibold().child(*line),
            String::new(),
        ),
        TileMarket::Resale(price) => (
            Some(("resale", super::chat_theme::success().into())),
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_sm()
                .font_semibold()
                .child(super::earn::star_icon(12.))
                .child(price.clone()),
            String::new(),
        ),
        TileMarket::None => (
            None,
            div()
                .text_sm()
                .font_semibold()
                .child(stars(gift.star_count)),
            reason
                .map(str::to_string)
                .or_else(|| gift.supply_label())
                .unwrap_or_default(),
        ),
    };
    let label = match &market {
        TileMarket::Auction(badge, line) => format!("Gift auction, {badge}, {line}"),
        TileMarket::Resale(price) => format!("Gift on resale from {price} Stars"),
        TileMarket::None => format!(
            "Gift for {}{}",
            stars(gift.star_count),
            reason.map(|r| format!(", {r}")).unwrap_or_default()
        ),
    };
    let tile = div()
        .id(("send-gift-tile", ix as u64))
        .relative()
        .w(px(TILE_W))
        .p_2()
        .rounded_lg()
        .bg(fill_muted().opacity(0.35))
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .role(gpui_kit::Role::Button)
        .aria_label(label)
        .children(badge.map(|(text, color)| {
            div()
                .absolute()
                .top_1()
                .right_1()
                .px_1p5()
                .rounded_full()
                .bg(color)
                .text_xs()
                .font_semibold()
                .text_color(gpui_kit::white())
                .child(text)
        }))
        .child(sticker_tile(
            ("send-gift-sticker", ix as u64),
            gift.sticker.as_ref(),
            &session.media.files,
            roots,
            72.,
            "\u{1F381}",
        ))
        .child(price)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(line),
        );
    if !clickable {
        return tile.opacity(0.55).into_any_element();
    }
    let auction_id = gift.auction.as_ref().map(|info| info.id.clone());
    let title = gift.title.clone();
    let resale = matches!(market, TileMarket::Resale(_));
    tile.cursor_pointer()
        .tab_index(0)
        .on_click(cx.listener(move |this, _, window, cx| {
            if let Some(auction_id) = auction_id.as_deref() {
                this.open_gift_auction(id, auction_id, cx);
            } else if resale {
                this.open_gift_resale(id, &title, cx);
            } else {
                this.pick_send_gift(Some(id), window, cx);
            }
        }))
        .into_any_element()
}

fn selected_gift_panel(
    this: &QuillApp,
    session: &Session,
    gift: &AvailableGift,
    roots: &[std::path::PathBuf],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let send = &session.payments.gifts_manage.send;
    let Some(recipient) = send.recipient.as_ref() else {
        return div().into_any_element();
    };
    let pay_upgrade = this.payments.gift_pay_upgrade && gift.upgrade_star_count > 0;
    let total = gift.total_cost(pay_upgrade);
    let balance = session.payments.hub.balance;
    let too_poor = balance.is_some_and(|b| b.stars < total);
    let limit = session.payments.gift_text_length_max;
    let id = gift.id;
    let mut col = div().flex().flex_col().gap_2();
    col = col
        .child(
            Button::new("send-gift-back")
                .label("\u{2039} All gifts")
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.pick_send_gift(None, window, cx))),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(sticker_tile(
                    ("send-gift-detail", 0),
                    gift.sticker.as_ref(),
                    &session.media.files,
                    roots,
                    112.,
                    "\u{1F381}",
                ))
                .child(
                    div()
                        .text_lg()
                        .font_semibold()
                        .child(stars(gift.star_count)),
                )
                .children(gift.supply_label().map(|l| hint(l, cx))),
        );
    if let Some(max) = limit {
        col = col
            .child(Textarea::new(&this.payments.gift_message_input).aria_label("Gift message"))
            .child(hint(format!("Optional. Up to {max} characters."), cx));
    }
    col = col.child(
        Checkbox::new("send-gift-hide-name")
            .label("Hide my name and message")
            .checked(this.payments.gift_hide_name)
            .on_click(cx.listener(|this, &on, _, cx| {
                this.payments.gift_hide_name = on;
                cx.notify();
            })),
    );
    if gift.upgrade_star_count > 0 && !recipient.is_self {
        col = col
            .child(
                Checkbox::new("send-gift-upgrade")
                    .label(format!(
                        "Also pay for the upgrade ({})",
                        stars(gift.upgrade_star_count)
                    ))
                    .checked(pay_upgrade)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.payments.gift_pay_upgrade = on;
                        cx.notify();
                    })),
            )
            .child(hint(
                "The receiver can turn it into a collectible for free.",
                cx,
            ));
    }
    let mut footer = panel(cx).child(div().text_sm().child(format!(
        "This takes {} from your balance{}.",
        stars(total),
        balance
            .map(|b| format!(" ({} left after)", stars((b.stars - total).max(0))))
            .unwrap_or_default()
    )));
    if too_poor {
        footer = footer.child(
            div()
                .text_sm()
                .text_color(cx.theme().danger)
                .child("Your balance is too low for this gift."),
        );
    }
    col.child(
        footer.child(
            Button::new("send-gift-confirm")
                .label(if send.sending {
                    "Sending\u{2026}".to_string()
                } else if recipient.is_self {
                    format!("Buy for myself: {}", stars(total))
                } else {
                    format!("Send for {}", stars(total))
                })
                .disabled(send.sending || too_poor)
                .on_click(cx.listener(move |this, _, _, cx| this.submit_send_gift(id, cx))),
        ),
    )
    .into_any_element()
}

fn premium_section(this: &QuillApp, session: &Session, cx: &mut Context<QuillApp>) -> AnyElement {
    let send = &session.payments.gifts_manage.send;
    let can_pay = send
        .recipient
        .as_ref()
        .is_some_and(|r| !r.is_self && !r.is_channel);
    let mut col = div().flex().flex_col().gap_1().child(
        div()
            .text_sm()
            .font_semibold()
            .child("Gift Telegram Premium"),
    );
    if can_pay && let Some(max) = session.payments.gift_text_length_max {
        col = col
            .child(Textarea::new(&this.payments.gift_message_input).aria_label("Gift message"))
            .child(hint(format!("Optional. Up to {max} characters."), cx));
    }
    let stars_offered = can_pay && send.premium_options.iter().any(|o| o.star_count > 0);
    let by_stars = stars_offered && this.payments.gift_premium_by_stars;
    if stars_offered {
        col = col.child(
            Checkbox::new("premium-gift-by-stars")
                .label("Pay with Stars instead")
                .checked(by_stars)
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.payments.gift_premium_by_stars = on;
                    cx.notify();
                })),
        );
        if let Some(balance) = session.payments.hub.balance {
            col = col.child(hint(format!("Your balance: {}", stars(balance.stars)), cx));
        }
    }
    for (ix, option) in send.premium_options.iter().enumerate() {
        let paid_in_stars = by_stars && option.star_count > 0;
        let price = if paid_in_stars {
            stars(option.star_count)
        } else {
            option.price_label()
        };
        let row = div()
            .id(("premium-gift-option", ix as u64))
            .flex()
            .justify_between()
            .gap_3()
            .text_sm()
            .child(div().child(option.title()))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .when(option.discount_percentage > 0, |row| {
                        row.child(
                            div()
                                .text_color(super::chat_theme::success())
                                .child(format!("-{}%", option.discount_percentage)),
                        )
                    })
                    .child(div().font_medium().child(price.clone())),
            );
        col = col.child(if can_pay {
            let option = option.clone();
            row.cursor_pointer()
                .tab_index(0)
                .role(gpui_kit::Role::Button)
                .aria_label(format!("Gift Premium for {}, {}", option.title(), price))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.buy_premium_gift(option.clone(), paid_in_stars, window, cx);
                }))
        } else {
            row
        });
    }
    col.child(hint(
        if can_pay {
            "You pay for Premium in the checkout."
        } else {
            "Premium can only be gifted to another person."
        },
        cx,
    ))
    .into_any_element()
}

crate::ui::shell::register_dialogs! {
    SendGift => DialogSpec::new(
        3950,
        |app| app.session().is_some_and(|s| s.payments.gifts_manage.send.open),
        QuillApp::build_send_gift_dialog,
    ),
}

//! The owner's actions on one received gift: pin, upgrade, transfer, resale
//! price, crafting and collection membership (tdesktop's gift view box). Upgrade and
//! transfer spend Stars, so each opens an inline confirmation that names the
//! price; the handlers are in `gift_manage_ui`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gift_manage_ui::{hint, panel, stars};
use super::message_text::format_unix_date_time;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_manage::{
    GiftFlow, PinGate, ResaleGate, TransferGate, parse_stars_input, pinned_ids,
};
use quill::local_time::now_unix;
use quill::premium_hub::ReceivedGift;
use quill::state::Session;

/// The owner's actions for the gift on screen, below the show/hide and
/// convert buttons of the gift page. `price_text` is what the resale price
/// field holds right now.
pub(super) fn gift_owner_actions(
    session: &Session,
    gift: &ReceivedGift,
    price_input: &Entity<InputState>,
    price_text: &str,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let hub = &session.payments.hub;
    let manage = &session.payments.gifts_manage;
    let busy = hub.gift_mutating;
    let now = now_unix();
    let balance = hub.balance;
    let poor = |cost: i64| balance.is_some_and(|b| b.stars < cost);
    let balance_line = || {
        format!(
            "Your balance ({}) is too low.",
            balance.map(|b| stars(b.stars)).unwrap_or_default()
        )
    };
    let mut col = div().flex().flex_col().gap_2();
    col = pin_section(col, gift, manage, hub.gifts.as_slice(), busy, cx);
    if let Some(cost) = gift.upgrade_cost(true) {
        col = upgrade_section(
            col,
            gift,
            manage,
            cost,
            poor(cost),
            &balance_line(),
            busy,
            cx,
        );
    }
    match gift.transfer_gate(true, now) {
        TransferGate::Later(date) => {
            col = col.child(hint(
                format!(
                    "You can transfer this gift after {}.",
                    format_unix_date_time(i64::from(date))
                ),
                cx,
            ));
        }
        TransferGate::Ready { star_count } => {
            col = transfer_section(
                col,
                gift,
                session,
                star_count,
                poor(star_count),
                &balance_line(),
                busy,
                cx,
            );
        }
        TransferGate::Unavailable => {}
    }
    match gift.resale_gate(true, now) {
        ResaleGate::Later(date) => {
            col = col.child(hint(
                format!(
                    "You can sell this gift after {}.",
                    format_unix_date_time(i64::from(date))
                ),
                cx,
            ));
        }
        ResaleGate::TonOnly => {
            col = col.child(hint(
                "This gift is listed for TON. Change the listing in an official app.",
                cx,
            ));
        }
        ResaleGate::Ready => {
            col = resale_section(col, gift, manage, price_input, price_text, busy, cx);
        }
        ResaleGate::Unavailable => {}
    }
    // tdesktop's "Craft" in the gift's menu (`CanCraftGift`).
    if let Some(craft) = super::gift_craft_ui::craft_gift_of(gift) {
        col = col.child(
            Button::new("gift-craft")
                .label("Craft")
                .small()
                .disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_craft_info(craft.clone(), cx);
                })),
        );
    }
    if !manage.collections.is_empty() {
        let mut chips = div().flex().flex_wrap().gap_1();
        for collection in &manage.collections {
            let cid = collection.id;
            let gid = gift.id.clone();
            let inside = gift.in_collection(cid);
            chips = chips.child(
                Button::new(format!("gift-in-collection-{cid}"))
                    .label(if inside {
                        format!("\u{2713} {}", collection.name)
                    } else {
                        collection.name.clone()
                    })
                    .small()
                    .selected(inside)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change_collection_membership(cid, &gid, !inside, cx)
                    })),
            );
        }
        col = col.child(hint("Collections", cx)).child(chips);
    }
    col.into_any_element()
}

fn pin_section(
    col: Div,
    gift: &ReceivedGift,
    manage: &quill::gift_manage::GiftManageState,
    gifts: &[ReceivedGift],
    busy: bool,
    cx: &mut Context<QuillApp>,
) -> Div {
    // Only the unfiltered list has every pin loaded.
    if manage.active_collection != 0 {
        return col;
    }
    let id = gift.id.clone();
    let limit = manage.limits.pinned();
    match gift.pin_gate(true, pinned_ids(gifts).len(), limit) {
        PinGate::Pin => col.child(
            Button::new("gift-pin")
                .label("Pin to the top of my profile")
                .ghost()
                .disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| this.pin_received_gift(&id, true, cx))),
        ),
        PinGate::Unpin => col.child(
            Button::new("gift-unpin")
                .label("Unpin from the top")
                .ghost()
                .disabled(busy)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.pin_received_gift(&id, false, cx)),
                ),
        ),
        PinGate::Full => col.child(hint(
            format!("You can pin up to {limit} gifts. Unpin one to pin this gift."),
            cx,
        )),
        PinGate::Unavailable => col,
    }
}

#[allow(clippy::too_many_arguments)]
fn upgrade_section(
    col: Div,
    gift: &ReceivedGift,
    manage: &quill::gift_manage::GiftManageState,
    cost: i64,
    too_poor: bool,
    balance_line: &str,
    busy: bool,
    cx: &mut Context<QuillApp>,
) -> Div {
    let id = gift.id.clone();
    let Some(GiftFlow::Upgrade {
        id: open,
        keep_details,
    }) = &manage.flow
    else {
        return col.child(
            Button::new("gift-upgrade")
                .label(if cost == 0 {
                    "Upgrade to a collectible (free)".to_string()
                } else {
                    format!("Upgrade to a collectible ({})", stars(cost))
                })
                .ghost()
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_gift_flow(
                        Some(GiftFlow::Upgrade {
                            id: id.clone(),
                            keep_details: false,
                        }),
                        window,
                        cx,
                    )
                })),
        );
    };
    if *open != gift.id {
        return col;
    }
    let keep = *keep_details;
    let mut p = panel(cx).child(div().text_sm().child(if cost == 0 {
        "Upgrade this gift to a collectible. The sender already paid for it, so it costs you nothing."
            .to_string()
    } else {
        format!(
            "Upgrade this gift to a collectible for {}? The Stars leave your balance and this cannot be undone.",
            stars(cost)
        )
    }));
    p = p.child(
        Checkbox::new("gift-upgrade-keep")
            .label("Keep the original sender and message")
            .checked(keep)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_keep_details(cx))),
    );
    if too_poor {
        p = p.child(hint(balance_line.to_string(), cx));
    }
    col.child(
        p.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("gift-upgrade-yes")
                        .label(if cost == 0 {
                            "Upgrade".to_string()
                        } else {
                            format!("Upgrade for {}", stars(cost))
                        })
                        .disabled(busy || too_poor)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.confirm_upgrade(&id, keep, cx)),
                        ),
                )
                .child(
                    Button::new("gift-upgrade-no")
                        .label("Cancel")
                        .ghost()
                        .on_click(
                            cx.listener(|this, _, window, cx| this.set_gift_flow(None, window, cx)),
                        ),
                ),
        ),
    )
}

#[allow(clippy::too_many_arguments)]
fn transfer_section(
    col: Div,
    gift: &ReceivedGift,
    session: &Session,
    star_count: i64,
    too_poor: bool,
    balance_line: &str,
    busy: bool,
    cx: &mut Context<QuillApp>,
) -> Div {
    let id = gift.id.clone();
    let manage = &session.payments.gifts_manage;
    let open = matches!(&manage.flow, Some(GiftFlow::Transfer { id: open }) if *open == gift.id);
    if !open {
        return col.child(
            Button::new("gift-transfer")
                .label("Transfer to the open chat\u{2026}")
                .ghost()
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_gift_flow(Some(GiftFlow::Transfer { id: id.clone() }), window, cx)
                })),
        );
    }
    let recipient = session
        .open_chat
        .and_then(|chat| session.gift_recipient(chat))
        .filter(|r| !r.is_self);
    let cancel = Button::new("gift-transfer-no")
        .label("Cancel")
        .ghost()
        .on_click(cx.listener(|this, _, window, cx| this.set_gift_flow(None, window, cx)));
    let Some(recipient) = recipient else {
        return col.child(
            panel(cx)
                .child(div().text_sm().child(
                    "Open the chat of the person or channel that should get this gift, then come back here.",
                ))
                .child(div().flex().child(cancel)),
        );
    };
    let mut p = panel(cx).child(div().text_sm().child(format!(
        "Transfer this gift to {}? It leaves your profile{} This cannot be undone.",
        recipient.name,
        if star_count > 0 {
            format!(" and {} are taken from your balance.", stars(star_count))
        } else {
            ".".to_string()
        }
    )));
    if too_poor {
        p = p.child(hint(balance_line.to_string(), cx));
    }
    col.child(
        p.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("gift-transfer-yes")
                        .label(if star_count > 0 {
                            format!("Transfer for {}", stars(star_count))
                        } else {
                            "Transfer".to_string()
                        })
                        .danger()
                        .disabled(busy || too_poor)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.confirm_transfer(&id, cx)),
                        ),
                )
                .child(cancel),
        ),
    )
}

fn resale_section(
    col: Div,
    gift: &ReceivedGift,
    manage: &quill::gift_manage::GiftManageState,
    price_input: &Entity<InputState>,
    price_text: &str,
    busy: bool,
    cx: &mut Context<QuillApp>,
) -> Div {
    let id = gift.id.clone();
    let listed = gift.manage.listed;
    let open = matches!(&manage.flow, Some(GiftFlow::Resale { id: open }) if *open == gift.id);
    if !open {
        let open_id = id.clone();
        let edit = Button::new("gift-resale")
            .label(if listed {
                "Change the price"
            } else {
                "Sell on the marketplace\u{2026}"
            })
            .ghost()
            .disabled(busy)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.set_gift_flow(
                    Some(GiftFlow::Resale {
                        id: open_id.clone(),
                    }),
                    window,
                    cx,
                )
            }));
        if !listed {
            return col.child(edit);
        }
        return col
            .child(hint(
                format!("On sale for {}.", stars(gift.manage.resale_star_count)),
                cx,
            ))
            .child(
                div().flex().gap_2().child(edit).child(
                    Button::new("gift-resale-remove")
                        .label("Remove from sale")
                        .ghost()
                        .disabled(busy)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.confirm_resale(&id, None, cx)),
                        ),
                ),
            );
    }
    let (min, max) = manage.limits.resale_range();
    let price = parse_stars_input(price_text).filter(|p| manage.limits.resale_price_ok(*p));
    col.child(
        panel(cx)
            .child(div().text_sm().child(
                "Put this gift on the marketplace. Anyone can buy it for the price you set.",
            ))
            .child(Input::new(price_input).aria_label("Resale price in Stars"))
            .child(hint(
                format!("Between {} and {}.", stars(min), stars(max)),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("gift-resale-yes")
                            .label(match price {
                                Some(p) if listed => format!("Change price to {}", stars(p)),
                                Some(p) => format!("List for {}", stars(p)),
                                None => "List for sale".to_string(),
                            })
                            .disabled(busy || price.is_none())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let typed =
                                    this.payments.gift_price_input.read(cx).value().to_string();
                                this.confirm_resale(&id, parse_stars_input(&typed), cx)
                            })),
                    )
                    .child(
                        Button::new("gift-resale-no")
                            .label("Cancel")
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_gift_flow(None, window, cx)
                            })),
                    ),
            ),
    )
}

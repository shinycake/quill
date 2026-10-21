//! The owner's actions on one received gift: pin, upgrade, transfer, resale
//! price, crafting and collection membership (tdesktop's gift view box). Upgrade
//! spends Stars, so it opens an inline confirmation that names the price; the
//! handlers are in `gift_manage_ui`. Transfer opens the transfer picker
//! (`gift_transfer_ui`).
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
use quill::lang::{keys, tr, tr_with};
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
        tr_with(
            keys::Q_MONEY_YOUR_BALANCE_IS_TOO_LOW_3,
            &[(
                "unwrap_or_default",
                &(balance.map(|b| stars(b.stars)).unwrap_or_default()).to_string(),
            )],
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
                tr_with(
                    keys::Q_MONEY_YOU_CAN_TRANSFER_THIS_GIFT,
                    &[(
                        "format_unix_date_time",
                        &(format_unix_date_time(i64::from(date))).to_string(),
                    )],
                ),
                cx,
            ));
        }
        TransferGate::Ready { .. } => col = col.child(transfer_button(gift, busy, cx)),
        // tdesktop offers Transfer for a collectible in a TON wallet too;
        // it opens "Action Locked".
        TransferGate::Unavailable if gift.held_outside_telegram() && !gift.was_refunded => {
            col = col.child(transfer_button(gift, busy, cx));
        }
        TransferGate::Unavailable => {}
    }
    match gift.resale_gate(true, now) {
        ResaleGate::Later(date) => {
            col = col.child(hint(
                tr_with(
                    keys::Q_MONEY_YOU_CAN_SELL_THIS_GIFT,
                    &[(
                        "format_unix_date_time",
                        &(format_unix_date_time(i64::from(date))).to_string(),
                    )],
                ),
                cx,
            ));
        }
        ResaleGate::TonOnly => {
            col = col.child(hint(tr(keys::Q_MONEY_THIS_GIFT_IS_LISTED_FOR), cx));
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
                .label(tr(keys::GIFT_CRAFT_MENU_BUTTON))
                .small()
                .disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_craft_info(craft.clone(), cx);
                })),
        );
    }
    if gift.can_delete(true) {
        let id = gift.id.clone();
        col = col.child(
            div().debug_selector(|| "gift-delete".into()).child(
                Button::new("gift-delete")
                    .label(tr(keys::GIFT_MENU_DELETE))
                    .small()
                    .ghost()
                    .text_color(cx.theme().danger)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.ask_delete_gift(&id, window, cx)
                    })),
            ),
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
        col = col
            .child(hint(tr(keys::Q_MONEY_COLLECTIONS), cx))
            .child(chips);
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
                .label(tr(keys::Q_MONEY_PIN_TO_THE_TOP_OF))
                .ghost()
                .disabled(busy)
                .on_click(cx.listener(move |this, _, _, cx| this.pin_received_gift(&id, true, cx))),
        ),
        PinGate::Unpin => col.child(
            Button::new("gift-unpin")
                .label(tr(keys::Q_MONEY_UNPIN_FROM_THE_TOP))
                .ghost()
                .disabled(busy)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.pin_received_gift(&id, false, cx)),
                ),
        ),
        PinGate::Full => col.child(hint(
            tr_with(
                keys::Q_MONEY_YOU_CAN_PIN_UP_TO,
                &[("limit", &limit.to_string())],
            ),
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
                    tr(keys::Q_MONEY_UPGRADE_TO_A_COLLECTIBLE_FREE).to_string()
                } else {
                    tr_with(
                        keys::Q_MONEY_UPGRADE_TO_A_COLLECTIBLE,
                        &[("stars", &(stars(cost)).to_string())],
                    )
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
        tr(keys::Q_MONEY_UPGRADE_THIS_GIFT_TO_A).to_string()
    } else {
        tr_with(
            keys::Q_MONEY_UPGRADE_THIS_GIFT_TO_A_2,
            &[("stars", &(stars(cost)).to_string())],
        )
    }));
    p = p.child(
        Checkbox::new("gift-upgrade-keep")
            .label(tr(keys::Q_MONEY_KEEP_THE_ORIGINAL_SENDER_AND))
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
                            tr(keys::GIFT_UNIQUE_UPGRADE).to_string()
                        } else {
                            tr_with(
                                keys::GIFT_UPGRADE_BUTTON,
                                &[("price", &(stars(cost)).to_string())],
                            )
                        })
                        .disabled(busy || too_poor)
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.confirm_upgrade(&id, keep, cx)),
                        ),
                )
                .child(
                    Button::new("gift-upgrade-no")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(
                            cx.listener(|this, _, window, cx| this.set_gift_flow(None, window, cx)),
                        ),
                ),
        ),
    )
}

/// "Transfer": opens the transfer picker (tdesktop `ShowTransferGiftBox`).
fn transfer_button(gift: &ReceivedGift, busy: bool, cx: &mut Context<QuillApp>) -> Button {
    let id = gift.id.clone();
    Button::new("gift-transfer")
        .label(tr(keys::GIFT_TRANSFER_BUTTON))
        .ghost()
        .disabled(busy)
        .on_click(cx.listener(move |this, _, window, cx| this.open_gift_transfer(&id, window, cx)))
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
                tr(keys::Q_MONEY_CHANGE_THE_PRICE)
            } else {
                tr(keys::Q_MONEY_SELL_ON_THE_MARKETPLACE)
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
                tr_with(
                    keys::Q_MONEY_ON_SALE_FOR,
                    &[(
                        "resale_star_count",
                        &(stars(gift.manage.resale_star_count)).to_string(),
                    )],
                ),
                cx,
            ))
            .child(
                div().flex().gap_2().child(edit).child(
                    Button::new("gift-resale-remove")
                        .label(tr(keys::Q_MONEY_REMOVE_FROM_SALE))
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
            .child(
                div()
                    .text_sm()
                    .child(tr(keys::Q_MONEY_PUT_THIS_GIFT_ON_THE)),
            )
            .child(Input::new(price_input).aria_label(tr(keys::Q_MONEY_RESALE_PRICE_IN_STARS)))
            .child(hint(
                tr_with(
                    keys::Q_MONEY_BETWEEN_AND,
                    &[
                        ("stars", &(stars(min)).to_string()),
                        ("stars_2", &(stars(max)).to_string()),
                    ],
                ),
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("gift-resale-yes")
                            .label(match price {
                                Some(p) if listed => tr_with(
                                    keys::Q_MONEY_CHANGE_PRICE_TO,
                                    &[("stars", &(stars(p)).to_string())],
                                ),
                                Some(p) => tr_with(
                                    keys::Q_MONEY_LIST_FOR,
                                    &[("stars", &(stars(p)).to_string())],
                                ),
                                None => tr(keys::Q_MONEY_LIST_FOR_SALE).to_string(),
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
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_gift_flow(None, window, cx)
                            })),
                    ),
            ),
    )
}

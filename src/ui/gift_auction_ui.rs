//! Gift auctions: the about, info, send, bought, change-recipient,
//! variants and "Active Auctions" boxes and the chat-list bar (tdesktop
//! `boxes/star_gift_auction_box.cpp`,
//! `dialogs/suggestions/suggestion_gift_auctions.cpp`). The bid box is in
//! `gift_auction_bid.rs`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::success;
use super::gift_manage_ui::hint;
use super::gift_market_state::MarketBox;
use super::gift_market_ui::{BoxView, feature_row, two_line_button};
use super::message_text::format_unix_date_time;
use super::premium_ui::sticker_tile;
use super::shell::DialogKind;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::gift_market::{
    AuctionPhase, AuctionState, about_features, active_row, auction_bar, average_tooltip,
    bought_link, bought_title, change_recipient_text, info_rows, join_button, nice_countdown,
    seconds_left, send_about, set_auction_about_shown, variants_link,
};
use quill::lang::{keys, tr, tr_with, trn_decimal};
use quill::local_time::now_unix;
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::telegram::envelope::MessageSender;

/// The gift's collection title, for headings.
pub(super) fn auction_title(session: &Session, state: &AuctionState) -> String {
    if !state.gift.title.is_empty() {
        return state.gift.title.clone();
    }
    session
        .gift_market_title(state.gift_id())
        .unwrap_or_else(|| tr(keys::PROFILE_ACTION_SHORT_GIFT).into())
}

/// The heading block of the auction boxes: the sticker, an optional
/// pretitle and the title.
fn auction_cover(
    session: &Session,
    state: &AuctionState,
    pretitle: Option<&'static str>,
    roots: &[std::path::PathBuf],
    cx: &App,
) -> Div {
    let preview = session.payments.gift_market.previews.get(&state.gift_id());
    // tdesktop cycles random model / backdrop / symbol sets every second.
    let cover = preview.and_then(|preview| preview.cover(now_unix().max(0) as u64));
    let backdrop = match cover {
        Some((_, _, backdrop)) => div().bg(linear_gradient(
            180.,
            linear_color_stop(rgb(backdrop.center), 0.),
            linear_color_stop(rgb(backdrop.edge), 1.),
        )),
        None => div().bg(cx.theme().secondary),
    };
    let sticker = cover
        .and_then(|(model, _, _)| model.sticker.as_ref())
        .or(state.gift.sticker.as_ref());
    let on_backdrop = cover.is_some();
    backdrop
        .rounded_lg()
        .py_4()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .child(sticker_tile(
            ("auction-cover", state.gift_id() as u64),
            sticker,
            &session.media.files,
            roots,
            96.,
            "\u{1F381}",
        ))
        .when_some(pretitle, |col, pretitle| {
            col.child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(if on_backdrop {
                        gpui_kit::white().opacity(0.8)
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(pretitle),
            )
        })
        .child(
            div()
                .text_lg()
                .font_semibold()
                .text_color(if on_backdrop {
                    gpui_kit::white()
                } else {
                    cx.theme().foreground
                })
                .child(auction_title(session, state)),
        )
}

impl QuillApp {
    fn auction_state(&self, gift_id: i64) -> Option<&AuctionState> {
        self.session()?.payments.gift_market.auctions.get(&gift_id)
    }

    fn open_auction_bought(&mut self, gift_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_gift_auction_acquired(gift_id);
        }
        self.market_push(MarketBox::AuctionBought { gift_id }, cx);
    }

    /// "Understood": remembered, then the box the auction opens with.
    pub(super) fn understood_auction(&mut self, gift_id: i64, cx: &mut Context<Self>) {
        set_auction_about_shown(true);
        let below = self.gift_market.stack.len() > 1;
        self.gift_market.stack.pop();
        if !below {
            self.show_auction_box(gift_id, cx);
        }
        cx.notify();
    }

    /// The ⋯ menu in the title row of the info and bid boxes: About and
    /// Copy Link. tdesktop drops it once the auction has finished.
    pub(super) fn auction_menu(
        &self,
        state: &AuctionState,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        state.active()?;
        let owner = cx.entity().downgrade();
        let gift_id = state.gift_id();
        let link = state.gift.auction.as_ref().map(|info| info.link());
        let menu = Button::new("auction-menu")
            .icon(IconName::EllipsisVertical)
            .ghost()
            .xsmall()
            .accessibility_label(tr(keys::Q_MONEY_AUCTION_ACTIONS))
            .dropdown_menu(move |mut menu, _, _| {
                let about = owner.clone();
                menu = menu.item(PopupMenuItem::new(tr(keys::AUCTION_MENU_ABOUT)).on_click(
                    move |_, _, cx| {
                        let _ = about.update(cx, |this, cx| {
                            this.market_push(MarketBox::AuctionAbout { gift_id }, cx);
                        });
                    },
                ));
                if let Some(link) = link.clone() {
                    let copy = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tr(keys::AUCTION_MENU_COPY_LINK)).on_click(
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(link.clone()));
                                let _ = copy.update(cx, |this, cx| {
                                    this.connection
                                        .set_note(StatusNote::info(tr(keys::STICKERS_COPIED)));
                                    cx.notify();
                                });
                            },
                        ),
                    );
                }
                menu
            });
        Some(menu.into_any_element())
    }

    pub(super) fn auction_about_box(&mut self, gift_id: i64, cx: &mut Context<Self>) -> BoxView {
        let (per_round, rounds) = self.auction_state(gift_id).map_or((1, 1), |state| {
            (
                state.per_round(),
                state.active().map_or(1, |active| active.total_rounds),
            )
        });
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_4().child(
            div()
                .text_sm()
                .text_center()
                .text_color(muted)
                .child(tr(keys::AUCTION_ABOUT_SUBTITLE)),
        );
        let icons = [IconName::Star, IconName::RefreshCw, IconName::Undo2];
        for ((title, text), icon) in about_features(per_round, rounds).into_iter().zip(icons) {
            body = body.child(feature_row(icon, title, text, cx));
        }
        BoxView::new(tr(keys::AUCTION_ABOUT_TITLE), body).footer(
            div().flex().justify_end().child(
                Button::new("auction-understood")
                    .label(tr(keys::AUCTION_ABOUT_UNDERSTOOD))
                    .primary()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.understood_auction(gift_id, cx);
                    })),
            ),
        )
    }

    pub(super) fn auction_info_box(&mut self, gift_id: i64, cx: &mut Context<Self>) -> BoxView {
        if let Some(live) = self.live.as_mut() {
            // The cover's variants; sent once per gift.
            let _ = live.driver.fetch_gift_upgrade_preview(gift_id);
        }
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new(tr(keys::AUCTION_ABOUT_TITLE), div());
        };
        let Some(state) = session.payments.gift_market.auctions.get(&gift_id) else {
            return BoxView::new(
                tr(keys::AUCTION_ABOUT_TITLE),
                div().child(tr(keys::Q_MONEY_LOADING)),
            );
        };
        // Countdowns and the cover change every second.
        self.request_animation_tick(1, cx);
        let now = now_unix();
        let started = state.started(now);
        let finished = state.finished(now);
        let title = auction_title(session, state);
        let mut body = div().flex().flex_col().gap_3().child(auction_cover(
            session,
            state,
            (!started).then_some(tr(keys::AUCTION_PREVIEW_NAME)),
            &roots,
            cx,
        ));
        body = body.child(
            Button::new("auction-learn-gifts")
                .label(tr(keys::Q_MONEY_LEARN_MORE_ABOUT_TELEGRAM_GIFTS))
                .ghost()
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.open_premium(cx))),
        );
        let rows = info_rows(state, now, |date| format_unix_date_time(i64::from(date)));
        body = body.child(super::ton_wallet_details::facts(rows).bordered(true));
        if let AuctionPhase::Finished(done) = &state.phase
            && done.average_price > 0
        {
            body = body.child(hint(average_tooltip(done.average_price, &title), cx));
        }
        let acquired = state.acquired();
        if acquired > 0 {
            body = body.child(
                Button::new("auction-bought")
                    .label(format!("{} \u{203A}", bought_link(acquired)))
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_auction_bought(gift_id, cx);
                    })),
            );
        } else if !started
            && let Some(preview) = session.payments.gift_market.previews.get(&gift_id)
            && !preview.models.is_empty()
        {
            body = body.child(
                Button::new("auction-variants")
                    .label(format!(
                        "{} \u{203A}",
                        variants_link(preview.models.len() as i32)
                    ))
                    .ghost()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.market_push(MarketBox::Variants { gift_id }, cx);
                    })),
            );
        }
        let (label, subtitle) = join_button(state, now, false);
        let action = two_line_button("auction-join", label, subtitle).on_click(cx.listener(
            move |this, _, window, cx| {
                if finished {
                    this.market_back(cx);
                    this.close_kit_dialog_if_done(DialogKind::GiftMarket, window, cx);
                } else {
                    this.payments
                        .gift_message_input
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.market_push(MarketBox::AuctionSend { gift_id }, cx);
                }
            },
        ));
        let view = BoxView::new(title, body).footer(div().flex().justify_end().child(action));
        match self.auction_menu(state, cx) {
            Some(menu) => view.title_action(menu),
            None => view,
        }
    }

    /// "Send a Gift" for an auction gift (tdesktop `SendGiftBox` with an
    /// auction): the message, "Hide My Name", then the bid box.
    pub(super) fn auction_send_box(&mut self, gift_id: i64, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new(tr(keys::GIFT_SEND_TITLE), div());
        };
        let Some(state) = session.payments.gift_market.auctions.get(&gift_id) else {
            return BoxView::new(tr(keys::GIFT_SEND_TITLE), div());
        };
        self.request_animation_tick(1, cx);
        let now = now_unix();
        let recipient = self.gift_market.recipient.clone();
        let hide_about = match recipient.as_ref() {
            Some(r) if !r.is_self => tr_with(
                keys::GIFT_SEND_ANONYMOUS_ABOUT,
                &[
                    ("user", &r.name.to_string()),
                    ("recipient", &r.name.to_string()),
                ],
            ),
            _ => tr(keys::GIFT_SEND_ANONYMOUS_SELF).to_string(),
        };
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().flex().justify_center().child(sticker_tile(
                ("auction-send-sticker", gift_id as u64),
                state.gift.sticker.as_ref(),
                &session.media.files,
                &roots,
                96.,
                "\u{1F381}",
            )))
            .child(
                Textarea::new(&self.payments.gift_message_input)
                    .aria_label(tr(keys::GIFT_SEND_MESSAGE)),
            )
            .child(
                Checkbox::new("auction-send-hide-name")
                    .label(tr(keys::GIFT_SEND_ANONYMOUS))
                    .checked(self.gift_market.hide_name)
                    .on_click(cx.listener(|this, &on, _, cx| {
                        this.gift_market.hide_name = on;
                        cx.notify();
                    })),
            )
            .child(hint(hide_about, cx));
        body = body.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_1()
                .child(div().text_sm().child(send_about(state.per_round())))
                .child(
                    Button::new("auction-send-learn")
                        .label(tr(keys::Q_MONEY_LEARN_MORE))
                        .ghost()
                        .xsmall()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.market_push(MarketBox::AuctionAbout { gift_id }, cx);
                        })),
                ),
        );
        let (label, subtitle) = join_button(state, now, true);
        let finished = state.finished(now);
        let action = two_line_button("auction-send-bid", label, subtitle).on_click(cx.listener(
            move |this, _, window, cx| {
                if finished {
                    this.market_back(cx);
                    this.close_kit_dialog_if_done(DialogKind::GiftMarket, window, cx);
                } else {
                    this.market_push(MarketBox::AuctionBid { gift_id }, cx);
                }
            },
        ));
        let title = match recipient {
            Some(r) if !r.is_self => tr_with(
                keys::Q_MONEY_SEND_A_GIFT_TO,
                &[("name", &r.name.to_string())],
            ),
            _ => tr(keys::GIFT_SEND_TITLE).to_string(),
        };
        BoxView::new(title, body).footer(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(self.market_back_button("auction-send-back", tr(keys::Q_MONEY_BACK), cx))
                .child(action),
        )
    }

    pub(super) fn change_recipient_box(&mut self, gift_id: i64, cx: &mut Context<Self>) -> BoxView {
        let Some(session) = self.session() else {
            return BoxView::new(tr(keys::AUCTION_CHANGE_TITLE), div());
        };
        let me = session.my_user_id;
        let name_of = |sender: Option<MessageSender>| -> Option<String> {
            match sender {
                Some(MessageSender::User { user_id }) if Some(user_id) == me => None,
                Some(sender) => Some(session.sender_label(sender)),
                None => None,
            }
        };
        let from = self
            .auction_state(gift_id)
            .and_then(|state| state.my())
            .and_then(|my| name_of(my.owner));
        let to = name_of(self.auction_recipient());
        let text = change_recipient_text(from.as_deref(), to.as_deref());
        BoxView::new(tr(keys::AUCTION_CHANGE_TITLE), div().text_sm().child(text)).footer(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(self.market_back_button("auction-change-cancel", tr(keys::CANCEL), cx))
                .child(
                    Button::new("auction-change")
                        .label(tr(keys::AUCTION_CHANGE_BUTTON))
                        .primary()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.payments
                                .gift_message_input
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            this.market_replace(MarketBox::AuctionSend { gift_id }, cx);
                        })),
                ),
        )
    }

    pub(super) fn auction_bought_box(&mut self, gift_id: i64, cx: &mut Context<Self>) -> BoxView {
        let Some(session) = self.session() else {
            return BoxView::new(tr(keys::Q_REST_ITEMS_BOUGHT), div());
        };
        let market = &session.payments.gift_market;
        let state = market.auctions.get(&gift_id);
        let title = state.map_or_else(
            || tr(keys::PROFILE_ACTION_SHORT_GIFT).into(),
            |s| auction_title(session, s),
        );
        let count = state.map_or(0, AuctionState::acquired);
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_3();
        match market.acquired.get(&gift_id) {
            None => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(keys::Q_MONEY_LOADING)),
                );
            }
            Some(list) => {
                for gift in list {
                    let heading = if gift.number > 0 {
                        format!("{title} #{}", group_digits(i64::from(gift.number)))
                    } else {
                        title.clone()
                    };
                    let mut rows = vec![
                        (
                            tr(keys::GIFT_LINK_LABEL_TO).to_string(),
                            gift.receiver
                                .map(|r| session.sender_label(r))
                                .unwrap_or_default(),
                        ),
                        (
                            tr(keys::GIFT_RESALE_DATE).to_string(),
                            format_unix_date_time(i64::from(gift.date)),
                        ),
                        (
                            tr(keys::AUCTION_BOUGHT_BID).to_string(),
                            format!("\u{2605}{}  #{}", group_digits(gift.stars), gift.position),
                        ),
                    ];
                    if !gift.text.is_empty() {
                        rows.push((tr(keys::Q_MONEY_MESSAGE).to_string(), gift.text.clone()));
                    }
                    body = body
                        .child(div().text_sm().font_semibold().child(tr_with(
                            keys::AUCTION_BOUGHT_IN_ROUND,
                            &[
                                ("name", &heading.to_string()),
                                ("n", &gift.round.to_string()),
                            ],
                        )))
                        .child(super::ton_wallet_details::facts(rows).bordered(true));
                }
            }
        }
        BoxView::new(bought_title(count), body).footer(
            div().flex().justify_end().child(self.market_back_button(
                "auction-bought-close",
                tr(keys::CLOSE),
                cx,
            )),
        )
    }

    pub(super) fn variants_box(&mut self, gift_id: i64, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new(tr(keys::AUCTION_PREVIEW_MODELS_BUTTON), div());
        };
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_3();
        if let Some(preview) = session.payments.gift_market.previews.get(&gift_id) {
            let count = preview.models.len();
            body = body.child(div().text_sm().text_color(muted).child(trn_decimal(
                keys::Q_LEFTOVERS_COLLECTIBLE_MODELS,
                count as i64,
            )));
            let mut grid = div().flex().flex_wrap().gap_2();
            for (ix, model) in preview.models.iter().enumerate() {
                grid = grid.child(
                    div()
                        .w(px(112.))
                        .p_2()
                        .rounded_lg()
                        .bg(cx.theme().secondary)
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(sticker_tile(
                            ("variant-model", ix as u64),
                            model.sticker.as_ref(),
                            &session.media.files,
                            &roots,
                            64.,
                            "\u{1F381}",
                        ))
                        .child(
                            div()
                                .text_xs()
                                .font_semibold()
                                .truncate()
                                .child(model.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(model.rarity.label()),
                        ),
                );
            }
            body = body.child(grid);
        }
        BoxView::new(tr(keys::AUCTION_PREVIEW_MODELS_BUTTON), body)
            .wide()
            .footer(div().flex().justify_end().child(self.market_back_button(
                "variants-close",
                tr(keys::CLOSE),
                cx,
            )))
    }

    /// Raise the bid of one of the active auctions.
    fn raise_active_bid(&mut self, gift_id: i64, cx: &mut Context<Self>) {
        let owner = self
            .auction_state(gift_id)
            .and_then(|state| state.my())
            .and_then(|my| my.owner);
        let recipient = owner.and_then(|sender| match sender {
            MessageSender::User { user_id } => {
                let session = self.session()?;
                let chat = quill::ids::ChatId(user_id);
                session.gift_recipient(chat).or_else(|| {
                    Some(quill::gift_manage::GiftRecipient {
                        sender,
                        name: session.sender_label(sender),
                        is_self: Some(user_id) == session.my_user_id,
                        is_channel: false,
                    })
                })
            }
            MessageSender::Chat { .. } => None,
        });
        self.gift_market.recipient = recipient;
        self.market_push(MarketBox::AuctionBid { gift_id }, cx);
    }

    pub(super) fn active_auctions_box(&mut self, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new(tr(keys::Q_REST_ACTIVE_AUCTIONS), div());
        };
        self.request_animation_tick(1, cx);
        let now = now_unix();
        let list = session.payments.gift_market.active_auctions(now);
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let mut body = div().flex().flex_col().gap_3();
        for state in &list {
            let gift_id = state.gift_id();
            let (round, text, outbid) = active_row(state);
            let countdown = nice_countdown(seconds_left(state.next_round_at(), now));
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(cx.theme().secondary)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(sticker_tile(
                                ("active-auction", gift_id as u64),
                                state.gift.sticker.as_ref(),
                                &session.media.files,
                                &roots,
                                48.,
                                "\u{1F381}",
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_semibold()
                                            .child(auction_title(session, state)),
                                    )
                                    .child(div().text_xs().text_color(muted).child(round))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if outbid {
                                                danger
                                            } else {
                                                success().into()
                                            })
                                            .child(text),
                                    ),
                            ),
                    )
                    .child(
                        Button::new(("active-auction-raise", gift_id as u64))
                            .label(tr_with(
                                keys::Q_MONEY_RAISE_BID,
                                &[("countdown", &countdown.to_string())],
                            ))
                            .primary()
                            .w_full()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.raise_active_bid(gift_id, cx);
                            })),
                    ),
            );
        }
        BoxView::new(
            if list.len() == 1 {
                tr(keys::Q_MONEY_1_ACTIVE_AUCTION).to_string()
            } else {
                tr_with(
                    keys::Q_MONEY_ACTIVE_AUCTIONS,
                    &[("len", &(list.len()).to_string())],
                )
            },
            body,
        )
        .footer(div().flex().justify_end().child(self.market_back_button(
            "active-auctions-close",
            tr(keys::CLOSE),
            cx,
        )))
    }

    /// The chat-list bar while the user has bids in running auctions
    /// (tdesktop `TopBarSuggestion` for gift auctions).
    pub(super) fn gift_auction_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = now_unix();
        auction_bar(
            &self.session()?.payments.gift_market.active_auctions(now),
            now,
        )?;
        self.gift_auction_bar_body(cx)
    }

    /// `gift_auction_bar` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn gift_auction_bar_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let now = now_unix();
        let list = session.payments.gift_market.active_auctions(now);
        let bar = auction_bar(&list, now)?;
        // The countdown on the button.
        self.request_animation_tick(1, cx);
        let theme = cx.theme();
        let text_color = if bar.outbid {
            theme.danger
        } else {
            theme.muted_foreground
        };
        let (border, fill) = (theme.border, theme.accent.opacity(0.1));
        let single = (list.len() == 1)
            .then(|| list.first().map(AuctionState::gift_id))
            .flatten();
        let open = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
            #[cfg(test)]
            {
                this.gift_market.bar_opens = this.gift_market.bar_opens.saturating_add(1);
            }
            match single {
                Some(gift_id) => this.raise_active_bid(gift_id, cx),
                None => this.market_push(MarketBox::ActiveAuctions, cx),
            }
        };
        Some(
            div()
                .id("gift-auction-bar")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(border)
                .bg(fill)
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(format!("{}. {}", bar.title, bar.text))
                .on_click(cx.listener(move |this, _, _, cx| open(this, cx)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(div().text_sm().font_semibold().child(bar.title))
                        .child(div().text_xs().text_color(text_color).child(bar.text)),
                )
                .child(
                    Button::new("gift-auction-bar-button")
                        .label(bar.button)
                        .small()
                        .primary()
                        // The bar's own click would open it a second time.
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            open(this, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

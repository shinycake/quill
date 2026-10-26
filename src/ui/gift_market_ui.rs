//! The gift market dialog: resale listings (`gift_resale_ui.rs`,
//! `gift_resale_buy.rs`) and gift auctions (`gift_auction_ui.rs`,
//! `gift_auction_bid.rs`), after tdesktop's `boxes/star_gift_resale_box.cpp`
//! and `boxes/star_gift_auction_box.cpp`. One kit dialog shows the top box
//! of `GiftMarketUi::stack`; Escape, the close button and "Back" return
//! to the box below.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::fill_muted;
use super::gift_market_state::MarketBox;
use super::premium_ui::sticker_tile;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::gift_market::{AuctionBoxChoice, GiftMarketState, auction_about_shown};
use quill::ids::FileId;
use quill::lang::{keys, tr};
use quill::local_time::now_unix;
use quill::marketplace::GiftPrice;
use quill::premium_hub::group_digits;
use quill::premium_hub::upgraded::Collectible;
use quill::state::Session;
use quill::telegram::envelope::{MessageSender, StickerContent};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Width of the wide boxes (resale grid, bid box).
const WIDE: f32 = 560.;

/// What the top box shows.
pub(super) struct BoxView {
    pub(super) title: SharedString,
    pub(super) body: AnyElement,
    pub(super) footer: Option<AnyElement>,
    /// A control at the right of the title row (tdesktop's top "⋯").
    pub(super) title_action: Option<AnyElement>,
    pub(super) wide: bool,
}

impl BoxView {
    pub(super) fn new(title: impl Into<SharedString>, body: impl IntoElement) -> Self {
        Self {
            title: title.into(),
            body: body.into_any_element(),
            footer: None,
            title_action: None,
            wide: false,
        }
    }

    pub(super) fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    pub(super) fn title_action(mut self, action: impl IntoElement) -> Self {
        self.title_action = Some(action.into_any_element());
        self
    }

    pub(super) fn wide(mut self) -> Self {
        self.wide = true;
        self
    }
}

impl QuillApp {
    /// The market state of the live or demo session.
    pub(super) fn gift_market_mut(&mut self) -> Option<&mut GiftMarketState> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session.payments.gift_market),
            None => self
                .demo_session
                .as_mut()
                .map(|session| &mut session.payments.gift_market),
        }
    }

    /// Opens `next` above the current box.
    pub(super) fn market_push(&mut self, next: MarketBox, cx: &mut Context<Self>) {
        self.gift_market.push(next);
        cx.notify();
    }

    /// Replaces the top box with `next`.
    pub(super) fn market_replace(&mut self, next: MarketBox, cx: &mut Context<Self>) {
        self.gift_market.stack.pop();
        self.gift_market.push(next);
        cx.notify();
    }

    /// Closes the top box ("Back", Escape, the close button).
    pub(super) fn market_back(&mut self, cx: &mut Context<Self>) {
        let popped = self.gift_market.stack.pop();
        self.after_market_pop(popped);
        cx.notify();
    }

    /// A footer button that closes the top box.
    pub(super) fn market_back_button(
        &self,
        id: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .label(label)
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                this.market_back(cx);
                this.close_kit_dialog_if_done(DialogKind::GiftMarket, window, cx);
            }))
    }

    fn after_market_pop(&mut self, popped: Option<MarketBox>) {
        let stack = &self.gift_market.stack;
        let still = |kind: &MarketBox| stack.contains(kind);
        match popped {
            Some(MarketBox::Buy) if !still(&MarketBox::Buy) => {
                if let Some(market) = self.gift_market_mut() {
                    market.buy = None;
                }
            }
            Some(MarketBox::Resale) if !still(&MarketBox::Resale) => {
                if let Some(market) = self.gift_market_mut() {
                    market.resale = None;
                    market.buy = None;
                }
            }
            Some(MarketBox::AuctionBid { .. }) => {
                self.gift_market.slider = None;
                self.gift_market.custom = None;
            }
            _ => {}
        }
        if self.gift_market.stack.is_empty() {
            self.gift_market.recipient = None;
            self.gift_market.pending_auction = false;
        }
        self.after_craft_pop();
    }

    /// Who an auction bid goes to: the chosen recipient, or the user.
    pub(super) fn auction_recipient(&self) -> Option<MessageSender> {
        self.gift_market
            .recipient
            .as_ref()
            .map(|recipient| recipient.sender)
            .or_else(|| {
                let me = self.session()?.my_user_id?;
                Some(MessageSender::User { user_id: me })
            })
    }

    /// Opens the box for the auction of `gift_id` (tdesktop
    /// `ShowStarGiftAuction`): the state is loaded first, then
    /// [`Self::show_auction_box`] picks the box.
    pub(super) fn open_gift_auction(
        &mut self,
        gift_id: i64,
        auction_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.gift_market.stack.is_empty() {
            self.gift_market.recipient = self
                .session()
                .map(|s| &s.payments.gifts_manage.send)
                .filter(|send| send.open)
                .and_then(|send| send.recipient.clone());
        }
        match self.live.as_mut() {
            Some(live) => match live.driver.fetch_gift_auction(auction_id) {
                Ok(_) => self.gift_market.pending_auction = true,
                Err(_) => {
                    self.connection.set_note(StatusNote::error(tr(
                        keys::Q_MONEY_COULDNT_LOAD_THE_AUCTION,
                    )));
                }
            },
            None => self.show_auction_box(gift_id, cx),
        }
        cx.notify();
    }

    /// The box an auction opens with (tdesktop `ChooseAndShowAuctionBox`).
    pub(super) fn show_auction_box(&mut self, gift_id: i64, cx: &mut Context<Self>) {
        let recipient = self.auction_recipient();
        let Some(state) = self
            .session()
            .and_then(|s| s.payments.gift_market.auctions.get(&gift_id))
        else {
            return;
        };
        let Some(recipient) = recipient else {
            return;
        };
        let next = match state.choose_box(&recipient, auction_about_shown(), now_unix()) {
            AuctionBoxChoice::Bid => MarketBox::AuctionBid { gift_id },
            AuctionBoxChoice::ChangeRecipient => MarketBox::ChangeRecipient { gift_id },
            AuctionBoxChoice::Info => MarketBox::AuctionInfo { gift_id },
            AuctionBoxChoice::About => MarketBox::AuctionAbout { gift_id },
        };
        self.gift_market.push(next);
        cx.notify();
    }

    /// Per-frame: open an auction whose state arrived, drop a finished
    /// purchase, follow the shown auction, toasts and sticker downloads.
    pub(in crate::ui) fn gift_market_frame_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gift_market.pending_auction {
            let ready = self
                .gift_market_mut()
                .and_then(|market| market.auction_ready.take());
            let loading = self
                .session()
                .is_some_and(|s| s.payments.gift_market.auction_loading);
            if let Some(gift_id) = ready {
                self.gift_market.pending_auction = false;
                self.show_auction_box(gift_id, cx);
            } else if !loading {
                self.gift_market.pending_auction = false;
            }
        }
        let bought = self.gift_market.top() == Some(&MarketBox::Buy)
            && self
                .session()
                .is_some_and(|s| s.payments.gift_market.buy.is_none());
        if bought {
            self.market_back(cx);
        }
        self.craft_frame_upkeep(cx);
        self.drain_gift_market_notes(window, cx);
        let follow = self
            .gift_market
            .stack
            .iter()
            .rev()
            .find_map(MarketBox::auction);
        let mut files = self.market_sticker_files();
        files.sort_unstable_by_key(|file| file.0);
        files.dedup();
        let fresh = files != self.gift_market.requested_files;
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.follow_gift_auction(follow);
            if fresh {
                for file in &files {
                    let _ = live.driver.download_file(*file, 8);
                }
            }
        }
        if fresh {
            self.gift_market.requested_files = files;
        }
    }

    /// Errors become status toasts, successes kit notifications.
    fn drain_gift_market_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(market) = self.gift_market_mut() else {
            return;
        };
        let notes = std::mem::take(&mut market.notes);
        let toasts = std::mem::take(&mut market.toasts);
        let low = std::mem::take(&mut market.balance_low);
        if let Some(note) = notes.into_iter().last() {
            self.connection.set_note(StatusNote::info(note));
        }
        if low {
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_MONEY_YOUR_STARS_BALANCE_IS_TOO,
            )));
            self.open_stars(cx);
        }
        for (title, text) in toasts {
            cx.defer_in(window, move |_, window, cx| {
                let mut toast = Notification::new().message(text);
                if !title.is_empty() {
                    toast = toast.title(title);
                }
                window.push_notification(toast, cx);
            });
        }
    }

    /// The sticker files the top box and the chat-list bar show.
    fn market_sticker_files(&self) -> Vec<FileId> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let market = &session.payments.gift_market;
        let mut stickers: Vec<&StickerContent> = Vec::new();
        match self.gift_market.top() {
            Some(MarketBox::Resale) => {
                if let Some(search) = market.resale.as_ref() {
                    let attributes = &search.attributes;
                    stickers.extend(
                        search
                            .listings
                            .iter()
                            .filter_map(|listing| listing.gift.model.sticker.as_ref()),
                    );
                    stickers.extend(
                        attributes
                            .models
                            .iter()
                            .chain(attributes.symbols.iter())
                            .filter_map(|entry| entry.item.sticker.as_ref()),
                    );
                }
            }
            Some(MarketBox::Buy) => {
                if let Some(buy) = market.buy.as_ref() {
                    stickers.extend(buy.listing.gift.model.sticker.as_ref());
                }
            }
            Some(MarketBox::Craft | MarketBox::CraftSelect | MarketBox::CraftResult) => {
                stickers.extend(super::gift_craft_ui::craft_stickers(market));
            }
            Some(MarketBox::Variants { gift_id }) => {
                if let Some(preview) = market.previews.get(gift_id) {
                    stickers.extend(
                        preview
                            .models
                            .iter()
                            .filter_map(|model| model.sticker.as_ref()),
                    );
                }
            }
            Some(top) => {
                if let Some(state) = top.auction().and_then(|id| market.auctions.get(&id)) {
                    stickers.extend(state.gift.sticker.as_ref());
                }
            }
            None => {}
        }
        stickers.extend(
            market
                .active
                .iter()
                .filter_map(|state| state.gift.sticker.as_ref()),
        );
        stickers
            .into_iter()
            .filter_map(StickerContent::display_file_id)
            .collect()
    }

    fn build_gift_market_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GiftMarket, |this, _, cx| {
                this.market_back(cx);
            });
        app.update(cx, |this, cx| {
            let Some(top) = this.gift_market.top().cloned() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let view = match top {
                MarketBox::Resale => this.resale_box(cx),
                MarketBox::Buy => this.resale_buy_box(cx),
                MarketBox::AuctionAbout { gift_id } => this.auction_about_box(gift_id, cx),
                MarketBox::AuctionInfo { gift_id } => this.auction_info_box(gift_id, cx),
                MarketBox::AuctionSend { gift_id } => this.auction_send_box(gift_id, cx),
                MarketBox::AuctionBid { gift_id } => this.auction_bid_box(gift_id, cx),
                MarketBox::AuctionBought { gift_id } => this.auction_bought_box(gift_id, cx),
                MarketBox::ChangeRecipient { gift_id } => this.change_recipient_box(gift_id, cx),
                MarketBox::ActiveAuctions => this.active_auctions_box(cx),
                MarketBox::Variants { gift_id } => this.variants_box(gift_id, cx),
                MarketBox::CraftInfo => this.craft_info_box(cx),
                MarketBox::Craft => this.craft_box(cx),
                MarketBox::CraftSelect => this.craft_select_box(cx),
                MarketBox::CraftResult => this.craft_result_box(cx),
                MarketBox::Notice { title, text } => this.notice_box(title, text, cx),
            };
            let body = Rc::new(RefCell::new(Some(view.body)));
            let mut dialog = dialog
                .overlay(true)
                .title(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(crate::ui::shell::dialog_title(view.title))
                        // Clear of the kit's close button in the corner.
                        .when_some(view.title_action, |row, action| {
                            row.child(div().mr_6().child(action))
                        }),
                )
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .on_close(on_close);
            if view.wide {
                dialog = dialog.w(px(WIDE));
            }
            if let Some(footer) = view.footer {
                dialog = dialog.footer(footer);
            }
            dialog
        })
    }
}

/// A collectible on its backdrop: the model sticker and the number.
pub(super) fn collectible_tile(
    id: (&'static str, u64),
    gift: &Collectible,
    session: &Session,
    roots: &[PathBuf],
    side: f32,
) -> Div {
    let backdrop = &gift.backdrop;
    let tile = if backdrop.center == 0 && backdrop.edge == 0 {
        div().bg(fill_muted().opacity(0.35))
    } else {
        div().bg(linear_gradient(
            180.,
            linear_color_stop(rgb(backdrop.center), 0.),
            linear_color_stop(rgb(backdrop.edge), 1.),
        ))
    };
    tile.relative()
        .w(px(side))
        .rounded_lg()
        .overflow_hidden()
        .flex()
        .flex_col()
        .items_center()
        .pt_3()
        .pb_2()
        .gap_1()
        .child(sticker_tile(
            id,
            gift.model.sticker.as_ref(),
            &session.media.files,
            roots,
            side * 0.6,
            "\u{1F381}",
        ))
        .child(
            div()
                .absolute()
                .top_1()
                .left_1p5()
                .text_xs()
                .font_semibold()
                .text_color(gpui_kit::white().opacity(0.85))
                .child(format!("#{}", group_digits(i64::from(gift.number)))),
        )
}

/// One row of an explainer box: an accent icon, a title and its text
/// (tdesktop `MakeFeatureListEntry`).
pub(super) fn feature_row(
    icon: IconName,
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    cx: &App,
) -> Div {
    div()
        .flex()
        .gap_3()
        .child(
            div()
                .pt_0p5()
                .text_color(cx.theme().primary)
                .child(Icon::new(icon).with_size(px(16.))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(div().text_sm().font_semibold().child(title.into()))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(text.into()),
                ),
        )
}

/// "★ 1,250" or the Grams amount, with the currency icon.
pub(super) fn price_line(price: GiftPrice, color: Hsla) -> Div {
    let (icon, amount) = match price {
        GiftPrice::Stars(stars) => (super::earn::star_icon(12.), group_digits(stars)),
        GiftPrice::TonCents(cents) => (
            super::earn::ton_icon(12., color),
            format!("{}.{:02}", group_digits(cents / 100), cents % 100),
        ),
    };
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_sm()
        .font_semibold()
        .text_color(color)
        .child(icon)
        .child(amount)
}

/// tdesktop's two-line box button: the action and, smaller, the time.
pub(super) fn two_line_button(id: &'static str, title: String, subtitle: String) -> Button {
    let button = Button::new(id)
        .primary()
        .accessibility_label(if subtitle.is_empty() {
            title.clone()
        } else {
            format!("{title}, {subtitle}")
        });
    if subtitle.is_empty() {
        return button.label(title);
    }
    button.h(px(44.)).child(
        div()
            .flex()
            .flex_col()
            .items_center()
            .child(div().text_sm().font_semibold().child(title))
            .child(div().text_xs().opacity(0.8).child(subtitle)),
    )
}

crate::ui::shell::register_dialogs! {
    /// Resale listings and gift auctions: one box at a time from
    /// `GiftMarketUi::stack`. Above the gift boxes it opens from; hidden
    /// while the Premium page it links to shows.
    GiftMarket => DialogSpec::new(
        3860,
        |app| {
            !app.gift_market.stack.is_empty()
                && !app.session().is_some_and(|s| s.payments.hub.premium_open)
        },
        QuillApp::build_gift_market_dialog,
    )
    .closes(|app, _, cx| app.market_back(cx)),
}

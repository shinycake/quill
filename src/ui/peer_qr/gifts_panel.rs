//! The gifts panel (tdesktop `PeerQrGiftsPanel`): the upgraded gifts the
//! peer owns as tiles, each its backdrop's gradient with the model on
//! it, the shown one ringed; a spinner while the first page loads; and,
//! with none, an empty state offering to browse gifts. Scrolling near
//! the end of a long list asks for the next page.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::card::hsla;
use super::view_model::QrView;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::gpui::Size as ItemSize;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::peer_qr::gift_tiles::{
    EMPTY_BUTTON, EMPTY_BUTTON_SKIP, EMPTY_PADDING, EMPTY_STICKER, EMPTY_TEXT_SKIP,
    EMPTY_TEXT_WIDTH, EMPTY_TITLE_SKIP, GIFT_RADIUS, GIFT_SKIP, TILE_MODEL_RATIO,
    gifts_panel_height, square_size, tile_rect,
};
use quill::peer_qr::layout::{PANEL_PADDING, panel_height};
use quill::peer_qr::pattern::Rect;
use quill::peer_qr::state::{QrAction, QrPanel, QrPeer};
use quill::peer_qr::theme::{GIFT_COLUMNS, GiftsLayout, ThemeGift, gifts_layout};
use std::rc::Rc;

fn at(value: f64) -> Pixels {
    px(value as f32)
}

/// The selection ring's width and its inset from the tile's edge
/// (tdesktop `peerQrSwatchRing`, drawn inside the rounded tile).
const RING: f64 = 2.;
const RING_INSET: f64 = 4.;

impl QuillApp {
    /// Browse Gifts shows in your own box and in a channel you may send
    /// gifts to (tdesktop `BrowseShownValue`).
    fn qr_gifts_browse(&self) -> Option<ChatId> {
        let session = self.session()?;
        let peer = self
            .peer_qr
            .open
            .as_ref()
            .and_then(|open| match open.state.subject {
                quill::peer_qr::state::QrSubject::Peer(peer) => Some(peer),
                quill::peer_qr::state::QrSubject::Custom(_) => None,
            })?;
        match peer {
            QrPeer::User(user_id) if session.my_user_id == Some(user_id) => Some(ChatId(user_id)),
            QrPeer::User(_) => None,
            QrPeer::Chat(chat_id) => session
                .gift_recipient(ChatId(chat_id))
                .filter(|recipient| recipient.is_channel)
                .map(|_| ChatId(chat_id)),
        }
    }

    /// How tall `panel` opens in a panel `width` wide.
    pub(super) fn qr_panel_height(&self, panel: QrPanel, width: f64) -> f64 {
        match panel {
            QrPanel::Colors | QrPanel::Emoji => panel_height(width),
            QrPanel::Gifts => {
                let Some(session) = self.session() else {
                    return panel_height(width);
                };
                let gifts = &session.users_state.peer_qr.gifts;
                gifts_panel_height(
                    gifts_layout(gifts),
                    width,
                    gifts.list.len(),
                    self.qr_gifts_browse().is_some(),
                )
            }
        }
    }

    /// The panel's content for this frame.
    pub(super) fn qr_gifts_panel(
        &mut self,
        view: &QrView,
        width: f64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let gifts = &session.users_state.peer_qr.gifts;
        let layout = gifts_layout(gifts);
        let list = gifts.list.clone();
        let more = gifts.loaded && !gifts.complete && !gifts.loading;
        let chosen = self
            .peer_qr
            .open
            .as_ref()
            .and_then(|open| open.state.gift.as_ref().map(|gift| gift.id));
        let foreground = hsla(view.colors.foreground);
        match layout {
            GiftsLayout::Loading => div()
                .debug_selector(|| "peer-qr-gifts-loading".into())
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().color(foreground))
                .into_any_element(),
            GiftsLayout::Empty => self.qr_gifts_empty(view, width, cx).into_any_element(),
            GiftsLayout::Wide(_) => div()
                .relative()
                .size_full()
                .children(list.iter().enumerate().map(|(index, gift)| {
                    let rect = tile_rect(layout, width, index);
                    div()
                        .absolute()
                        .left(at(rect.x))
                        .top(at(rect.y))
                        .child(self.qr_gift_tile(gift, index, rect, chosen == Some(gift.id), cx))
                }))
                .into_any_element(),
            GiftsLayout::Grid => self
                .qr_gifts_grid(list, width, chosen, more, cx)
                .into_any_element(),
        }
    }

    /// A long list: rows of three squares, scrolled, the next page asked
    /// for once the last row comes into view (tdesktop `checkLoadMore`).
    fn qr_gifts_grid(
        &mut self,
        list: Vec<ThemeGift>,
        width: f64,
        chosen: Option<i64>,
        more: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let size = square_size(width);
        let rows = list.len().div_ceil(GIFT_COLUMNS);
        let sizes: Rc<Vec<ItemSize<Pixels>>> =
            Rc::new(vec![ItemSize::new(px(0.), at(size + GIFT_SKIP)); rows]);
        let list = Rc::new(list);
        v_virtual_list(
            cx.entity(),
            "peer-qr-gifts-grid",
            sizes,
            move |this: &mut QuillApp, range, _window, cx| {
                if more && range.end >= rows {
                    let app = cx.entity();
                    cx.defer(move |cx| {
                        app.update(cx, |this, cx| this.qr_act(QrAction::MoreGifts, cx));
                    });
                }
                range
                    .map(|row| {
                        let tiles = list
                            .iter()
                            .enumerate()
                            .skip(row * GIFT_COLUMNS)
                            .take(GIFT_COLUMNS)
                            .map(|(index, gift)| {
                                let rect = Rect::new(0., 0., size, size);
                                this.qr_gift_tile(gift, index, rect, chosen == Some(gift.id), cx)
                            })
                            .collect::<Vec<_>>();
                        div()
                            .px(at(PANEL_PADDING))
                            .flex()
                            .gap(at(GIFT_SKIP))
                            .children(tiles)
                    })
                    .collect::<Vec<_>>()
            },
        )
        .pt(at(PANEL_PADDING))
        .size_full()
    }

    /// One tile: the gift's backdrop as a rounded radial gradient, the
    /// model at 82% of its height, a white ring when it is the shown gift.
    fn qr_gift_tile(
        &mut self,
        gift: &ThemeGift,
        index: usize,
        rect: Rect,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let (width, height) = (rect.width.round(), rect.height.round());
        let background = self.qr_gift_tile_image(
            gift.center,
            gift.edge,
            width.max(1.) as u32,
            height.max(1.) as u32,
            cx,
        );
        let model = self.qr_gift_model(gift).map(|(image, _)| image);
        let box_side = TILE_MODEL_RATIO * height;
        let picked = gift.clone();
        let face = div()
            .relative()
            .w(at(width))
            .h(at(height))
            .rounded(at(GIFT_RADIUS))
            .bg(hsla(gift.edge))
            .overflow_hidden()
            .children(background.map(|image| {
                img(ImageSource::from(image))
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(at(width))
                    .h(at(height))
                    .rounded(at(GIFT_RADIUS))
            }))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(model.map(|image| {
                        img(ImageSource::from(image))
                            .size(at(box_side))
                            .object_fit(ObjectFit::Contain)
                    })),
            )
            .when(selected, |face| {
                face.child(
                    div()
                        .absolute()
                        .top(at(RING_INSET))
                        .left(at(RING_INSET))
                        .w(at(width - 2. * RING_INSET))
                        .h(at(height - 2. * RING_INSET))
                        .rounded(at(GIFT_RADIUS - RING_INSET))
                        .border(at(RING))
                        .border_color(gpui_kit::white()),
                )
            });
        div()
            .debug_selector(move || format!("peer-qr-gift-{index}"))
            .child(
                Button::new(("peer-qr-gift", index as u64))
                    .ghost()
                    .p_0()
                    .w(at(width))
                    .h(at(height))
                    .rounded(at(GIFT_RADIUS))
                    .accessibility_label(gift.name.clone())
                    .tooltip(gift.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.qr_act(QrAction::PickGift(Box::new(picked.clone())), cx);
                    }))
                    .child(face),
            )
    }

    /// No gifts: a gift, "No gifts yet", what gifts do here, and Browse
    /// Gifts where gifts can be sent (tdesktop `layoutEmpty`).
    fn qr_gifts_empty(&mut self, view: &QrView, width: f64, cx: &mut Context<Self>) -> Div {
        let foreground = hsla(view.colors.foreground);
        let (left, top, right, bottom) = EMPTY_PADDING;
        let browse = self.qr_gifts_browse();
        div()
            .debug_selector(|| "peer-qr-gifts-empty".into())
            .w(at(width))
            .pt(at(top))
            .pb(at(bottom))
            .pl(at(left))
            .pr(at(right))
            .flex()
            .flex_col()
            .items_center()
            .text_color(foreground)
            .child(
                div()
                    .size(at(EMPTY_STICKER))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::Gift).with_size(at(EMPTY_STICKER * 0.6))),
            )
            .child(
                div()
                    .mt(at(EMPTY_TITLE_SKIP))
                    .text_size(px(15.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_center()
                    .child(tr(keys::QR_BOX_GIFTS_EMPTY_TITLE)),
            )
            .child(
                div()
                    .mt(at(EMPTY_TEXT_SKIP))
                    .w(at(EMPTY_TEXT_WIDTH.min(width - 2. * PANEL_PADDING)))
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .text_center()
                    .text_color(foreground.opacity(0.75))
                    .child(tr(keys::QR_BOX_GIFTS_EMPTY_ABOUT)),
            )
            .children(browse.map(|chat| {
                div()
                    .mt(at(EMPTY_BUTTON_SKIP))
                    .debug_selector(|| "peer-qr-gifts-browse".into())
                    .child(
                        Button::new("peer-qr-gifts-browse")
                            .primary()
                            .rounded_full()
                            .h(at(EMPTY_BUTTON))
                            .px_5()
                            .label(tr(keys::QR_BOX_GIFTS_BROWSE))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_send_gift_for(chat, window, cx);
                            })),
                    )
            }))
    }
}

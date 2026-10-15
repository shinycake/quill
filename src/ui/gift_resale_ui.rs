//! The resale box: the listings of one collection with the sort and
//! attribute filter pills (tdesktop `boxes/star_gift_resale_box.cpp`,
//! `GiftResaleBox`). A listing opens the buy box (`gift_resale_buy.rs`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::gift_market_state::MarketBox;
use super::gift_market_ui::{BoxView, collectible_tile, price_line};
use super::image_budget::{Fit, sized_media};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_market::{
    AttributeId, AttributeKind, ResaleFilter, ResaleSort, listing_price, resale_count_label,
};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::telegram::envelope::StickerContent;
use std::path::PathBuf;

/// Grid tile width in the resale box.
const TILE: f32 = 124.;

/// One row of an attribute menu, captured for the menu builder.
#[derive(Clone)]
struct AttributeRow {
    id: AttributeId,
    name: String,
    count: i32,
    /// The sticker image for models and symbols.
    sticker: Option<(PathBuf, Option<(i32, i32)>)>,
    /// The center color for backdrops.
    color: Option<u32>,
}

/// The sticker file on disk, when downloaded and inside the media roots.
pub(super) fn sticker_file(
    sticker: Option<&StickerContent>,
    session: &Session,
    roots: &[PathBuf],
) -> Option<(PathBuf, Option<(i32, i32)>)> {
    let sticker = sticker?;
    [sticker.display_file_id(), sticker.thumb_file_id]
        .into_iter()
        .flatten()
        .filter_map(|id| session.media.files.get(&id.0)?.usable_path())
        .find_map(|path| sandboxed_display_path(path, roots))
        .map(|path| (path, Some((sticker.width, sticker.height))))
}

/// A small sticker (or a gift glyph) for a menu row.
fn menu_icon(row: &AttributeRow) -> AnyElement {
    let frame = div()
        .size(px(20.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    match (&row.sticker, row.color) {
        (Some((path, dims)), _) => frame
            .child(
                img(sized_media(path, (px(20.), px(20.)), *dims, Fit::Contain))
                    .size(px(20.))
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
        (None, Some(color)) => frame
            .child(div().size(px(14.)).rounded_full().bg(rgb(color)))
            .into_any_element(),
        (None, None) => frame.text_xs().child("\u{1F381}").into_any_element(),
    }
}

impl QuillApp {
    /// Opens the listings of `gift_id` (the send box's "resale" tile).
    pub(super) fn open_gift_resale(&mut self, gift_id: i64, title: &str, cx: &mut Context<Self>) {
        let opened = match self.live.as_mut() {
            Some(live) => live.driver.open_gift_resale(gift_id, title).is_ok(),
            None => self
                .session()
                .and_then(|s| s.payments.gift_market.resale.as_ref())
                .is_some_and(|search| search.gift_id == gift_id),
        };
        if opened {
            if self.gift_market.stack.is_empty() {
                self.gift_market.recipient = self
                    .session()
                    .and_then(|s| s.payments.gifts_manage.send.recipient.clone());
            }
            self.gift_market.push(MarketBox::Resale);
        } else {
            self.connection.status_note = tr(keys::Q_MONEY_COULDNT_LOAD_THE_GIFTS_ON).into();
        }
        cx.notify();
    }

    fn set_resale_filter(&mut self, filter: ResaleFilter, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                let _ = live.driver.set_gift_resale_filter(filter);
            }
            None => {
                if let Some(search) = self.gift_market_mut().and_then(|m| m.resale.as_mut()) {
                    search.filter = filter;
                }
            }
        }
        cx.notify();
    }

    /// Changes the filter of the open resale list with `change`.
    pub(super) fn edit_resale_filter(
        &mut self,
        change: impl FnOnce(&mut ResaleFilter),
        cx: &mut Context<Self>,
    ) {
        let Some(mut filter) = self
            .session()
            .and_then(|s| s.payments.gift_market.resale.as_ref())
            .map(|search| search.filter.clone())
        else {
            return;
        };
        change(&mut filter);
        self.set_resale_filter(filter, cx);
    }

    fn more_resale(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_gift_resale(false);
        }
        cx.notify();
    }

    /// The chat the market buys for: the send box's chat, or none for the
    /// user.
    fn market_recipient_chat(&self) -> Option<ChatId> {
        let recipient = self.gift_market.recipient.as_ref()?;
        if recipient.is_self {
            return None;
        }
        self.session().and_then(|s| s.open_chat)
    }

    pub(super) fn pick_resale_listing(&mut self, name: String, cx: &mut Context<Self>) {
        let chat = self.market_recipient_chat();
        let started = match self.live.as_mut() {
            Some(live) => live
                .driver
                .start_gift_resale_buy(&name, chat, false)
                .is_ok(),
            None => {
                let recipient = self.gift_market.recipient.clone();
                let me = self.session().and_then(|s| s.my_user_id).unwrap_or(0);
                self.gift_market_mut().is_some_and(|market| {
                    let listing = market
                        .resale
                        .as_ref()
                        .and_then(|s| s.listings.iter().find(|l| l.gift.name == name))
                        .cloned();
                    let recipient = recipient.unwrap_or(quill::gift_manage::GiftRecipient {
                        sender: quill::telegram::envelope::MessageSender::User { user_id: me },
                        name: String::new(),
                        is_self: true,
                        is_channel: false,
                    });
                    listing.is_some_and(|listing| {
                        market.buy = Some(quill::gift_market::ResaleBuy::new(
                            listing, recipient, false,
                        ));
                        true
                    })
                })
            }
        };
        if started {
            self.payments.gift_hide_name = false;
            self.gift_market.push(MarketBox::Buy);
        }
        cx.notify();
    }

    pub(super) fn resale_box(&mut self, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new("Gifts on sale", div());
        };
        let Some(search) = session.payments.gift_market.resale.as_ref() else {
            return BoxView::new("Gifts on sale", div());
        };
        let title = session
            .gift_market_title(search.gift_id)
            .unwrap_or_else(|| tr(keys::Q_MONEY_GIFTS_ON_SALE).into());
        let muted = cx.theme().muted_foreground;
        let filter = search.filter.clone();
        let mut body = div().flex().flex_col().gap_3().child(
            div()
                .text_sm()
                .text_color(muted)
                .child(resale_count_label(search.total_count)),
        );
        body = body.child(self.resale_pills(session, &filter, &roots, cx));
        if let Some(error) = search.error.as_ref() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        if search.loaded && search.listings.is_empty() && !search.loading {
            body = body.child(
                div()
                    .py_6()
                    .text_sm()
                    .text_center()
                    .text_color(muted)
                    .child(tr(keys::GIFT_RESALE_SEARCH_NONE)),
            );
        }
        let mut grid = div().flex().flex_wrap().gap_2();
        for listing in &search.listings {
            let gift = &listing.gift;
            let name = gift.name.clone();
            let label = format!(
                "{}, {}",
                gift.full_name(),
                listing_price(gift, false)
                    .map(|price| price.label())
                    .unwrap_or_default()
            );
            let price =
                listing_price(gift, false).map(|price| price_line(price, gpui_kit::white()));
            let tile =
                collectible_tile(("resale-tile", gift.id as u64), gift, session, &roots, TILE)
                    .children(price.map(|price| {
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(gpui_kit::black().opacity(0.25))
                            .child(price)
                    }));
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("resale-{name}")))
                    .rounded_lg()
                    .cursor_pointer()
                    .tab_index(0)
                    .role(Role::Button)
                    .aria_label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_resale_listing(name.clone(), cx);
                    }))
                    .child(tile),
            );
        }
        body = body.child(grid);
        if search.loading {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_LOADING)),
            );
        } else if search.has_more() {
            body = body.child(
                Button::new("resale-more")
                    .label(tr(keys::Q_MONEY_SHOW_MORE))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_resale(cx))),
            );
        }
        let stars_only = filter.stars_only;
        let footer = div()
            .flex()
            .justify_between()
            .gap_2()
            .child(
                Button::new("resale-stars-only")
                    .label(if stars_only {
                        tr(keys::GIFT_RESALE_ALL_LISTINGS)
                    } else {
                        tr(keys::GIFT_RESALE_STARS_ONLY)
                    })
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_resale_filter(|f| f.stars_only = !stars_only, cx);
                    })),
            )
            .child(self.market_back_button("resale-back", tr(keys::Q_MONEY_BACK), cx));
        BoxView::new(title, body).footer(footer).wide()
    }

    /// The sort pill and the three attribute pills.
    fn resale_pills(
        &self,
        session: &Session,
        filter: &ResaleFilter,
        roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> Div {
        let owner = cx.entity().downgrade();
        let sort = filter.sort;
        let stars_only = filter.stars_only;
        let sort_owner = owner.clone();
        let sort_pill = Button::new("resale-sort")
            .label(sort.label())
            .icon(IconName::SortAscending)
            .secondary()
            .small()
            .dropdown_caret(true)
            .accessibility_label(sort.menu_label())
            .dropdown_menu(move |mut menu, _, _| {
                for choice in ResaleSort::ALL {
                    let owner = sort_owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(choice.menu_label())
                            .checked(choice == sort)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.edit_resale_filter(|f| f.sort = choice, cx);
                                });
                            }),
                    );
                }
                menu = menu.separator();
                for (label, only) in [
                    (tr(keys::GIFT_RESALE_ALL_LISTINGS), false),
                    (tr(keys::GIFT_RESALE_STARS_ONLY), true),
                ] {
                    let owner = sort_owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(only == stars_only)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.edit_resale_filter(|f| f.stars_only = only, cx);
                                });
                            }),
                    );
                }
                menu
            });
        let mut row = div().flex().flex_wrap().gap_2().child(sort_pill);
        let Some(search) = session.payments.gift_market.resale.as_ref() else {
            return row;
        };
        let attributes = &search.attributes;
        for kind in AttributeKind::ALL {
            let rows: Vec<AttributeRow> = match kind {
                AttributeKind::Model => attributes
                    .models
                    .iter()
                    .map(|entry| AttributeRow {
                        id: AttributeId::Model(entry.item.sticker_id),
                        name: entry.item.name.clone(),
                        count: entry.count,
                        sticker: sticker_file(entry.item.sticker.as_ref(), session, roots),
                        color: None,
                    })
                    .collect(),
                AttributeKind::Backdrop => attributes
                    .backdrops
                    .iter()
                    .map(|entry| AttributeRow {
                        id: AttributeId::Backdrop(entry.item.id),
                        name: entry.item.name.clone(),
                        count: entry.count,
                        sticker: None,
                        color: Some(entry.item.center),
                    })
                    .collect(),
                AttributeKind::Symbol => attributes
                    .symbols
                    .iter()
                    .map(|entry| AttributeRow {
                        id: AttributeId::Symbol(entry.item.sticker_id),
                        name: entry.item.name.clone(),
                        count: entry.count,
                        sticker: sticker_file(entry.item.sticker.as_ref(), session, roots),
                        color: None,
                    })
                    .collect(),
            };
            if rows.is_empty() {
                continue;
            }
            row = row.child(attribute_pill(kind, rows, filter.clone(), owner.clone()));
        }
        row
    }
}

/// "Model" / "2 Models" with its checklist menu.
fn attribute_pill(
    kind: AttributeKind,
    rows: Vec<AttributeRow>,
    filter: ResaleFilter,
    owner: WeakEntity<QuillApp>,
) -> impl IntoElement {
    let selected = filter.selected(kind);
    let id = match kind {
        AttributeKind::Model => "resale-models",
        AttributeKind::Backdrop => "resale-backdrops",
        AttributeKind::Symbol => "resale-symbols",
    };
    Button::new(id)
        .label(kind.tab_label(selected))
        .secondary()
        .small()
        .dropdown_caret(true)
        .dropdown_menu(move |mut menu: PopupMenu, _, _| {
            menu = menu.max_h(px(360.)).scrollable(true);
            if selected > 0 {
                let owner = owner.clone();
                menu = menu
                    .item(
                        PopupMenuItem::new(tr(keys::GIFT_RESALE_FILTER_ALL)).on_click(
                            move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.edit_resale_filter(|f| f.select_all(kind), cx);
                                });
                            },
                        ),
                    )
                    .separator();
            }
            for row in &rows {
                let owner = owner.clone();
                let id = row.id;
                let shown = row.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(menu_icon(&shown))
                            .child(div().child(shown.name.clone()))
                            .child(
                                div()
                                    .font_semibold()
                                    .child(group_digits(i64::from(shown.count))),
                            )
                    })
                    .checked(filter.is_checked(id))
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.edit_resale_filter(|f| f.toggle(id), cx);
                        });
                    }),
                );
            }
            menu
        })
}

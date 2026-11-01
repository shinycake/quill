//! The value of an upgraded gift (tdesktop `UniqueGiftValueBox`,
//! `settings/settings_credits_graphics.cpp`, and the "Value" row of the
//! unique gift box, `MakeUniqueGiftValueValue` in
//! `boxes/gift_premium_box.cpp`).
//!
//! The row shows the estimate ("~$2,487.50") with a "learn more" pill; the
//! pill asks `getUpgradedGiftValueInfo` and spins until the answer opens
//! the box. A failure shows under the row. The box: the gift's sticker,
//! the price in an accent pill, why it is that price, the sale table and
//! the "for sale on Telegram / Fragment" links.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::entity_links::OpenLinkConfirm;
use super::premium_ui::sticker_tile;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::link_policy::{OpenDecision, open_decision};
use quill::premium_hub::gift_value::{
    Availability, GiftValueLookup, Platform, ValueRow, about_text, availability,
    format_value_price, fragment_link, row_value, table_rows,
};
use quill::premium_hub::upgraded::Collectible;
use quill::state::Session;
use quill::telegram::envelope::StickerContent;
use quill::text::LinkTarget;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// tdesktop `boxWideWidth`-like width of the value box.
const BOX_WIDTH: f32 = 420.;
/// The gift's sticker at the top of the box.
const STICKER: f32 = 96.;

/// The "Value" row of the unique gift box: the estimate and "learn more",
/// a spinner on the pill while the value loads, the error under it when
/// the request failed. `None` when the gift has no estimate (tdesktop
/// shows the row only then).
pub(super) fn gift_value_row(
    session: &Session,
    gift: &Collectible,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let value = row_value(gift)?;
    let lookup = session
        .payments
        .hub
        .gift_value
        .as_ref()
        .filter(|lookup| lookup.name == gift.name);
    let loading = lookup.is_some_and(|lookup| lookup.loading);
    let error = lookup.and_then(|lookup| lookup.error.clone());
    let muted = cx.theme().muted_foreground;
    let target = GiftValueLookup::new(gift);
    let learn_more = Button::new("gift-value-learn-more")
        .label(tr(keys::GIFT_UNIQUE_VALUE_LEARN_MORE))
        .xsmall()
        .secondary()
        .rounded(px(10.))
        .loading(loading)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.open_gift_value(target.clone(), cx);
        }));
    let row = div()
        .id("gift-value-row")
        .debug_selector(|| "gift-value-row".to_string())
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .text_sm()
                .child(div().text_color(muted).child(tr(keys::GIFT_UNIQUE_VALUE)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().font_medium().child(value))
                        // The pill is inert while loading; the spinner says why.
                        .when(loading, |cell| {
                            cell.child(
                                div()
                                    .debug_selector(|| "gift-value-spinner".to_string())
                                    .child(gpui_kit::component::spinner::Spinner::new().xsmall()),
                            )
                        })
                        .child(
                            div()
                                .debug_selector(|| "gift-value-learn-more".to_string())
                                .child(learn_more),
                        ),
                ),
        )
        .when_some(error, |row, error| {
            row.child(
                div()
                    .id("gift-value-error")
                    .debug_selector(|| "gift-value-error".to_string())
                    .text_xs()
                    .text_right()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
        });
    Some(row.into_any_element())
}

/// `text` with `bold` (its first occurrence) in bold.
fn with_bold(text: String, bold: &str) -> StyledText {
    let highlights = match text.find(bold).filter(|_| !bold.is_empty()) {
        Some(start) => vec![(
            start..start.saturating_add(bold.len()),
            HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            },
        )],
        None => Vec::new(),
    };
    StyledText::new(text).with_highlights(highlights)
}

impl QuillApp {
    fn gift_value_mut(&mut self) -> Option<&mut Option<GiftValueLookup>> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session.payments.hub.gift_value),
            None => self
                .demo_session
                .as_mut()
                .map(|session| &mut session.payments.hub.gift_value),
        }
    }

    /// "learn more": ask for the value (tdesktop asks once per box and
    /// reopens from what it has). Offline (demos), a known value reopens
    /// and anything else just shows the loading pill.
    pub(super) fn open_gift_value(&mut self, lookup: GiftValueLookup, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                let _ = live.driver.request_gift_value(lookup);
            }
            None => {
                if let Some(slot) = self.gift_value_mut() {
                    match slot.as_mut() {
                        Some(known) if known.name == lookup.name && known.info.is_some() => {
                            known.open = true;
                        }
                        _ => {
                            *slot = Some(GiftValueLookup {
                                loading: true,
                                ..lookup
                            })
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// Closes the box; the value stays known for the next "learn more".
    pub(super) fn close_gift_value(&mut self, cx: &mut Context<Self>) {
        if let Some(Some(lookup)) = self.gift_value_mut() {
            lookup.open = false;
        }
        cx.notify();
    }

    /// "{count} for sale on Telegram": the resale list of the gift, in
    /// place of this box (tdesktop `ShowStarGiftResale`).
    fn gift_value_open_resale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self
            .session()
            .and_then(|s| s.payments.hub.gift_value.as_ref())
            .map(|lookup| (lookup.regular_gift_id, lookup.title.clone()));
        self.close_gift_value(cx);
        self.close_kit_dialog_if_done(DialogKind::GiftValue, window, cx);
        if let Some((gift_id, title)) = target {
            self.open_gift_resale(gift_id, &title, cx);
        }
    }

    /// "{count} for sale on Fragment": Fragment's listings in the browser,
    /// https only and through the link policy.
    fn gift_value_open_fragment(&mut self, cx: &mut Context<Self>) {
        let Some(url) = self
            .session()
            .and_then(|s| s.payments.hub.gift_value.as_ref())
            .and_then(|lookup| lookup.info.as_ref())
            .and_then(|info| fragment_link(info).map(str::to_string))
        else {
            return;
        };
        let link = LinkTarget::Url {
            url: url.clone(),
            label: None,
        };
        match open_decision(&link) {
            OpenDecision::Open => self.open_link(&url, cx),
            // A look-alike address asks first, in the same "Open this
            // link?" box a message link uses; its Open button opens it.
            OpenDecision::Confirm {
                url,
                shown,
                suspicious,
            } => {
                self.message_ui.open_link_confirm = Some(OpenLinkConfirm {
                    url,
                    shown,
                    suspicious,
                    from_poll_option: false,
                });
                cx.notify();
            }
        }
    }

    fn build_gift_value_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GiftValue, |this, _, cx| {
                this.close_gift_value(cx);
            });
        app.update(cx, |this, cx| {
            let roots = this.media_display_roots();
            let Some(session) = this.session() else {
                return dialog.on_close(on_close);
            };
            let Some(lookup) = session.payments.hub.gift_value.clone() else {
                return dialog.on_close(on_close);
            };
            let Some(info) = lookup.info.clone() else {
                return dialog.on_close(on_close);
            };
            let theme = cx.theme();
            let (muted, primary) = (theme.muted_foreground, theme.primary);

            // tdesktop's accent pill (`uniqueGiftValuePrice`): a kit Tag,
            // like the change pill in the table.
            let price = div()
                .id("gift-value-price")
                .debug_selector(|| "gift-value-price".to_string())
                .child(
                    Tag::primary()
                        .rounded_full()
                        .px_4()
                        .py_1()
                        .text_2xl()
                        .font_semibold()
                        .child(format_value_price(info.value, &info.currency, false)),
                );
            let (about, bold) = about_text(&info, &lookup.title, &lookup.full_name);
            let about = div()
                .id("gift-value-about")
                .debug_selector(|| "gift-value-about".to_string())
                .text_sm()
                .text_center()
                .text_color(muted)
                .child(with_bold(about, &bold));

            let rows = table_rows(&info, &lookup.title);
            let table = rows.into_iter().fold(
                DescriptionList::horizontal()
                    .columns(1)
                    .bordered(true)
                    .small()
                    .label_width(px(130.)),
                |list, row| {
                    let label = SharedString::from(row.label.clone());
                    list.item(label, value_cell(row, cx), 1)
                },
            );

            let owner = cx.entity().downgrade();
            let links = availability(&info).into_iter().map(|link| {
                availability_button(
                    link,
                    lookup.sticker.as_ref(),
                    session,
                    &roots,
                    owner.clone(),
                    primary,
                )
            });

            let body = div()
                .id("gift-value-box")
                .debug_selector(|| "gift-value-box".to_string())
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .pt_2()
                .child(sticker_tile(
                    ("gift-value-sticker", 0),
                    lookup.sticker.as_ref(),
                    &session.media.files,
                    &roots,
                    STICKER,
                    "\u{1F381}",
                ))
                .child(price)
                .child(about)
                .child(div().w_full().child(table))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .children(links),
                )
                .into_any_element();
            let body = Rc::new(RefCell::new(Some(body)));
            let ok_app = owner.clone();
            dialog
                .overlay(true)
                .w(px(BOX_WIDTH))
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(
                    div().flex().justify_end().w_full().child(
                        div().debug_selector(|| "gift-value-ok".to_string()).child(
                            Button::new("gift-value-ok")
                                .label(tr(keys::BOX_OK))
                                .primary()
                                .on_click(move |_, window, cx| {
                                    let _ = ok_app.update(cx, |this, cx| {
                                        this.close_gift_value(cx);
                                        this.close_kit_dialog_if_done(
                                            DialogKind::GiftValue,
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        ),
                    ),
                )
                .on_close(on_close)
        })
    }
}

/// The value side of a table row: the Star sign before the initial
/// price, the change pill after the last price, the info mark with its
/// explanation after the minimum and average prices.
fn value_cell(row: ValueRow, cx: &App) -> AnyElement {
    let id = row.kind.id();
    div()
        .flex()
        .items_center()
        .gap_1p5()
        .when(row.stars_icon, |cell| {
            cell.child(super::earn::star_icon(12.))
        })
        .child(div().child(row.value))
        .when_some(row.change, |cell, change| {
            cell.child(Tag::secondary().small().rounded_full().child(change))
        })
        .when_some(row.tooltip, |cell, tip| {
            let tip = SharedString::from(tip);
            cell.child(
                div()
                    .id(SharedString::from(format!("gift-value-tip-{id}")))
                    .debug_selector(move || format!("gift-value-tip-{id}"))
                    .text_color(cx.theme().muted_foreground)
                    .child(Icon::new(IconName::Info).with_size(px(14.)))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx)),
            )
        })
        .into_any_element()
}

/// "{count} {emoji} for sale on {platform} {arrow}" as a quiet button:
/// the gift's sticker for `{emoji}`, a chevron for `{arrow}`.
fn availability_button(
    link: Availability,
    sticker: Option<&StickerContent>,
    session: &Session,
    roots: &[PathBuf],
    owner: WeakEntity<QuillApp>,
    color: Hsla,
) -> Div {
    let (id, platform) = match link.platform {
        Platform::Telegram => ("gift-value-on-telegram", Platform::Telegram),
        Platform::Fragment => ("gift-value-on-fragment", Platform::Fragment),
    };
    let label = link
        .text
        .replace("{emoji}", "")
        .replace("{arrow}", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut content = div().flex().items_center().gap_1().text_color(color);
    let mut rest = link.text.as_str();
    while !rest.is_empty() {
        let next = [("{emoji}", 0), ("{arrow}", 1)]
            .into_iter()
            .filter_map(|(tag, which)| rest.find(tag).map(|at| (at, tag, which)))
            .min_by_key(|(at, _, _)| *at);
        let Some((at, tag, which)) = next else {
            content = content.child(rest.trim().to_string());
            break;
        };
        let before = rest.get(..at).unwrap_or_default().trim();
        if !before.is_empty() {
            content = content.child(before.to_string());
        }
        content = if which == 0 {
            content.child(sticker_tile(
                (id, 1),
                sticker,
                &session.media.files,
                roots,
                18.,
                "\u{1F381}",
            ))
        } else {
            content.child(Icon::new(IconName::ChevronRight).with_size(px(14.)))
        };
        rest = rest.get(at.saturating_add(tag.len())..).unwrap_or_default();
    }
    let button = Button::new(id)
        .ghost()
        .accessibility_label(label)
        .child(content)
        .on_click(move |_, window, cx| {
            let _ = owner.update(cx, |this, cx| match platform {
                Platform::Telegram => this.gift_value_open_resale(window, cx),
                Platform::Fragment => this.gift_value_open_fragment(cx),
            });
        });
    div().debug_selector(move || id.to_string()).child(button)
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "gift_value_ui_tests.rs"]
mod tests;

crate::ui::shell::register_dialogs! {
    /// The value of an upgraded gift, above the gift box or the market box
    /// it was opened from.
    GiftValue => DialogSpec::new(
        3858,
        |app| {
            app.session().is_some_and(|s| {
                s.payments
                    .hub
                    .gift_value
                    .as_ref()
                    .is_some_and(|lookup| lookup.open && lookup.info.is_some())
            })
        },
        QuillApp::build_gift_value_dialog,
    )
    .closes(|app, _, cx| app.close_gift_value(cx)),
}

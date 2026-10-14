//! Gift crafting (tdesktop `boxes/star_gift_craft_box.cpp`,
//! `star_gift_craft_animation.cpp`): "Gift Crafting" explains it, "Craft
//! Gift" holds up to four collectibles of one collection, the forge runs
//! while `craftGift` is answered, then the new gift or "Crafting Failed".
//! "Select Gifts" is in `gift_craft_select.rs`. Only the craft button
//! spends anything: the used gifts are gone either way.

use super::app::QuillApp;
use super::gift_market_state::MarketBox;
use super::gift_market_ui::{BoxView, collectible_tile, feature_row, two_line_button};
use super::gift_resale_buy::attribute_row;
use super::shell::DialogKind;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::gift_market::{
    CRAFT_SLOTS, CraftGate, CraftGift, CraftResult, CraftSession, GiftMarketState, ResaleFilter,
    ResaleSearch, attribute_chances, craft_colors, craft_failed_text, format_percent,
    shows_craftable_link,
};
use quill::local_time::{civil_local, hhmm, month_name, now_unix};
use quill::premium_hub::ReceivedGift;
use quill::telegram::envelope::StickerContent;
use std::time::Instant;

/// The forge runs at least this long, like tdesktop's animation.
pub(super) const FORGE_SECONDS: f32 = 3.;
/// A craft slot's side.
const SLOT: f32 = 112.;

/// The owned collectible behind a gift page, when it can be crafted.
pub(super) fn craft_gift_of(gift: &ReceivedGift) -> Option<CraftGift> {
    let collectible = gift.gift.collectible.as_deref()?;
    (collectible.craft_per_mille > 0 && !collectible.is_burned).then(|| CraftGift {
        gift: collectible.clone(),
        received_gift_id: gift.id.clone(),
        craft_date: gift.manage.craft_date,
    })
}

/// tdesktop `langDayOfMonthShort` and the short time: ("Oct 14", "15:10").
fn craft_when(unix: i32) -> (String, String) {
    let time = civil_local(i64::from(unix));
    let month: String = month_name(time.month).chars().take(3).collect();
    (format!("{month} {}", time.day), hhmm(&time))
}

/// The "Crafting unavailable" text for a gate, if it blocks.
pub(super) fn gate_notice(gate: CraftGate) -> Option<MarketBox> {
    gate.message(craft_when).map(|text| MarketBox::Notice {
        title: "Crafting unavailable".into(),
        text,
    })
}

/// `text` with `bold` in bold, wherever it appears.
fn with_bold(text: String, bold: &[&str]) -> StyledText {
    let mut highlights = Vec::new();
    for part in bold.iter().filter(|part| !part.is_empty()) {
        if let Some(start) = text.find(part) {
            highlights.push((
                start..start.saturating_add(part.len()),
                HighlightStyle {
                    font_weight: Some(FontWeight::BOLD),
                    ..Default::default()
                },
            ));
        }
    }
    highlights.sort_by_key(|(range, _)| range.start);
    StyledText::new(text).with_highlights(highlights)
}

/// The sticker files the craft boxes show.
pub(super) fn craft_stickers(market: &GiftMarketState) -> Vec<&StickerContent> {
    let Some(craft) = market.craft.as_ref() else {
        return Vec::new();
    };
    let mut stickers: Vec<&StickerContent> = craft
        .slots
        .iter()
        .chain(craft.candidates.iter())
        .filter_map(|gift| gift.gift.model.sticker.as_ref())
        .collect();
    stickers.extend(
        craft
            .market
            .listings
            .iter()
            .filter_map(|listing| listing.gift.model.sticker.as_ref()),
    );
    if let Some(CraftResult::Success { gift, .. }) = craft.result.as_ref() {
        stickers.extend(gift.model.sticker.as_ref());
    }
    stickers
}

/// The dark panel the craft box draws on; its colors follow the slots.
fn craft_panel(count: usize) -> Div {
    let (center, edge, _, _) = craft_colors(count);
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_3()
        .p_4()
        .rounded_lg()
        .text_color(gpui_kit::white())
        .bg(linear_gradient(
            180.,
            linear_color_stop(rgb(center), 0.),
            linear_color_stop(rgb(edge), 1.),
        ))
}

impl QuillApp {
    pub(super) fn craft_session(&self) -> Option<&CraftSession> {
        self.session()
            .and_then(|s| s.payments.gift_market.craft.as_ref())
    }

    pub(super) fn craft_mut(&mut self) -> Option<&mut CraftSession> {
        self.gift_market_mut()
            .and_then(|market| market.craft.as_mut())
    }

    /// "Craft" on an owned collectible (tdesktop checks the gates first,
    /// then shows the info box).
    pub(super) fn open_craft_info(&mut self, gift: CraftGift, cx: &mut Context<Self>) {
        let gate = gift.gate(true, now_unix());
        if let Some(notice) = gate_notice(gate) {
            self.gift_market.push(notice);
        } else if gate == CraftGate::Ready {
            self.gift_market.craft_primary = Some(gift);
            self.gift_market.push(MarketBox::CraftInfo);
        }
        cx.notify();
    }

    /// "Start Crafting": the craft box with the gift in the first slot.
    fn start_crafting(&mut self, cx: &mut Context<Self>) {
        let Some(primary) = self.gift_market.craft_primary.clone() else {
            return;
        };
        let started = match self.live.as_mut() {
            Some(live) => live.driver.open_gift_craft(primary).is_ok(),
            None => self.gift_market_mut().is_some_and(|market| {
                // The demo keeps the lists it seeded.
                let mut next = CraftSession::new(primary);
                if let Some(old) = market.craft.take()
                    && old.regular_gift_id == next.regular_gift_id
                {
                    next.candidates = old.candidates;
                    next.candidates_loaded = old.candidates_loaded;
                    next.probabilities = old.probabilities;
                    next.market = old.market;
                }
                market.craft = Some(next);
                true
            }),
        };
        if started {
            self.market_replace(MarketBox::Craft, cx);
        } else {
            self.connection.status_note = "Couldn't start crafting.".into();
            cx.notify();
        }
    }

    fn remove_craft_slot(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(craft) = self.craft_mut() {
            craft.remove(index);
        }
        cx.notify();
    }

    fn open_craft_select(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.preload_craft_lists();
        }
        self.market_push(MarketBox::CraftSelect, cx);
    }

    fn open_craftable_models(&mut self, gift_id: i64, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_gift_upgrade_preview(gift_id);
        }
        self.market_push(MarketBox::Variants { gift_id }, cx);
    }

    /// The craft button: `craftGift`, and the forge starts.
    pub(super) fn craft_now(&mut self, cx: &mut Context<Self>) {
        let started = match self.live.as_mut() {
            Some(live) => live.driver.craft_gift().is_ok(),
            // The demo crafts nothing; the forge ends in a failure.
            None => self.craft_mut().is_some_and(|craft| {
                craft.crafting = true;
                true
            }),
        };
        if started {
            self.gift_market.forge_started = Some(Instant::now());
        } else {
            self.connection.status_note = "Couldn't craft the gift.".into();
        }
        cx.notify();
    }

    /// "Craft Another Gift" / "Craft New Gift": empty slots and fresh
    /// lists, then "Select Gifts" for the first one.
    pub(super) fn craft_again(&mut self, cx: &mut Context<Self>) {
        let demo = self.live.is_none();
        if let Some(craft) = self.craft_mut() {
            let used: Vec<String> = craft
                .slots
                .iter()
                .map(|slot| slot.received_gift_id.clone())
                .collect();
            craft.slots.clear();
            craft.result = None;
            if demo {
                craft
                    .candidates
                    .retain(|gift| !used.contains(&gift.received_gift_id));
            } else {
                craft.candidates.clear();
                craft.candidates_loaded = false;
                craft.candidates_offset.clear();
                craft.market =
                    ResaleSearch::new(craft.regular_gift_id, ResaleFilter::default(), true);
            }
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.preload_craft_lists();
        }
        self.market_replace(MarketBox::CraftSelect, cx);
    }

    /// Ends the forge once it ran its time and the answer is in.
    pub(super) fn craft_frame_upkeep(&mut self, cx: &mut Context<Self>) {
        let Some(started) = self.gift_market.forge_started else {
            return;
        };
        if started.elapsed().as_secs_f32() < FORGE_SECONDS {
            return;
        }
        let demo = self.live.is_none();
        let Some(craft) = self.craft_mut() else {
            self.gift_market.forge_started = None;
            return;
        };
        if demo && craft.crafting {
            craft.crafting = false;
            craft.result = Some(CraftResult::Fail);
        }
        if craft.crafting {
            return;
        }
        let result = craft.result.clone();
        if matches!(
            result,
            Some(CraftResult::InvalidGift | CraftResult::TooEarly(_))
        ) {
            craft.result = None;
        }
        self.gift_market.forge_started = None;
        match result {
            Some(CraftResult::Success { .. } | CraftResult::Fail) => {
                self.market_replace(MarketBox::CraftResult, cx);
            }
            Some(CraftResult::TooEarly(seconds)) => {
                let when = now_unix().saturating_add(i64::from(seconds));
                let when = i32::try_from(when).unwrap_or(i32::MAX);
                if let Some(notice) = gate_notice(CraftGate::Later(when)) {
                    self.market_push(notice, cx);
                }
            }
            Some(CraftResult::InvalidGift) => {
                self.connection.status_note = "This gift can't be used for crafting.".into();
                cx.notify();
            }
            // An error: its note is shown, the craft box stays.
            None => cx.notify(),
        }
    }

    /// Drops the craft once no craft box is left.
    pub(super) fn after_craft_pop(&mut self) {
        let crafting = self.gift_market.stack.iter().any(|kind| {
            matches!(
                kind,
                MarketBox::CraftInfo
                    | MarketBox::Craft
                    | MarketBox::CraftSelect
                    | MarketBox::CraftResult
            )
        });
        if crafting {
            return;
        }
        self.gift_market.craft_primary = None;
        self.gift_market.forge_started = None;
        if let Some(market) = self.gift_market_mut() {
            market.craft = None;
        }
    }

    pub(super) fn craft_info_box(&mut self, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let mut body = div().flex().flex_col().gap_4();
        if let Some(session) = self.session()
            && let Some(primary) = self.gift_market.craft_primary.as_ref()
        {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(collectible_tile(
                        ("craft-info-cover", 0),
                        &primary.gift,
                        session,
                        &roots,
                        120.,
                    ))
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(cx.theme().muted_foreground)
                            .child("Turn your gifts into rare, epic and legendary versions."),
                    ),
            );
        }
        let rows = [
            (
                IconName::Star,
                "Get Rare Models",
                "Select up to 4 gifts to craft a new exclusive model.",
            ),
            (
                IconName::Plus,
                "Maximize Chances",
                "Combine more gifts to increase your odds of success.",
            ),
            (
                IconName::RefreshCw,
                "Affect the Result",
                "Use gifts with the same attribute to boost its chance.",
            ),
        ];
        for (icon, title, text) in rows {
            body = body.child(feature_row(icon, title, text, cx));
        }
        BoxView::new("Gift Crafting", body).footer(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(self.market_back_button("craft-info-back", "Cancel", cx))
                .child(
                    Button::new("craft-info-start")
                        .label("Start Crafting")
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.start_crafting(cx))),
                ),
        )
    }

    pub(super) fn craft_box(&mut self, cx: &mut Context<Self>) -> BoxView {
        if let Some(started) = self.gift_market.forge_started {
            return self.forge_box(started, cx);
        }
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new("Craft Gift", div());
        };
        let Some(craft) = session.payments.gift_market.craft.as_ref() else {
            return BoxView::new("Craft Gift", div());
        };
        let name = craft
            .slots
            .first()
            .map(|slot| slot.gift.full_name())
            .or_else(|| session.gift_market_title(craft.regular_gift_id))
            .unwrap_or_default();
        let mut grid = div().w(px(SLOT * 2. + 12.)).flex().flex_wrap().gap_3();
        for index in 0..CRAFT_SLOTS {
            let slot = match craft.slots.get(index) {
                Some(gift) => div()
                    .relative()
                    .size(px(SLOT))
                    .child(
                        collectible_tile(
                            ("craft-slot", index as u64),
                            &gift.gift,
                            session,
                            &roots,
                            SLOT,
                        )
                        .h(px(SLOT)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_1()
                            .right_1()
                            .px_1p5()
                            .rounded_full()
                            .bg(gpui_kit::black().opacity(0.35))
                            .text_xs()
                            .font_semibold()
                            .child(format_percent(gift.gift.craft_per_mille)),
                    )
                    .when(index > 0, |slot| {
                        slot.child(
                            div().absolute().bottom_1().right_1().child(
                                Button::new(("craft-remove", index as u64))
                                    .icon(IconName::Close)
                                    .xsmall()
                                    .ghost()
                                    .text_color(gpui_kit::white())
                                    .accessibility_label(format!(
                                        "Remove {}",
                                        gift.gift.full_name()
                                    ))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_craft_slot(index, cx);
                                    })),
                            ),
                        )
                    }),
                None => div().child(
                    Button::new(("craft-add", index as u64))
                        .icon(IconName::Plus)
                        .ghost()
                        .w(px(SLOT))
                        .h(px(SLOT))
                        .rounded_lg()
                        .border_1()
                        .border_color(gpui_kit::white().opacity(0.3))
                        .bg(gpui_kit::white().opacity(0.06))
                        .text_color(gpui_kit::white())
                        .accessibility_label("Add a gift")
                        .on_click(cx.listener(|this, _, _, cx| this.open_craft_select(cx))),
                ),
            };
            grid = grid.child(slot);
        }
        let about = format!("Add up to 4 gifts to craft new {name}.");
        let mut panel = craft_panel(craft.slots.len())
            .child(grid)
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .child(with_bold(about, &["4", &name])),
            )
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .opacity(0.7)
                    .child("If crafting fails, all used gifts will be lost."),
            );
        let chances = attribute_chances(&craft.slots, &craft.probabilities);
        let mut chips = div().flex().flex_wrap().justify_center().gap_2();
        for (ix, chance) in chances.iter().enumerate() {
            let tip = chance.tooltip();
            chips = chips.child(
                div()
                    .id(("craft-chance", ix))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_full()
                    .bg(gpui_kit::white().opacity(0.12))
                    .text_xs()
                    .aria_label(tip.clone())
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .child(
                        ProgressCircle::new(("craft-chance-ring", ix))
                            .value(chance.per_mille as f32 / 10.)
                            .color(gpui_kit::white())
                            .xsmall(),
                    )
                    .child(format!(
                        "{} {}",
                        format_percent(chance.per_mille),
                        chance.name()
                    )),
            );
        }
        if !chances.is_empty() {
            panel = panel.child(chips);
        }
        if shows_craftable_link(&chances) {
            let gift_id = craft.regular_gift_id;
            panel = panel.child(
                Button::new("craft-view-all")
                    .label("View all craftable models \u{203A}")
                    .ghost()
                    .text_color(gpui_kit::white())
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_craftable_models(gift_id, cx);
                    })),
            );
        }
        let success = format_percent(craft.success_per_mille());
        let ready = !craft.slots.is_empty() && !craft.crafting;
        let craft_button = two_line_button(
            "craft-start",
            format!("Craft {name}"),
            format!("{success} Success Chance"),
        )
        .disabled(!ready)
        .on_click(cx.listener(|this, _, _, cx| this.craft_now(cx)));
        BoxView::new("Craft Gift", div().child(panel)).footer(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(self.market_back_button("craft-back", "Back", cx))
                .child(craft_button),
        )
    }

    /// The forge: the first gift glowing, the others fading, a ring
    /// filling over `FORGE_SECONDS` (a simplified tdesktop
    /// `StartCraftAnimation`).
    fn forge_box(&mut self, started: Instant, cx: &mut Context<Self>) -> BoxView {
        let elapsed = started.elapsed().as_secs_f32();
        let progress = (elapsed / FORGE_SECONDS).clamp(0., 1.);
        self.request_animation_tick(30, cx);
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new("Crafting", div());
        };
        let Some(craft) = session.payments.gift_market.craft.as_ref() else {
            return BoxView::new("Crafting", div());
        };
        let (_, _, glow, _) = craft_colors(craft.slots.len());
        let pulse = 0.35 + 0.25 * (elapsed * std::f32::consts::TAU / 1.5).sin();
        let name = craft
            .slots
            .first()
            .map(|slot| slot.gift.full_name())
            .unwrap_or_default();
        let mut panel = craft_panel(craft.slots.len());
        if let Some(primary) = craft.slots.first() {
            panel = panel.child(
                div()
                    .relative()
                    .p_3()
                    .rounded_xl()
                    .bg(rgb(glow).opacity(pulse))
                    .child(collectible_tile(
                        ("craft-forge-cover", 0),
                        &primary.gift,
                        session,
                        &roots,
                        SLOT,
                    )),
            );
        }
        let mut lost = div().flex().gap_2();
        for (index, slot) in craft.slots.iter().enumerate().skip(1) {
            lost = lost.child(div().opacity(1. - progress).child(collectible_tile(
                ("craft-forge-lost", index as u64),
                &slot.gift,
                session,
                &roots,
                56.,
            )));
        }
        let waiting = progress >= 1. && craft.crafting;
        if progress < 1. {
            panel = panel.child(lost);
        }
        panel = panel
            .child(
                ProgressCircle::new("craft-forge-ring")
                    .size(px(40.))
                    .value(progress * 100.)
                    .loading(waiting)
                    .color(rgb(glow))
                    .accessibility_label("Crafting"),
            )
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .child(with_bold(format!("Crafting {name}"), &[&name])),
            )
            .child(div().text_sm().text_center().child(format!(
                "{} success chance",
                format_percent(craft.success_per_mille())
            )))
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .opacity(0.7)
                    .child("If crafting fails, all used gifts will be lost."),
            );
        BoxView::new("Crafting", div().child(panel))
    }

    pub(super) fn craft_result_box(&mut self, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new("Crafting Failed", div());
        };
        let Some(craft) = session.payments.gift_market.craft.as_ref() else {
            return BoxView::new("Crafting Failed", div());
        };
        let close = Button::new("craft-result-close")
            .label("Close")
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                this.gift_market.stack.clear();
                this.after_craft_pop();
                cx.notify();
                this.close_kit_dialog_if_done(DialogKind::GiftMarket, window, cx);
            }));
        let footer = |label: &'static str, cx: &mut Context<Self>| {
            div().flex().justify_end().gap_2().child(close).child(
                Button::new("craft-result-again")
                    .label(label)
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.craft_again(cx))),
            )
        };
        match craft.result.as_ref() {
            Some(CraftResult::Success { gift, .. }) => {
                let body = div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().flex().justify_center().child(collectible_tile(
                        ("craft-result", 0),
                        gift,
                        session,
                        &roots,
                        160.,
                    )))
                    .child(attribute_row(
                        "Model",
                        &gift.model.name,
                        gift.model.rarity,
                        cx,
                    ))
                    .child(attribute_row(
                        "Backdrop",
                        &gift.backdrop.name,
                        gift.backdrop.rarity,
                        cx,
                    ))
                    .child(attribute_row(
                        "Symbol",
                        &gift.symbol.name,
                        gift.symbol.rarity,
                        cx,
                    ));
                BoxView::new(gift.full_name(), body).footer(footer("Craft Another Gift", cx))
            }
            _ => {
                let count = craft.slots.len();
                let text = craft_failed_text(count);
                let lost = count.to_string();
                let body = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_sm()
                    .text_center()
                    .children(
                        text.lines()
                            .map(|line| with_bold(line.to_string(), &[lost.as_str()])),
                    );
                BoxView::new("Crafting Failed", body).footer(footer("Craft New Gift", cx))
            }
        }
    }

    pub(super) fn notice_box(
        &mut self,
        title: String,
        text: String,
        cx: &mut Context<Self>,
    ) -> BoxView {
        BoxView::new(title, div().text_sm().child(text)).footer(
            div().flex().justify_end().child(
                self.market_back_button("market-notice-ok", "OK", cx)
                    .primary(),
            ),
        )
    }
}

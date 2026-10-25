//! The Premium promo page: header, the feature list, plans with prices, the
//! "Doubled limits" page and the subscribe button.
//!
//! tdesktop `settings/settings_premium.cpp`. Subscribing follows
//! `StartPremiumPayment` (`purchase.rs`); the feature previews are in
//! `preview.rs`.

use super::*;
use gpui_kit::assets::IconName;
use quill::ids::FileId;
use quill::lang::{keys, tr, tr_with};
use quill::premium_hub::PremiumFeature;
use quill::premium_hub::promo::{
    PreviewOpen, PromoFeature, PromoPage, animation_for, button_plan, limit_lines, subscribe_label,
};

/// Tint of the feature icons, cycled down the list.
const ICON_TINTS: [(u32, u32); 5] = [
    (0x6b93ff, 0x8f7cff),
    (0x8f7cff, 0xc66cd8),
    (0xc66cd8, 0xe46a9a),
    (0xe4806a, 0xe4a46a),
    (0x4cb8a0, 0x4c9fe0),
];

/// Text and icons on the promo's gradients. The gradients are fixed brand
/// colours, so the foreground is a fixed white too and never follows a
/// custom theme.
pub(super) fn on_brand() -> Rgba {
    rgb(0xffffff)
}

/// The stickers the stickers preview shows: the one that was clicked
/// first, then the examples.
pub(super) fn preview_stickers(
    hub: &quill::premium_hub::PremiumHub,
) -> impl Iterator<Item = &quill::telegram::envelope::StickerItem> {
    let requested = hub.premium_requested.as_ref();
    requested
        .into_iter()
        .chain(hub.premium_stickers.iter().filter(move |sticker| {
            requested.is_none_or(|requested| requested.file_id != sticker.file_id)
        }))
        .take(super::preview::STICKER_TILES)
}

fn feature_icon(index: usize) -> Div {
    let (from, to) = ICON_TINTS
        .get(index % ICON_TINTS.len())
        .copied()
        .unwrap_or_default();
    div()
        .flex_none()
        .size(px(30.))
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .bg(linear_gradient(
            135.,
            linear_color_stop(rgb(from), 0.),
            linear_color_stop(rgb(to), 1.),
        ))
        .child(
            Icon::new(IconName::StarFill)
                .with_size(px(15.))
                .text_color(on_brand()),
        )
}

impl QuillApp {
    /// The page the Premium dialog is on.
    pub(super) fn promo_page(&self) -> PromoPage {
        self.session()
            .map(|s| s.payments.hub.promo_page)
            .unwrap_or_default()
    }

    /// Ask for the example stickers and fetch their files as they arrive
    /// (the answer lands a moment later, so this checks a few times).
    fn start_sticker_examples(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.maybe_fetch_premium_stickers();
        }
        cx.spawn(async move |this, cx| {
            for _ in 0..10 {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(600))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let ids: Vec<FileId> = this
                        .session()
                        .map(|s| {
                            preview_stickers(&s.payments.hub)
                                .filter_map(|st| st.display_file_id())
                                .collect()
                        })
                        .unwrap_or_default();
                    if let Some(live) = this.live.as_mut() {
                        for id in ids {
                            let _ = live.driver.download_file(id, 16);
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// A Premium sticker was chosen without Premium: show the Premium
    /// stickers preview with that sticker first (tdesktop
    /// `ShowSendPremiumError` → `ShowStickerPreviewBox`).
    pub(in crate::ui) fn show_premium_sticker_preview(
        &mut self,
        sticker: Option<quill::telegram::envelope::StickerItem>,
        cx: &mut Context<Self>,
    ) {
        self.open_premium_at(PremiumFeature::UniqueStickers, cx);
        if let Some(hub) = self.hub_mut() {
            hub.premium_requested = sticker;
        }
    }

    /// Title of the Premium dialog on the current page.
    pub(super) fn promo_title(&self) -> String {
        match self.promo_page() {
            PromoPage::Overview => tr(keys::SCHEDULE_REPEAT_PROMO_LINK).into(),
            PromoPage::Limits => tr(keys::PREMIUM_SUMMARY_SUBTITLE_DOUBLE_LIMITS).into(),
            PromoPage::Preview { feature, .. } => feature.title().into(),
        }
    }

    pub(super) fn set_promo_page(&mut self, page: PromoPage, cx: &mut Context<Self>) {
        // A new page starts without the old clip; a preview asks for its own.
        self.premium_preview.stop();
        if let PromoPage::Preview { feature, .. } = page {
            self.fetch_preview_clip(feature, cx);
            if feature == PromoFeature::Premium(PremiumFeature::UniqueStickers) {
                self.start_sticker_examples(cx);
            }
        }
        if let Some(hub) = self.hub_mut() {
            hub.promo_page = page;
        }
        cx.notify();
    }

    /// Download the promotion clip of `feature`: at once when the Premium
    /// state is known, else as soon as it arrives while the preview shows.
    fn fetch_preview_clip(&mut self, feature: PromoFeature, cx: &mut Context<Self>) {
        let clip = move |this: &Self| -> Option<Option<i32>> {
            let state = this.session()?.payments.hub.premium_state.as_ref()?;
            Some(animation_for(&state.animations, feature).map(|a| a.file_id))
        };
        let download = |this: &mut Self, id: i32| {
            if let Some(live) = this.live.as_mut() {
                let _ = live.driver.download_file(FileId(id), 32);
            }
        };
        if let Some(found) = clip(self) {
            if let Some(id) = found {
                download(self, id);
            }
            return;
        }
        cx.spawn(async move |this, cx| {
            for _ in 0..24 {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let done = this
                    .update(cx, |this, cx| {
                        let showing = matches!(
                            this.promo_page(),
                            PromoPage::Preview { feature: shown, .. } if shown == feature
                        );
                        if !showing || !this.session().is_some_and(|s| s.payments.hub.premium_open)
                        {
                            return true;
                        }
                        let Some(found) = clip(this) else {
                            return false;
                        };
                        if let Some(id) = found {
                            download(this, id);
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    /// The body of the Premium dialog for the current page.
    pub(super) fn premium_promo_body(&self, cx: &mut Context<Self>) -> Div {
        match self.promo_page() {
            PromoPage::Overview => self.promo_overview(cx),
            PromoPage::Limits => self.promo_limits(cx),
            PromoPage::Preview { feature, opened } => self.promo_preview(feature, opened, cx),
        }
    }

    fn promo_overview(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div();
        };
        let hub = &session.payments.hub;
        let subscribed =
            hub.premium_state.as_ref().is_some_and(|s| s.is_subscribed) || session.my_is_premium();
        let state_text = hub
            .premium_state
            .as_ref()
            .map(|s| s.text.clone())
            .filter(|t| !t.is_empty());
        let mut body = div().flex().flex_col().gap_3();
        body = body.child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .py_2()
                .child(
                    div()
                        .size(px(64.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(linear_gradient(
                            135.,
                            linear_color_stop(rgb(0x6b93ff), 0.),
                            linear_color_stop(rgb(0xe46ace), 1.),
                        ))
                        .child(
                            Icon::new(IconName::StarFill)
                                .with_size(px(32.))
                                .text_color(on_brand()),
                        ),
                )
                .when(subscribed, |this| {
                    this.child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child(tr(keys::PREMIUM_SUMMARY_TITLE_SUBSCRIBED)),
                    )
                })
                .child(div().text_sm().text_color(muted).text_center().child(
                    match (subscribed, state_text) {
                        (true, Some(text)) => text,
                        (true, None) => tr(keys::Q_MONEY_THANK_YOU_FOR_SUBSCRIBING_TO).into(),
                        (false, _) => tr(keys::Q_MONEY_GO_BEYOND_THE_LIMITS_AND).into(),
                    },
                )),
        );
        if let Some(err) = hub.premium_error.as_ref() {
            body = body.child(div().text_sm().child(err.clone()));
        }
        if hub.premium_loading && hub.premium.is_none() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(keys::Q_MONEY_LOADING_FEATURES)),
            );
        }
        if let Some(info) = hub.premium.as_ref() {
            let mut list = div().flex().flex_col().gap_1();
            for (index, &feature) in info.features.iter().enumerate() {
                let target = PromoPage::for_feature(feature.into(), PreviewOpen::Overview);
                let row = div()
                    .id(("premium-feature", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .child(feature_icon(index))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().font_semibold().child(feature.title()))
                            .child(div().text_xs().text_color(muted).child(feature.about())),
                    )
                    .cursor_pointer()
                    .hover(|s| s.bg(fill_muted()))
                    .child(Icon::new(IconName::ChevronRight).with_size(px(14.)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_promo_page(target, cx);
                    }));
                list = list.child(row);
            }
            body = body.child(list);
        }
        if !subscribed {
            body = body.child(self.promo_subscribe(cx));
        }
        body
    }

    /// Plans and the subscribe button (`start_premium_payment`).
    pub(super) fn promo_subscribe(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div();
        };
        if !session.premium_possible() {
            return div()
                .text_xs()
                .text_color(muted)
                .text_center()
                .child(tr(keys::Q_MONEY_TELEGRAM_PREMIUM_IS_NOT_AVAILABLE));
        }
        let plans = session
            .payments
            .hub
            .premium_state
            .as_ref()
            .map(|s| s.options.clone())
            .unwrap_or_default();
        let mut section = div().flex().flex_col().gap_2().pt_2();
        let chosen = button_plan(&plans).map(|plan| plan.months);
        for plan in &plans {
            let mut row = div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .px_3()
                .py_1p5()
                .rounded_md()
                .bg(cx.theme().muted)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_sm().font_semibold().child(plan.name()))
                        .child(div().text_xs().text_color(muted).child(plan.total_text())),
                );
            let mut price = div().flex().items_center().gap_2();
            if plan.discount_percentage > 0 {
                price = price.child(
                    div()
                        .px_1p5()
                        .rounded_sm()
                        .text_xs()
                        .font_semibold()
                        .text_color(on_brand())
                        .bg(rgb(0x6b93ff))
                        .child(format!("-{}%", plan.discount_percentage)),
                );
            }
            price = price.child(div().text_sm().font_medium().child(tr_with(
                keys::PREMIUM_GIFT_PER,
                &[("cost", &(plan.per_month_text()).to_string())],
            )));
            row = row.child(price);
            if Some(plan.months) == chosen {
                row = row.border_1().border_color(rgb(0x6b93ff));
            }
            section = section.child(row);
        }
        section = section.child(
            Button::new("premium-subscribe")
                .label(subscribe_label(&plans))
                .primary()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.start_premium_payment(window, cx);
                })),
        );
        section
    }

    /// "Doubled limits": each limit with its free and Premium value.
    fn promo_limits(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let info = self.session().and_then(|s| s.payments.hub.premium.as_ref());
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .text_sm()
                .text_color(muted)
                .child(tr(keys::Q_MONEY_PREMIUM_ACCOUNTS_GET_HIGHER_LIMITS)),
        );
        for line in limit_lines(info) {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().muted)
                    .child(div().text_sm().font_semibold().child(line.label))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .child(
                                div()
                                    .text_color(muted)
                                    .child(line.default_value.to_string()),
                            )
                            .child(Icon::new(IconName::ArrowRight).with_size(px(12.)))
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(rgb(0x6b93ff))
                                    .child(format!("{} {}", line.premium_value, line.unit)),
                            ),
                    ),
            );
        }
        body.child(
            Button::new("premium-limits-back")
                .label(tr(keys::Q_MONEY_BACK))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_promo_page(PromoPage::Overview, cx);
                })),
        )
    }
}

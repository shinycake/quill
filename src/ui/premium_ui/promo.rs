//! The Premium promo page: header, the feature list, plans with prices, the
//! "Doubled limits" page and the subscribe button.
//!
//! tdesktop `settings/settings_premium.cpp`. Subscribing opens @PremiumBot,
//! as tdesktop does when the store flow is not available; Quill takes no
//! payment itself (`premium-in-app-purchase` stays blocked).

use super::*;
use gpui_kit::assets::IconName;
use quill::premium_hub::promo::{PromoPage, button_plan, limit_lines, subscribe_label};

/// Tint of the feature icons, cycled down the list.
const ICON_TINTS: [(u32, u32); 5] = [
    (0x6b93ff, 0x8f7cff),
    (0x8f7cff, 0xc66cd8),
    (0xc66cd8, 0xe46a9a),
    (0xe4806a, 0xe4a46a),
    (0x4cb8a0, 0x4c9fe0),
];

fn feature_icon(index: usize) -> Div {
    let (from, to) = ICON_TINTS[index % ICON_TINTS.len()];
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
                .text_color(rgb(0xffffff)),
        )
}

impl QuillApp {
    /// The page the Premium dialog is on.
    pub(super) fn promo_page(&self) -> PromoPage {
        self.session()
            .map(|s| s.payments.hub.promo_page)
            .unwrap_or_default()
    }

    fn set_promo_page(&mut self, page: PromoPage, cx: &mut Context<Self>) {
        if let Some(hub) = self.hub_mut() {
            hub.promo_page = page;
        }
        cx.notify();
    }

    /// The body of the Premium dialog for the current page.
    pub(super) fn premium_promo_body(&self, cx: &mut Context<Self>) -> Div {
        match self.promo_page() {
            PromoPage::Overview => self.promo_overview(cx),
            PromoPage::Limits => self.promo_limits(cx),
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
                                .text_color(rgb(0xffffff)),
                        ),
                )
                .when(subscribed, |this| {
                    this.child(div().text_lg().font_semibold().child("You are all set!"))
                })
                .child(
                    div().text_sm().text_color(muted).text_center().child(
                        match (subscribed, state_text) {
                            (true, Some(text)) => text,
                            (true, None) => "Thank you for subscribing to Telegram Premium.".into(),
                            (false, _) => "Go beyond the limits and unlock dozens of exclusive \
                                           features by subscribing to Telegram Premium."
                                .into(),
                        },
                    ),
                ),
        );
        if let Some(err) = hub.premium_error.as_ref() {
            body = body.child(div().text_sm().child(err.clone()));
        }
        if hub.premium_loading && hub.premium.is_none() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Loading features\u{2026}"),
            );
        }
        if let Some(info) = hub.premium.as_ref() {
            let mut list = div().flex().flex_col().gap_1();
            for (index, feature) in info.features.iter().enumerate() {
                let limits_row = feature.title == "Doubled limits";
                let mut row = div()
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
                            .child(div().text_sm().font_semibold().child(feature.title))
                            .child(div().text_xs().text_color(muted).child(feature.about)),
                    );
                if limits_row {
                    row = row
                        .cursor_pointer()
                        .hover(|s| s.bg(fill_muted()))
                        .child(Icon::new(IconName::ChevronRight).with_size(px(14.)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_promo_page(PromoPage::Limits, cx);
                        }));
                }
                list = list.child(row);
            }
            body = body.child(list);
        }
        if !subscribed {
            body = body.child(self.promo_subscribe(cx));
        }
        body
    }

    /// Plans and the subscribe button. Opens @PremiumBot; no payment here.
    fn promo_subscribe(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div();
        };
        if !session.premium_possible() {
            return div()
                .text_xs()
                .text_color(muted)
                .text_center()
                .child("Telegram Premium is not available for this account right now.");
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
                        .text_color(rgb(0xffffff))
                        .bg(rgb(0x6b93ff))
                        .child(format!("-{}%", plan.discount_percentage)),
                );
            }
            price = price.child(
                div()
                    .text_sm()
                    .font_medium()
                    .child(format!("{} / month", plan.per_month_text())),
            );
            row = row.child(price);
            if Some(plan.months) == chosen {
                row = row.border_1().border_color(rgb(0x6b93ff));
            }
            section = section.child(row);
        }
        section = section
            .child(
                Button::new("premium-subscribe")
                    .label(subscribe_label(&plans))
                    .primary()
                    .on_click(cx.listener(|_, _, _, cx| cx.open_url(PREMIUM_BOT_URL))),
            )
            .child(div().text_xs().text_color(muted).text_center().child(
                "Subscriptions are managed through @PremiumBot. Quill never takes payment.",
            ));
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
                .child("Premium accounts get higher limits."),
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
                .label("Back")
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_promo_page(PromoPage::Overview, cx);
                })),
        )
    }
}

//! The Premium promo page: header, the feature list, plans with prices, the
//! "Doubled limits" page and the subscribe button.
//!
//! tdesktop `settings/settings_premium.cpp`. Subscribing follows
//! `StartPremiumPayment` (`purchase.rs`).

use super::*;
use crate::ui::video_player::{PlayState, Purpose, VideoPlayer, video_frame};
use gpui_kit::assets::IconName;
use quill::ids::FileId;
use quill::lang::{keys, tr, tr_with};
use quill::local_path::sandboxed_display_path;
use quill::premium_hub::promo::{
    PromoPage, animation_for, button_plan, limit_lines, step_preview, subscribe_label,
};

/// The feature whose preview shows example stickers (tdesktop shows a
/// sticker carousel there instead of a video).
const STICKERS_FEATURE: &str = "Premium stickers";
/// `premiumFeatureMessageEffects`' title (`premium_hub`).
const EFFECTS_FEATURE: &str = "Message effects";
/// Stickers shown in that preview.
const STICKER_TILES: usize = 6;

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
fn on_brand() -> Rgba {
    rgb(0xffffff)
}

/// The stickers the stickers preview shows: the one that was clicked
/// first, then the examples.
fn preview_stickers(
    hub: &quill::premium_hub::PremiumHub,
) -> impl Iterator<Item = &quill::telegram::envelope::StickerItem> {
    let requested = hub.premium_requested.as_ref();
    requested
        .into_iter()
        .chain(hub.premium_stickers.iter().filter(move |sticker| {
            requested.is_none_or(|requested| requested.file_id != sticker.file_id)
        }))
        .take(STICKER_TILES)
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

    pub(super) fn preview_feature_title(&self, index: usize) -> Option<&'static str> {
        self.session()?
            .payments
            .hub
            .premium
            .as_ref()?
            .features
            .get(index)
            .map(|f| f.title)
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
        if let Some(hub) = self.hub_mut() {
            hub.premium_requested = sticker;
        }
        self.open_premium(cx);
        self.go_to_feature_preview(STICKERS_FEATURE, cx);
    }

    /// "Subscribe to Telegram Premium" under a Premium message effect: the
    /// Premium dialog on its effects feature (tdesktop
    /// `ShowPremiumPreviewBox(PremiumFeature::Effects)`).
    pub(in crate::ui) fn show_premium_effects_preview(&mut self, cx: &mut Context<Self>) {
        self.open_premium(cx);
        self.go_to_feature_preview(EFFECTS_FEATURE, cx);
    }

    /// A reaction that needs Premium: the Premium dialog on its feature
    /// (tdesktop `ShowReactPremiumError` → `ShowPremiumPreviewBox`).
    pub(in crate::ui) fn show_premium_reaction_preview(
        &mut self,
        feature: quill::state::ReactPremiumFeature,
        cx: &mut Context<Self>,
    ) {
        self.open_premium(cx);
        self.go_to_feature_preview(
            match feature {
                quill::state::ReactPremiumFeature::UniqueReactions => {
                    quill::premium_hub::UNIQUE_REACTIONS_TITLE
                }
                quill::state::ReactPremiumFeature::SavedMessagesTags => {
                    quill::premium_hub::SAVED_TAGS_TITLE
                }
            },
            cx,
        );
    }

    /// Disable Sharing without Premium: the Premium dialog on that feature
    /// (tdesktop `ShowPremiumPreviewBox(PremiumFeature::NoForwards)`).
    pub(in crate::ui) fn show_premium_no_forwards_preview(&mut self, cx: &mut Context<Self>) {
        self.open_premium(cx);
        self.go_to_feature_preview(quill::premium_hub::NO_FORWARDS_TITLE, cx);
    }

    /// Page the open Premium dialog to the feature titled `title`: at once
    /// when the feature list is known, else as soon as it arrives.
    fn go_to_feature_preview(&mut self, title: &'static str, cx: &mut Context<Self>) {
        let feature_page = move |this: &Self| {
            this.session()?
                .payments
                .hub
                .premium
                .as_ref()?
                .features
                .iter()
                .position(|feature| feature.title == title)
        };
        if let Some(index) = feature_page(self) {
            self.set_promo_page(PromoPage::Preview(index), cx);
            return;
        }
        cx.spawn(async move |this, cx| {
            for _ in 0..24 {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let done = this
                    .update(cx, |this, cx| {
                        let open = this.session().is_some_and(|s| s.payments.hub.premium_open);
                        if !open {
                            return true;
                        }
                        match feature_page(this) {
                            Some(index) => {
                                this.set_promo_page(PromoPage::Preview(index), cx);
                                true
                            }
                            None => false,
                        }
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    /// Title of the Premium dialog on the current page.
    pub(super) fn promo_title(&self) -> String {
        match self.promo_page() {
            PromoPage::Overview => tr(keys::SCHEDULE_REPEAT_PROMO_LINK).into(),
            PromoPage::Limits => tr(keys::PREMIUM_SUMMARY_SUBTITLE_DOUBLE_LIMITS).into(),
            PromoPage::Preview(index) => self
                .session()
                .and_then(|s| s.payments.hub.premium.as_ref())
                .and_then(|info| info.features.get(index))
                .map_or_else(
                    || tr(keys::SCHEDULE_REPEAT_PROMO_LINK).into(),
                    |f| f.title.to_string(),
                ),
        }
    }

    fn set_promo_page(&mut self, page: PromoPage, cx: &mut Context<Self>) {
        // A new page starts without the old clip; a preview asks for its own.
        self.premium_preview.stop();
        if let PromoPage::Preview(index) = page {
            let clip = self
                .session()
                .and_then(|s| {
                    let hub = &s.payments.hub;
                    let feature = hub.premium.as_ref()?.features.get(index)?;
                    animation_for(&hub.premium_state.as_ref()?.animations, feature.title)
                        .map(|a| a.file_id)
                })
                .filter(|id| *id > 0);
            if let (Some(id), Some(live)) = (clip, self.live.as_mut()) {
                let _ = live.driver.download_file(FileId(id), 32);
            }
            if self.preview_feature_title(index) == Some(STICKERS_FEATURE) {
                self.start_sticker_examples(cx);
            }
        }
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
            PromoPage::Preview(index) => self.promo_preview(index, cx),
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
            for (index, feature) in info.features.iter().enumerate() {
                let limits_row = feature.title == tr(keys::PREMIUM_SUMMARY_SUBTITLE_DOUBLE_LIMITS);
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
                let target = if limits_row {
                    PromoPage::Limits
                } else {
                    PromoPage::Preview(index)
                };
                row = row
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

impl QuillApp {
    /// The muted looping player of the preview clip, opened once the clip
    /// is a local sandboxed file; `None` while it is not, or if it failed.
    fn preview_player(&self, file_id: i32, cx: &mut Context<Self>) -> Option<VideoPlayer> {
        if self.premium_preview.file_id.get() != Some(file_id) {
            self.premium_preview.stop();
        }
        // A re-entrant borrow skips this frame (the slot below fails too).
        let open = self.premium_preview.video.try_borrow().ok()?.clone();
        let video = match open {
            Some(video) => video,
            None => {
                let roots = self.media_display_roots();
                let path = self
                    .session()?
                    .media
                    .files
                    .get(&file_id)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))?;
                let video =
                    VideoPlayer::open(&path, Purpose::Inline { max_edge: 720 }, None, cx).ok()?;
                video.set_volume(0.0);
                video.set_looping(true);
                video.play();
                video.watch(cx.entity_id());
                *self.premium_preview.video.try_borrow_mut().ok()? = Some(video.clone());
                self.premium_preview.file_id.set(Some(file_id));
                video
            }
        };
        (video.state() != PlayState::Failed).then_some(video)
    }

    /// One feature with its promotion clip and paging to the others.
    fn promo_preview(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div();
        };
        let hub = &session.payments.hub;
        let subscribed =
            hub.premium_state.as_ref().is_some_and(|s| s.is_subscribed) || session.my_is_premium();
        let Some(info) = hub.premium.as_ref().filter(|i| !i.features.is_empty()) else {
            return div();
        };
        let count = info.features.len();
        let index = index.min(count - 1);
        let Some(&feature) = info.features.get(index) else {
            return div();
        };
        let clip = hub
            .premium_state
            .as_ref()
            .and_then(|s| animation_for(&s.animations, feature.title))
            .cloned();
        let side = px(340.);
        let height = match clip.as_ref() {
            Some(c) if c.width > 0 && c.height > 0 => {
                (side * (c.height as f32 / c.width as f32)).clamp(px(180.), px(300.))
            }
            _ => px(200.),
        };
        let picture = if feature.title == STICKERS_FEATURE {
            None
        } else {
            clip.as_ref()
                .and_then(|c| self.preview_player(c.file_id, cx))
        };
        // Rounded as the clip over it is (`corner_radii` below).
        let stage = div()
            .w(side)
            .h(height)
            .rounded(px(12.))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(linear_gradient(
                135.,
                linear_color_stop(rgb(0x6b93ff), 0.),
                linear_color_stop(rgb(0xe46ace), 1.),
            ));
        let stage = match picture {
            Some(player) => stage.child(
                div().w(side).h(height).child(
                    video_frame(&player)
                        .object_fit(ObjectFit::Cover)
                        // The dialog shows behind the rounded corners.
                        .corner_radii(Corners::all(px(12.)), cx.theme().background),
                ),
            ),
            None if feature.title == STICKERS_FEATURE && preview_stickers(hub).next().is_some() => {
                let roots = self.media_display_roots();
                let mut grid = div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap_2()
                    .p_3()
                    .size_full();
                for (i, sticker) in preview_stickers(hub).enumerate() {
                    let path = [sticker.display_file_id(), sticker.thumb_file_id]
                        .into_iter()
                        .flatten()
                        .filter_map(|id| session.media.files.get(&id.0)?.usable_path())
                        .find_map(|p| sandboxed_display_path(p, &roots));
                    let tile = div()
                        .id(("premium-sticker", i))
                        .size(px(96.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center();
                    grid = grid.child(match path {
                        Some(path) => tile.child(
                            img(crate::ui::image_budget::sized_media(
                                &path,
                                (px(96.), px(96.)),
                                Some((sticker.width, sticker.height)),
                                crate::ui::image_budget::Fit::Contain,
                            ))
                            .size(px(96.))
                            .object_fit(ObjectFit::Contain),
                        ),
                        None => tile.text_size(px(40.)).child(sticker.emoji.clone()),
                    });
                }
                stage.child(grid)
            }
            None => stage.child(
                Icon::new(IconName::StarFill)
                    .with_size(px(48.))
                    .text_color(on_brand()),
            ),
        };
        let mut dots = div().flex().items_center().justify_center().gap_1p5();
        for i in 0..count {
            dots = dots.child(div().size(px(6.)).rounded_full().bg(if i == index {
                rgb(0x6b93ff).into()
            } else {
                cx.theme().muted
            }));
        }
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .child(stage)
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .text_center()
                    .child(feature.title),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .text_center()
                    .child(feature.about),
            )
            .child(dots)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("premium-preview-prev")
                            .label(tr(keys::Q_MONEY_PREVIOUS))
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_promo_page(
                                    PromoPage::Preview(step_preview(index, count, -1)),
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new("premium-preview-next")
                            .label(tr(keys::CREATE_GROUP_NEXT))
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.set_promo_page(
                                    PromoPage::Preview(step_preview(index, count, 1)),
                                    cx,
                                );
                            })),
                    ),
            );
        if !subscribed {
            body = body.child(self.promo_subscribe(cx));
        }
        body.child(
            Button::new("premium-preview-back")
                .label(tr(keys::Q_MONEY_BACK))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_promo_page(PromoPage::Overview, cx);
                })),
        )
    }
}

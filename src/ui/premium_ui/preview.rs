//! The preview of one Premium or Business feature: its promotion clip,
//! paging through the list that holds it, and the buttons of the way it
//! was opened.
//!
//! tdesktop `boxes/premium_preview_box.cpp` (`PreviewBox`).

use super::promo::{on_brand, preview_stickers};
use super::*;
use crate::ui::video_player::{PlayState, Purpose, VideoPlayer, video_frame};
use gpui_kit::assets::IconName;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::premium_hub::promo::{
    PreviewOpen, PromoFeature, PromoPage, animation_for, preview_order, step_preview,
};
use quill::premium_hub::{BusinessFeature, PremiumFeature};

/// Stickers shown in the stickers preview.
pub(super) const STICKER_TILES: usize = 6;

/// The button of a preview opened as a box (tdesktop `ShowPremiumPreviewBox`
/// names the feature for four of them).
fn unlock_label(feature: PromoFeature) -> &'static str {
    tr(match feature {
        PromoFeature::Premium(PremiumFeature::UniqueReactions) => keys::PREMIUM_UNLOCK_REACTIONS,
        PromoFeature::Premium(PremiumFeature::UniqueStickers) => keys::PREMIUM_UNLOCK_STICKERS,
        PromoFeature::Premium(PremiumFeature::CustomEmoji) => keys::PREMIUM_UNLOCK_EMOJI,
        PromoFeature::Premium(PremiumFeature::EmojiStatus) => keys::PREMIUM_UNLOCK_STATUS,
        _ => keys::PREMIUM_MORE_ABOUT,
    })
}

/// What a gift can become (tdesktop `AddGiftsInfoRows`).
const GIFT_ROWS: [(IconName, quill::lang::Key, quill::lang::Key); 3] = [
    (
        IconName::Sparkles,
        keys::GIFT_UPGRADE_UNIQUE_TITLE,
        keys::GIFT_UPGRADE_UNIQUE_ABOUT,
    ),
    (
        IconName::Repeat,
        keys::GIFT_UPGRADE_TRADABLE_TITLE,
        keys::GIFT_UPGRADE_TRADABLE_ABOUT,
    ),
    (
        IconName::Crown,
        keys::GIFT_UPGRADE_WEARABLE_TITLE,
        keys::GIFT_UPGRADE_WEARABLE_ABOUT,
    ),
];

impl QuillApp {
    /// The features the preview of `feature` pages through.
    fn preview_features(&self, feature: PromoFeature) -> Vec<PromoFeature> {
        let Some(hub) = self.session().map(|s| &s.payments.hub) else {
            return vec![feature];
        };
        let premium = hub.premium.as_ref().map_or(&[][..], |i| &i.features[..]);
        let business = hub
            .business_features
            .as_deref()
            .unwrap_or(&BusinessFeature::FALLBACK_ORDER);
        preview_order(premium, business, feature)
    }

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

    /// The clip, the sticker examples, or a star over the brand gradient.
    fn preview_stage(&self, feature: PromoFeature, cx: &mut Context<Self>) -> Div {
        let Some(session) = self.session() else {
            return div();
        };
        let hub = &session.payments.hub;
        let stickers = feature == PromoFeature::Premium(PremiumFeature::UniqueStickers);
        let clip = hub
            .premium_state
            .as_ref()
            .and_then(|s| animation_for(&s.animations, feature))
            .cloned();
        let side = px(340.);
        let height = match clip.as_ref() {
            Some(c) if c.width > 0 && c.height > 0 => {
                (side * (c.height as f32 / c.width as f32)).clamp(px(180.), px(300.))
            }
            _ => px(200.),
        };
        let picture = if stickers {
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
        match picture {
            Some(player) => stage.child(
                div().w(side).h(height).child(
                    video_frame(&player)
                        .object_fit(ObjectFit::Cover)
                        // The dialog shows behind the rounded corners.
                        .corner_radii(Corners::all(px(12.)), cx.theme().background),
                ),
            ),
            None if stickers && preview_stickers(hub).next().is_some() => {
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
                Icon::new(if feature == PromoFeature::Gifts {
                    IconName::Gift
                } else {
                    IconName::StarFill
                })
                .with_size(px(48.))
                .text_color(on_brand()),
            ),
        }
    }

    /// Dots and Previous / Next through `order`; nothing for a lone feature.
    fn preview_paging(
        &self,
        order: &[PromoFeature],
        feature: PromoFeature,
        opened: PreviewOpen,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if order.len() < 2 {
            return None;
        }
        let mut dots = div().flex().items_center().justify_center().gap_1p5();
        for (i, shown) in order.iter().enumerate() {
            dots = dots.child(
                div()
                    .id(("premium-preview-dot", i))
                    .size(px(6.))
                    .rounded_full()
                    .bg(if *shown == feature {
                        rgb(0x6b93ff).into()
                    } else {
                        cx.theme().muted
                    }),
            );
        }
        let previous = step_preview(order, feature, -1);
        let next = step_preview(order, feature, 1);
        Some(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
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
                                        PromoPage::Preview {
                                            feature: previous,
                                            opened,
                                        },
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
                                        PromoPage::Preview {
                                            feature: next,
                                            opened,
                                        },
                                        cx,
                                    );
                                })),
                        ),
                ),
        )
    }

    fn gift_rows(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut rows = div().flex().flex_col().gap_3().w_full();
        for (icon, title, about) in GIFT_ROWS {
            rows = rows.child(
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .child(
                        Icon::new(icon)
                            .with_size(px(20.))
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().font_semibold().child(tr(title)))
                            .child(div().text_xs().text_color(muted).child(tr(about))),
                    ),
            );
        }
        rows
    }

    /// Close the Premium dialog from one of its own buttons.
    fn close_premium_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_premium(cx);
        self.close_kit_dialog_if_done(DialogKind::PremiumFeatures, window, cx);
    }

    /// One feature with its promotion clip, paging to the others and the
    /// buttons of the way it was opened.
    pub(super) fn promo_preview(
        &self,
        feature: PromoFeature,
        opened: PreviewOpen,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div();
        };
        let hub = &session.payments.hub;
        let subscribed =
            hub.premium_state.as_ref().is_some_and(|s| s.is_subscribed) || session.my_is_premium();
        let order = self.preview_features(feature);
        let mut body = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .child(self.preview_stage(feature, cx))
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .text_center()
                    .child(feature.title()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .text_center()
                    .child(feature.about()),
            );
        if feature == PromoFeature::Gifts {
            return body.child(self.gift_rows(cx)).child(
                Button::new("premium-preview-understood")
                    .label(tr(keys::AUCTION_ABOUT_UNDERSTOOD))
                    .primary()
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_premium_dialog(window, cx);
                    })),
            );
        }
        body = body.children(self.preview_paging(&order, feature, opened, cx));
        match opened {
            PreviewOpen::Box => body.child(
                Button::new("premium-preview-unlock")
                    .label(unlock_label(feature))
                    .primary()
                    .w_full()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(hub) = this.hub_mut() {
                            hub.premium_ref = feature.start_ref().unwrap_or_default().into();
                        }
                        this.set_promo_page(PromoPage::Overview, cx);
                    })),
            ),
            PreviewOpen::ToBuy if subscribed => body.child(
                Button::new("premium-preview-close")
                    .label(tr(keys::CLOSE))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_premium_dialog(window, cx);
                    })),
            ),
            PreviewOpen::ToBuy => body.child(self.promo_subscribe(cx)),
            PreviewOpen::Overview => body
                .when(!subscribed, |body| body.child(self.promo_subscribe(cx)))
                .child(
                    Button::new("premium-preview-back")
                        .label(tr(keys::Q_MONEY_BACK))
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_promo_page(PromoPage::Overview, cx);
                        })),
                ),
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "preview_tests.rs"]
mod tests;

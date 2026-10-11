//! Tapping a custom emoji in a message shows where it comes from: a small
//! card with the emoji, "This emoji is from the {pack} pack." and a View
//! button that opens the pack (tdesktop `ShowReactionPreview` with
//! `emojiPreview`, `history_view_reaction_preview.cpp`).

use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::state::CustomEmojiPreview;
use quill::sticker_set_box::custom_emoji_preview_label;
use std::time::Duration;

/// How long the card stays when nobody dismisses it.
const CARD_LIFETIME: Duration = Duration::from_secs(6);

impl QuillApp {
    /// A custom emoji in a message was tapped: ask for its pack.
    pub(super) fn tap_custom_emoji(&mut self, emoji_id: i64, cx: &mut Context<Self>) {
        let set_id = self
            .session()
            .and_then(|s| {
                s.stickers
                    .emoji
                    .custom_emoji_stickers
                    .iter()
                    .find(|item| item.custom_emoji_id == Some(emoji_id))
                    .map(|item| item.set_id)
            })
            .filter(|set_id| *set_id != 0);
        let Some(set_id) = set_id else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.preview_custom_emoji(emoji_id, set_id);
        }
        cx.notify();
    }

    fn clear_custom_emoji_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.stickers.custom_emoji_preview = None;
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.custom_emoji_preview = None;
        }
        self.message_ui.custom_emoji_card_seen = None;
        cx.notify();
    }

    /// The card, while a tapped emoji's pack is known. A new card starts
    /// its own timer.
    pub(super) fn custom_emoji_card(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let preview: CustomEmojiPreview = self.session()?.stickers.custom_emoji_preview.clone()?;
        if self.message_ui.custom_emoji_card_seen.as_ref() != Some(&preview) {
            self.message_ui.custom_emoji_card_seen = Some(preview.clone());
            let shown = preview.clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(CARD_LIFETIME).await;
                let _ = this.update(cx, |this, cx| {
                    if this.message_ui.custom_emoji_card_seen.as_ref() == Some(&shown) {
                        this.clear_custom_emoji_preview(cx);
                    }
                });
            })
            .detach();
        }
        let still = self.custom_emoji_still(preview.emoji_id);
        let label = custom_emoji_preview_label(&preview.title);
        let set_id = preview.set_id;
        let theme = cx.theme();
        let (border, bg, fg, muted) = (
            theme.border,
            theme.popover,
            theme.popover_foreground,
            theme.muted,
        );
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(132.))
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("custom-emoji-card")
                        .role(Role::Group)
                        .aria_label(label.clone())
                        .max_w(px(460.))
                        .flex()
                        .items_center()
                        .gap_3()
                        .p_3()
                        .rounded_xl()
                        .border_1()
                        .border_color(border)
                        .bg(bg)
                        .text_color(fg)
                        .shadow_md()
                        .child(match still {
                            Some(path) => img(path)
                                .size(px(56.))
                                .flex_none()
                                .object_fit(ObjectFit::Contain)
                                .into_any_element(),
                            None => div()
                                .size(px(56.))
                                .flex_none()
                                .rounded_lg()
                                .bg(muted)
                                .into_any_element(),
                        })
                        .child(div().flex_1().min_w_0().text_sm().child(label))
                        .child(
                            Button::new("custom-emoji-card-view")
                                .label("View")
                                .primary()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.clear_custom_emoji_preview(cx);
                                    this.view_message_sticker_set(set_id, cx);
                                })),
                        )
                        .child(
                            Button::new("custom-emoji-card-close")
                                .icon(gpui_kit::assets::IconName::X)
                                .ghost()
                                .small()
                                .tooltip("Close")
                                .accessibility_label("Close")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_custom_emoji_preview(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}

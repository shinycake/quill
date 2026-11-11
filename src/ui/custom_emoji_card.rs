//! Tapping a custom emoji in a message shows where it comes from: a small
//! card with the emoji, "This emoji is from the {pack} pack." and a View
//! button that opens the pack (tdesktop `ShowReactionPreview` with
//! `emojiPreview`, `history_view_reaction_preview.cpp`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::TestSupportExt;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::RequestId;
use quill::lang::{keys, tr};
use quill::state::{CustomEmojiAnswer, CustomEmojiPreview};
use quill::sticker_set_box::custom_emoji_preview_label;
use std::time::Duration;

/// How long the card stays when nobody dismisses it.
const CARD_LIFETIME: Duration = Duration::from_secs(6);

/// Where a window's custom emoji card stands. Every stage has a way out: the
/// pack's answer or error ends the ask, a close, View or the timer hides the
/// card, and a tap of another emoji replaces it. The `Shown` stage owns its
/// timer, so replacing or closing the card stops it.
#[derive(Default)]
pub(super) enum CustomEmojiCard {
    #[default]
    Hidden,
    /// The emoji was tapped; its pack is being asked for under `request`,
    /// and only that request's answer shows the card.
    Asked { emoji_id: i64, request: RequestId },
    /// The card is on screen until `_timer` ends or it is closed.
    Shown {
        preview: CustomEmojiPreview,
        _timer: Task<()>,
    },
}

impl QuillApp {
    /// A custom emoji in a message was tapped: ask for its pack, or start
    /// the timer of the card already showing it over.
    pub(super) fn tap_custom_emoji(&mut self, emoji_id: i64, cx: &mut Context<Self>) {
        if let CustomEmojiCard::Shown { preview, .. } = &self.message_ui.custom_emoji
            && preview.emoji_id == emoji_id
        {
            let preview = preview.clone();
            self.show_custom_emoji_card(preview, cx);
            return;
        }
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
        let Some(live) = self.live.as_mut() else {
            return;
        };
        // A send that failed asked nothing: no card waits for an answer.
        let Ok(request) = live.driver.preview_custom_emoji(emoji_id, set_id) else {
            return;
        };
        self.note_custom_emoji_tap(emoji_id, request);
        cx.notify();
    }

    /// The emoji was tapped and its pack asked for under `request`.
    pub(super) fn note_custom_emoji_tap(&mut self, emoji_id: i64, request: RequestId) {
        self.message_ui.custom_emoji = CustomEmojiCard::Asked { emoji_id, request };
    }

    fn clear_custom_emoji_card(&mut self, cx: &mut Context<Self>) {
        self.message_ui.custom_emoji = CustomEmojiCard::Hidden;
        cx.notify();
    }

    /// The card shows `preview` from now, for its own lifetime.
    fn show_custom_emoji_card(&mut self, preview: CustomEmojiPreview, cx: &mut Context<Self>) {
        let timer = self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor().timer(CARD_LIFETIME).await;
            let _ = this.update(cx, |this, cx| this.clear_custom_emoji_card(cx));
        });
        self.message_ui.custom_emoji = CustomEmojiCard::Shown {
            preview,
            _timer: timer,
        };
    }

    /// The answer of the request this window's tap is waiting for, if it
    /// came: the card shows, or (a refusal, a pack with no title) the tap
    /// ends without one.
    fn settle_custom_emoji_ask(
        &mut self,
        emoji_id: i64,
        request: RequestId,
        cx: &mut Context<Self>,
    ) {
        let answer = self
            .session_mut()
            .and_then(|session| session.stickers.custom_emoji_answers.take(request));
        match answer {
            Some(CustomEmojiAnswer::Ready(preview)) if preview.emoji_id == emoji_id => {
                self.show_custom_emoji_card(preview, cx)
            }
            Some(_) => self.message_ui.custom_emoji = CustomEmojiCard::Hidden,
            None => {}
        }
    }

    /// The card, once the pack of the emoji tapped in this window is known
    /// (every window's render asks, with its state in place).
    pub(in crate::ui) fn custom_emoji_card(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        match &self.message_ui.custom_emoji {
            CustomEmojiCard::Hidden => return None,
            CustomEmojiCard::Asked { emoji_id, request } => {
                let (emoji_id, request) = (*emoji_id, *request);
                self.settle_custom_emoji_ask(emoji_id, request, cx);
            }
            CustomEmojiCard::Shown { .. } => {}
        }
        let CustomEmojiCard::Shown { preview, .. } = &self.message_ui.custom_emoji else {
            return None;
        };
        let preview = preview.clone();
        self.custom_emoji_card_body(preview, cx)
    }

    /// `custom_emoji_card` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    #[inline(never)]
    fn custom_emoji_card_body(
        &mut self,
        preview: CustomEmojiPreview,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
                        .test_support()
                        // Clicks, hover and scroll over the card stay on it.
                        .occlude()
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
                                .label(tr(keys::Q_MEDIA_VIEW))
                                .primary()
                                .small()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.clear_custom_emoji_card(cx);
                                    this.view_message_sticker_set(set_id, cx);
                                })),
                        )
                        .child(
                            Button::new("custom-emoji-card-close")
                                .icon(gpui_kit::assets::IconName::X)
                                .ghost()
                                .small()
                                .tooltip(tr(keys::CLOSE))
                                .accessibility_label(tr(keys::CLOSE))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_custom_emoji_card(cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}

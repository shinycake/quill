//! The live story's comment row (tdesktop `Reply` in video stream mode):
//! comments toggle, send-as, the field ("Comment" or "Comment for N
//! Stars"), emoji, Edit Stars, Send (tinted with the Stars chosen), and
//! the Stars button with everyone's total. With comments off the field is
//! replaced by "Comments disabled."
#![deny(clippy::indexing_slicing)]

use super::live_state::{StarsBoxMode, level_color};
use super::live_view::sender_avatar;
use super::*;
use gpui_kit::assets::IconName;
use quill::lang::trn_with;
use quill::live_story::{CommentsState, level_for_stars};
use quill::premium_hub::group_digits;
use quill::state::Session;
use std::cell::Cell;
use std::rc::Rc;

impl QuillApp {
    /// The comment row, or `None` while the viewer has not joined.
    pub(super) fn live_story_composer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.stories.live.watching?;
        let session = self.session()?;
        let live = session.stories.live.as_ref()?;
        let call = session
            .calls
            .active_group_call
            .as_ref()
            .filter(|call| call.id == live.group_call_id);
        let admin = self.live_story_admin();
        let allowed = call.is_some_and(|call| call.are_messages_allowed && call.can_send_messages);
        let price = if admin {
            0
        } else {
            call.map_or(0, |call| call.paid_message_star_count)
        };
        let chosen = self.stories.live.chosen_stars;
        let total = live.stars_total();
        let toggle = comments_toggle(live.comments.state(), cx);
        let send_as = self.live_send_as_button(session, cx);
        let middle: AnyElement = if allowed || admin {
            self.live_field_row(price, chosen, admin, session, cx)
        } else {
            div()
                .debug_selector(|| "live-story-comments-off".into())
                .flex_1()
                .min_w_0()
                .h(px(40.))
                .flex()
                .items_center()
                .px(px(12.))
                .text_sm()
                .text_color(text_muted())
                .child(tr(keys::VIDEO_STREAM_COMMENTS_DISABLED))
                .into_any_element()
        };
        let stars = div()
            .id("live-story-stars")
            .debug_selector(|| "live-story-stars".into())
            .flex_none()
            .h(px(36.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded_full()
            .bg(rgba(0xffffff1f))
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(tr(keys::PAID_REACTION_TITLE))
            .text_color(rgb(0xffffff))
            .child(
                Icon::new(IconName::StarFill)
                    .with_size(px(18.))
                    .text_color(rgb(0xffb800)),
            )
            .when(total > 0, |this| {
                this.child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(group_digits(total)),
                )
            })
            .on_click(cx.listener(|this, _, _, cx| this.tap_live_stars(cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    let mode = if admin {
                        StarsBoxMode::Admin
                    } else {
                        StarsBoxMode::Reaction
                    };
                    this.open_live_stars_box(mode, 0, false, cx);
                }),
            );
        let panel = self
            .stories
            .live
            .send_as_open
            .then(|| send_as_panel(session, cx));
        Some(
            div()
                .id("live-story-composer")
                .debug_selector(|| "live-story-composer".into())
                .occlude()
                .relative()
                .w(px(360.))
                .flex()
                .items_center()
                .gap(px(4.))
                .children(toggle)
                .children(send_as)
                .child(middle)
                .child(stars)
                .children(panel)
                .into_any_element(),
        )
    }

    /// The field with its emoji, Edit Stars and Send buttons.
    fn live_field_row(
        &self,
        price: i64,
        chosen: i64,
        admin: bool,
        session: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = &self.stories.live.input;
        let placeholder_price = self.stories.live.placeholder.clone();
        let sync = placeholder_sync(input.clone(), placeholder_price, price);
        let level = level_for_stars(&session.live_levels(), chosen);
        let send = div()
            .id("live-story-send-wrap")
            .debug_selector(|| "live-story-send".into())
            .relative()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.stories.live.send_menu = true;
                    cx.notify();
                }),
            )
            .child(
                Button::new("live-story-send")
                    .when(chosen > 0, |button| {
                        button
                            .label(format!("\u{2605}{}", group_digits(chosen)))
                            .bg(level_color(level.first_color, 1.0))
                    })
                    .when(chosen == 0, |button| button.icon(IconName::Send).ghost())
                    .text_color(rgb(0xffffff))
                    .accessibility_label(tr(keys::Q_MEDIA_SEND))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.send_live_comment(window, cx)),
                    ),
            )
            .when(self.stories.live.send_menu, |this| {
                this.child(send_menu(chosen, cx))
            });
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                div().flex_1().min_w_0().child(
                    Textarea::new(input)
                        .aria_label(tr(keys::VIDEO_STREAM_COMMENT_PH))
                        .h(px(40.)),
                ),
            )
            .child(sync)
            .child(
                Button::new("live-story-emoji")
                    .icon(IconName::FaceSlightlySmiling)
                    .ghost()
                    .text_color(text_bright())
                    .accessibility_label(tr(keys::Q_MEDIA_EMOJI_STICKERS_AND_GIFS))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_live_emoji(cx))),
            )
            .when(!admin, |this| {
                this.child(
                    Button::new("live-story-edit-stars")
                        .debug_selector(|| "live-story-edit-stars".into())
                        .icon(IconName::Star)
                        .ghost()
                        .text_color(text_bright())
                        .tooltip(tr(keys::VIDEO_STREAM_EDIT_STARS))
                        .accessibility_label(tr(keys::VIDEO_STREAM_EDIT_STARS))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_live_stars_box(StarsBoxMode::Comment, 0, false, cx);
                        })),
                )
            })
            .child(send)
            .into_any_element()
    }

    /// Your avatar as the comment sender, when there is a choice (tdesktop
    /// shows the send-as button for two or more senders).
    fn live_send_as_button(&self, session: &Session, cx: &mut Context<Self>) -> Option<AnyElement> {
        let senders = session.stories.live.as_ref()?.message_senders.as_ref()?;
        if senders.len() < 2 {
            return None;
        }
        let current = session.live_story_sender()?;
        Some(
            div()
                .id("live-story-send-as")
                .flex_none()
                .cursor_pointer()
                .role(gpui_kit::Role::Button)
                .aria_label(tr(keys::SEND_AS_TITLE))
                .child(sender_avatar(session, current, 28.))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stories.live.send_as_open = !this.stories.live.send_as_open;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }
}

/// Sets the field's placeholder for the comment price once it changes
/// (setting it needs the window, which the draw has).
fn placeholder_sync(
    input: Entity<gpui_kit::component::input::TextareaState>,
    shown: Rc<Cell<i64>>,
    price: i64,
) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |_, (), window, _| {
            if shown.get() == price {
                return;
            }
            shown.set(price);
            let input = input.clone();
            window.on_next_frame(move |window, cx| {
                let text = if price > 0 {
                    trn_with(
                        keys::VIDEO_STREAM_COMMENT_PAID_PH,
                        price,
                        &[("count", &group_digits(price))],
                    )
                } else {
                    tr(keys::VIDEO_STREAM_COMMENT_PH).to_string()
                };
                input.update(cx, |input, cx| input.set_placeholder(text, window, cx));
            });
        },
    )
    .size_0()
    .into_any_element()
}

/// The comments toggle; hidden while there are none (tdesktop
/// `CommentsState::Empty`).
fn comments_toggle(state: CommentsState, cx: &mut Context<QuillApp>) -> Option<AnyElement> {
    let icon = match state {
        CommentsState::Empty => return None,
        CommentsState::Shown => IconName::MessageSquare,
        CommentsState::Hidden => IconName::MessageSquareOff,
        CommentsState::WithNew => IconName::MessageSquareDot,
    };
    Some(
        Button::new("live-story-comments-toggle")
            .icon(icon)
            .ghost()
            .text_color(text_bright())
            .selected(state == CommentsState::Shown)
            .accessibility_label(tr(keys::Q_MEDIA_LIVE_STORY))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_live_comments(cx)))
            .into_any_element(),
    )
}

/// "Send message as..." with the senders TDLib offers.
fn send_as_panel(session: &Session, cx: &mut Context<QuillApp>) -> AnyElement {
    let current = session.live_story_sender();
    let senders = session
        .stories
        .live
        .as_ref()
        .and_then(|live| live.message_senders.clone())
        .unwrap_or_default();
    let rows = senders.into_iter().enumerate().map(|(index, offered)| {
        let sender = offered.sender;
        let chosen = current == Some(sender);
        div()
            .id(("live-story-sender", index as u64))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .py(px(6.))
            .cursor_pointer()
            .when(chosen, |this| this.bg(rgba(0xffffff14)))
            .hover(|style| style.bg(rgba(0xffffff1a)))
            .role(gpui_kit::Role::MenuItem)
            .child(sender_avatar(session, sender, 32.))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .child(session.sender_label(sender)),
            )
            .when(!offered.needs_premium, |this| {
                this.on_click(cx.listener(move |this, _, _, cx| this.pick_live_sender(sender, cx)))
            })
    });
    div()
        .id("live-story-send-as-panel")
        .absolute()
        .bottom(px(48.))
        .left(px(0.))
        .occlude()
        .w(px(260.))
        .py(px(6.))
        .rounded(px(8.))
        .bg(rgb(0x2c333d))
        .shadow_md()
        .text_color(rgb(0xffffff))
        .child(
            div()
                .px(px(12.))
                .py(px(6.))
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr(keys::SEND_AS_TITLE)),
        )
        .children(rows)
        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.stories.live.send_as_open = false;
            cx.notify();
        }))
        .into_any_element()
}

/// The send button's menu: Edit Stars, and Remove Stars once some are
/// chosen.
fn send_menu(chosen: i64, cx: &mut Context<QuillApp>) -> AnyElement {
    let item = |id: &'static str, label: &'static str| {
        div()
            .id(id)
            .px(px(12.))
            .py(px(6.))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0xffffff1a)))
            .role(gpui_kit::Role::MenuItem)
            .child(label)
    };
    div()
        .id("live-story-send-menu")
        .absolute()
        .bottom(px(44.))
        .right(px(0.))
        .occlude()
        .min_w(px(150.))
        .py(px(4.))
        .rounded(px(6.))
        .bg(rgb(0x2c333d))
        .shadow_md()
        .text_sm()
        .text_color(rgb(0xffffff))
        .child(
            item("live-story-menu-edit", tr(keys::VIDEO_STREAM_EDIT_STARS)).on_click(cx.listener(
                |this, _, _, cx| {
                    this.stories.live.send_menu = false;
                    this.open_live_stars_box(StarsBoxMode::Comment, 0, false, cx);
                },
            )),
        )
        .when(chosen > 0, |this| {
            this.child(
                item(
                    "live-story-menu-remove",
                    tr(keys::VIDEO_STREAM_REMOVE_STARS),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stories.live.send_menu = false;
                    this.clear_live_stars(cx);
                })),
            )
        })
        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.stories.live.send_menu = false;
            cx.notify();
        }))
        .into_any_element()
}

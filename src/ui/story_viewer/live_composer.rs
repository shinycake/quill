//! The live story's comment row (tdesktop `ComposeControls` in video
//! stream mode, `updateControlsGeometry` / `paintBackground`): the
//! comments toggle on its own round background, then one rounded field
//! (`storiesComposeBg`, radius 21) holding send-as, the field ("Comment"
//! or "Comment for N Stars"), emoji, Edit Stars and Send (tinted with the
//! Stars chosen), and apart from it the Stars button with everyone's
//! total. With comments off the field says "Comments disabled."
#![deny(clippy::indexing_slicing)]

use super::live_state::{StarsBoxMode, level_color};
use super::live_view::sender_avatar;
use super::*;
use crate::ui::composer_field::field_doc;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem};
use quill::lang::trn_with;
use quill::live_story::{CommentsState, level_for_stars};
use quill::premium_hub::group_digits;
use quill::stat_chart::format_short;
use quill::state::Session;

/// The row's width: the story frame's.
const ROW_W: f32 = 360.;
/// The buttons' height. tdesktop's are 42 in a wider column; 36 keeps
/// room for "Comment for 1,000 Stars" in the story's 360.
const BUTTON: f32 = 36.;
/// tdesktop `commentsSkip` / `starsSkip`.
const SKIP: f32 = 8.;
/// The field's text (`historyComposeField`).
const FIELD_TEXT: f32 = 14.;

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
        let main: AnyElement = if allowed || admin {
            self.live_field_row(price, chosen, admin, session, cx)
        } else {
            div()
                .debug_selector(|| "live-story-comments-off".into())
                .flex_1()
                .min_w_0()
                .h(px(BUTTON))
                .flex()
                .items_center()
                .px(px(12.))
                .rounded(px(BUTTON / 2.))
                .bg(chrome::compose_bg())
                .text_size(px(FIELD_TEXT))
                .text_color(chrome::gray_text())
                .child(tr(keys::VIDEO_STREAM_COMMENTS_DISABLED))
                .into_any_element()
        };
        let stars = self.live_stars_button(total, admin, cx);
        Some(
            div()
                .id("live-story-composer")
                .debug_selector(|| "live-story-composer".into())
                .occlude()
                .relative()
                .w(px(ROW_W))
                .flex()
                .items_center()
                .children(toggle.map(|toggle| div().flex_none().mr(px(SKIP)).child(toggle)))
                .child(main)
                .child(div().flex_none().ml(px(SKIP)).child(stars))
                .into_any_element(),
        )
    }

    /// The Stars button: a star with everyone's total under it (tdesktop
    /// `starsReaction` with `starsReactionCounter`). A click adds one Star,
    /// a right click opens the box.
    fn live_stars_button(&self, total: i64, admin: bool, cx: &mut Context<Self>) -> AnyElement {
        Button::new("live-story-stars")
            .debug_selector(|| "live-story-stars".into())
            .custom(chrome::compose(cx))
            .rounded_full()
            .size(px(BUTTON))
            // The kit's side padding would leave the total no room.
            .p_0()
            .tab_index(0)
            .accessibility_label(tr(keys::PAID_REACTION_TITLE))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        Icon::new(IconName::StarFill)
                            .with_size(px(if total > 0 { 16. } else { 20. }))
                            .text_color(chrome::stars_gold()),
                    )
                    .when(total > 0, |this| {
                        this.child(
                            div()
                                .text_size(px(9.))
                                .line_height(px(10.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(chrome::gray_icon())
                                .child(stars_counter(total)),
                        )
                    }),
            )
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
            )
            .into_any_element()
    }

    /// The rounded field: send-as, the text, emoji, Edit Stars and Send.
    fn live_field_row(
        &self,
        price: i64,
        chosen: i64,
        admin: bool,
        session: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = &self.stories.live.input;
        let empty = input.read(cx).value().is_empty();
        let placeholder = if price > 0 {
            trn_with(
                keys::VIDEO_STREAM_COMMENT_PAID_PH,
                price,
                &[("count", &group_digits(price))],
            )
        } else {
            tr(keys::VIDEO_STREAM_COMMENT_PH).to_string()
        };
        let level = level_for_stars(&session.live_levels(), chosen);
        // Send, or the Stars chosen in the level's color; a right click
        // offers Edit Stars and Remove Stars (tdesktop `SendMenu`).
        let send_button = if chosen > 0 {
            Button::new("live-story-send")
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(level_color(level.first_color, 1.0))
                        .foreground(chrome::fg())
                        .hover(level_color(level.first_color, 0.9))
                        .active(level_color(level.first_color, 0.8))
                        .shadow(false),
                )
                .rounded_full()
                .h(px(28.))
                .px(px(10.))
                .label(format!("\u{2605}{}", format_short(chosen)))
        } else {
            Button::new("live-story-send")
                .custom(chrome::compose(cx).foreground(chrome::blue()))
                .rounded_full()
                .size(px(BUTTON))
                .icon(IconName::Send)
        };
        let send = div()
            .id("live-story-send-wrap")
            .debug_selector(|| "live-story-send".into())
            .flex_none()
            .child(
                send_button
                    .tab_index(0)
                    .accessibility_label(tr(keys::Q_MEDIA_SEND))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.send_live_comment(window, cx)),
                    ),
            );
        let send = if admin {
            send.into_any_element()
        } else {
            let owner = cx.entity().downgrade();
            send.context_menu(move |menu, _, _| {
                let edit = owner.clone();
                let remove = owner.clone();
                menu.item(
                    PopupMenuItem::new(tr(keys::VIDEO_STREAM_EDIT_STARS)).on_click(
                        move |_, _, cx| {
                            let _ = edit.update(cx, |this, cx| {
                                this.open_live_stars_box(StarsBoxMode::Comment, 0, false, cx);
                            });
                        },
                    ),
                )
                .when(chosen > 0, |menu| {
                    menu.item(
                        PopupMenuItem::element(|_, _| {
                            div()
                                .debug_selector(|| "live-story-remove-stars".into())
                                .child(tr(keys::VIDEO_STREAM_REMOVE_STARS))
                        })
                        .on_click(move |_, _, cx| {
                            let _ = remove.update(cx, |this, cx| this.clear_live_stars(cx));
                        }),
                    )
                })
            })
            .into_any_element()
        };
        let field = div()
            .relative()
            .flex_1()
            .min_w_0()
            .pl(px(10.))
            .text_size(px(FIELD_TEXT))
            .child(
                Textarea::new(input)
                    .appearance(false)
                    .bordered(false)
                    .text_color(chrome::fg())
                    .aria_label(placeholder.clone())
                    .token(self.field_token_renderer(input, cx))
                    .h(px(BUTTON)),
            )
            // The placeholder in `storiesComposeGrayText` (the kit's takes
            // the app theme's muted color).
            .when(empty, |this| {
                this.child(
                    div()
                        .debug_selector(|| "live-story-placeholder".into())
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(12.))
                        .right_0()
                        .flex()
                        .items_center()
                        .truncate()
                        .text_color(chrome::gray_text())
                        .child(placeholder),
                )
            });
        let send_as = self.live_send_as_button(session, cx);
        let icon_button = |id: &'static str, icon: IconName, label: &'static str| {
            Button::new(id)
                .custom(chrome::compose(cx))
                .rounded_full()
                .size(px(32.))
                .icon(icon)
                .tab_index(0)
                .accessibility_label(label)
        };
        div()
            .id("live-story-field")
            .debug_selector(|| "live-story-field".into())
            .flex_1()
            .min_w_0()
            .h(px(BUTTON))
            .flex()
            .items_center()
            .rounded(px(BUTTON / 2.))
            .bg(chrome::compose_bg())
            .children(send_as)
            .child(field)
            .child(
                icon_button(
                    "live-story-emoji",
                    IconName::FaceSlightlySmiling,
                    tr(keys::Q_MEDIA_EMOJI_STICKERS_AND_GIFS),
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_live_emoji(cx))),
            )
            .when(!admin, |this| {
                this.child(
                    div()
                        .debug_selector(|| "live-story-edit-stars".into())
                        .child(
                            icon_button(
                                "live-story-edit-stars",
                                IconName::Star,
                                tr(keys::VIDEO_STREAM_EDIT_STARS),
                            )
                            .tooltip(tr(keys::VIDEO_STREAM_EDIT_STARS))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open_live_stars_box(StarsBoxMode::Comment, 0, false, cx);
                            })),
                        ),
                )
            })
            .child(send)
            .into_any_element()
    }

    /// Your avatar as the comment sender, when there is a choice (tdesktop
    /// shows the send-as button for two or more senders); a click opens
    /// "Send message as...".
    fn live_send_as_button(&self, session: &Session, cx: &mut Context<Self>) -> Option<AnyElement> {
        let senders = session.stories.live.as_ref()?.message_senders.clone()?;
        if senders.len() < 2 {
            return None;
        }
        let current = session.live_story_sender()?;
        let owner = cx.entity().downgrade();
        // The menu draws each sender's avatar and name; their labels and
        // photos are read now.
        let rows: Vec<(MessageSender, String, Option<String>, bool)> = senders
            .iter()
            .map(|offered| {
                let sender = offered.sender;
                let photo = match sender {
                    MessageSender::User { user_id } => session.user_photo_path(user_id),
                    MessageSender::Chat { chat_id } => session.chat_photo_path(ChatId(chat_id)),
                };
                (
                    sender,
                    session.sender_label(sender),
                    photo.map(|path| path.to_string()),
                    offered.needs_premium,
                )
            })
            .collect();
        Some(
            Button::new("live-story-send-as")
                .debug_selector(|| "live-story-send-as".into())
                .custom(chrome::compose(cx))
                .rounded_full()
                .size(px(BUTTON))
                .flex_none()
                .tab_index(0)
                .accessibility_label(tr(keys::SEND_AS_TITLE))
                .child(sender_avatar(session, current, 28.))
                .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                    let mut menu = menu.label(tr(keys::SEND_AS_TITLE));
                    for (sender, name, photo, needs_premium) in rows.clone() {
                        let owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_, _| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(crate::ui::message_text::kit_avatar_element(
                                        &name,
                                        photo.as_deref().map(std::path::Path::new),
                                        px(28.),
                                    ))
                                    .child(div().min_w_0().truncate().child(name.clone()))
                            })
                            .checked(sender == current)
                            .disabled(needs_premium)
                            .on_click(move |_, _, cx| {
                                let _ =
                                    owner.update(cx, |this, cx| this.pick_live_sender(sender, cx));
                            }),
                        );
                    }
                    menu
                })
                .into_any_element(),
        )
    }
}

/// The comments toggle on its own round background; hidden while there
/// are none (tdesktop `CommentsState::Empty`).
fn comments_toggle(state: CommentsState, cx: &mut Context<QuillApp>) -> Option<AnyElement> {
    let (icon, label) = match state {
        CommentsState::Empty => return None,
        CommentsState::Shown => (
            IconName::MessageSquare,
            tr(keys::Q_LIVE_STORY_HIDE_COMMENTS),
        ),
        CommentsState::Hidden => (
            IconName::MessageSquareOff,
            tr(keys::Q_LIVE_STORY_SHOW_COMMENTS),
        ),
        CommentsState::WithNew => (
            IconName::MessageSquareDot,
            tr(keys::Q_LIVE_STORY_SHOW_COMMENTS),
        ),
    };
    Some(
        Button::new("live-story-comments-toggle")
            .debug_selector(|| "live-story-comments-toggle".into())
            .custom(chrome::compose(cx))
            .rounded_full()
            .size(px(BUTTON))
            .icon(icon)
            .tab_index(0)
            .accessibility_label(label)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_live_comments(cx)))
            .into_any_element(),
    )
}

impl QuillApp {
    /// The comment field as composer markup (formatting and custom emoji
    /// become the comment's entities when it is sent).
    pub(super) fn live_comment_markup(&self, cx: &App) -> String {
        field_doc(self.stories.live.input.read(cx)).to_markup()
    }
}

/// The total under the star: the full number while it fits the round
/// button, the short form ("1240", "12.3K") past that (tdesktop
/// `SendStarButton::setCount`).
fn stars_counter(total: i64) -> String {
    let full = group_digits(total);
    if full.chars().count() <= 4 {
        full
    } else {
        format_short(total)
    }
}

#[cfg(test)]
mod tests {
    use super::stars_counter;

    #[test]
    fn stars_counter_shortens_past_four_characters() {
        assert_eq!(stars_counter(999), "999");
        assert_eq!(stars_counter(1240), "1240");
        assert_eq!(stars_counter(12_345), "12.3K");
    }
}

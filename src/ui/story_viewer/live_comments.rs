//! The live story's comments over the picture and the pinned paid
//! comments above them (tdesktop `Calls::Group::MessagesUi` in video
//! stream mode). A comment is "**Name** text" with its formatting and
//! custom emoji, a crown with the place before a top donor's name
//! (`MessagesUi::nameText`), the price of a paid one, on the background of
//! its kind; a right click opens the kit menu with the date, Copy Text and
//! Delete (`MessagesUi::showContextMenu`).
#![deny(clippy::indexing_slicing)]

use super::live_state::{
    ADMIN_BADGE_OPACITY, ADMIN_BG_OPACITY, DARK_OVER_OPACITY, PAID_BG_OPACITY, Pinned, level_color,
};
use super::live_view::{FRAME_H, FRAME_W, sender_avatar};
use super::*;
use crate::ui::group_call_panel::BG as GROUP_CALL_BG;
use crate::ui::message_text::custom_emoji_paths;
use crate::ui::video_player::controls::live_pill;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use quill::live_story::{MAX_TOP_SHOWN, level_for_stars};
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::telegram::envelope::{GroupCallMessageLevel, ParsedGroupCallMessage};
use quill::text::{TextEntity, TextEntityKind};
use std::sync::{Arc, OnceLock};

/// tdesktop `groupCallMessagePadding` and `groupCallMessageSkip`.
pub(super) const MESSAGE_SKIP: f32 = 8.;
/// tdesktop `groupCallUserpic` / `groupCallPinnedUserpic`.
const USERPIC: f32 = 20.;
const PINNED_USERPIC: f32 = 22.;
/// tdesktop `groupCallPinnedMaxWidth`.
const PINNED_MAX_TEXT: f32 = 96.;
/// The comment text size (`messageTextStyle`).
pub(super) const TEXT_SIZE: f32 = 13.;
/// What a crown stands in for in the comment's text.
const CROWN_TEXT: &str = "\u{1f451}";

/// The crown's id among the comment's custom emoji. Real ids are
/// positive, so these never meet one.
fn crown_id(place: usize) -> i64 {
    -1 - i64::try_from(place).unwrap_or(0)
}

/// The place's digit as a stroked path inside the crown's body.
fn digit_path(place: usize) -> &'static str {
    match place {
        0 => "M8.8 9.3 L10.3 8.1 L10.3 14.2",
        1 => {
            "M8.4 9.4 C8.6 8.5 9.3 8.1 10.1 8.1 C11 8.1 11.7 8.7 11.7 9.6 C11.7 10.4 \
             11.2 10.9 10.5 11.6 L8.4 14.1 L11.9 14.1"
        }
        _ => {
            "M8.5 8.6 C8.9 8.2 9.5 8.1 10.1 8.1 C11 8.1 11.6 8.6 11.6 9.4 C11.6 10.2 \
             11 10.8 10 10.8 C11.1 10.8 11.8 11.4 11.8 12.4 C11.8 13.5 11 14.2 10 14.2 \
             C9.3 14.2 8.7 14 8.3 13.5"
        }
    }
}

/// tdesktop `CrownMask`: a white crown with the place knocked out of it,
/// so the comment's background shows through the digit.
fn crown_svg(place: usize) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 20 20"><defs><mask id="m" maskUnits="userSpaceOnUse" x="0" y="0" width="20" height="20"><rect width="20" height="20" fill="white"/><path d="{digit}" fill="none" stroke="black" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></mask></defs><path d="M2.2 5.6 L6.3 9.2 L10 3.2 L13.7 9.2 L17.8 5.6 L16.3 16.4 L3.7 16.4 Z" fill="white" stroke="white" stroke-width="1.2" stroke-linejoin="round" mask="url(#m)"/></svg>"##,
        digit = digit_path(place)
    )
}

/// The crowns of the three top places, made once.
fn crown_image(place: usize) -> Option<ImageSource> {
    static CROWNS: OnceLock<Vec<Arc<Image>>> = OnceLock::new();
    CROWNS
        .get_or_init(|| {
            (0..MAX_TOP_SHOWN)
                .map(|place| {
                    Arc::new(Image::from_bytes(
                        ImageFormat::Svg,
                        crown_svg(place).into_bytes(),
                    ))
                })
                .collect()
        })
        .get(place)
        .cloned()
        .map(ImageSource::Image)
}

/// A sender's short name (tdesktop `PeerData::shortName`: a user's first
/// name).
fn short_name(session: &Session, sender: MessageSender) -> String {
    match sender {
        MessageSender::User { user_id } => session
            .user(user_id)
            .map(|user| user.first_name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| session.sender_label(sender)),
        MessageSender::Chat { .. } => session.sender_label(sender),
    }
}

/// "[crown] **Name** text": the comment's text after the name, its
/// entities moved with it.
pub(super) fn comment_line(
    name: &str,
    place: Option<usize>,
    text: &str,
    entities: &[TextEntity],
) -> (String, Vec<TextEntity>) {
    let mut line = String::new();
    let mut out = Vec::new();
    if let Some(place) = place {
        line.push_str(CROWN_TEXT);
        out.push(TextEntity {
            utf8_start: 0,
            utf8_end: line.len(),
            kind: TextEntityKind::CustomEmoji {
                custom_emoji_id: crown_id(place),
            },
        });
        line.push(' ');
    }
    let start = line.len();
    line.push_str(name);
    out.push(TextEntity {
        utf8_start: start,
        utf8_end: line.len(),
        kind: TextEntityKind::Bold,
    });
    line.push(' ');
    let offset = line.len();
    line.push_str(text);
    out.extend(entities.iter().filter_map(|entity| {
        let utf8_start = entity.utf8_start.checked_add(offset)?;
        let utf8_end = entity.utf8_end.checked_add(offset)?;
        (utf8_end <= line.len()).then(|| TextEntity {
            utf8_start,
            utf8_end,
            kind: entity.kind.clone(),
        })
    }));
    (line, out)
}

impl QuillApp {
    /// The comments over the lower half of the frame, newest at the
    /// bottom (tdesktop: left 8, width frame − 16, at most half the frame).
    pub(super) fn live_comments(
        &self,
        session: &Session,
        messages: &[ParsedGroupCallMessage],
        levels: &[GroupCallMessageLevel],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let live = session.stories.live.as_ref();
        let top: Vec<MessageSender> = live
            .and_then(|live| live.donors.as_ref())
            .map(|donors| {
                donors
                    .top_donors
                    .iter()
                    .filter(|donor| !donor.is_anonymous)
                    .filter_map(|donor| donor.sender)
                    .take(MAX_TOP_SHOWN)
                    .collect()
            })
            .unwrap_or_default();
        let failed = live.map(|live| live.failed.clone()).unwrap_or_default();
        let call_id = live.map_or(0, |live| i64::from(live.group_call_id));
        let files = &session.media.files;
        let roots = self.media_display_roots();
        let stickers = &session.stickers.emoji.custom_emoji_stickers;
        let rows: Vec<AnyElement> = messages
            .iter()
            .filter(|m| !m.text.is_empty())
            .map(|message| {
                let place = top.iter().position(|s| *s == message.sender_id);
                let (text, entities) = comment_line(
                    &short_name(session, message.sender_id),
                    place,
                    &message.text,
                    &message.entities,
                );
                let mut emoji = custom_emoji_paths(&entities, Some(stickers), files, &roots);
                if let Some(crown) = place.and_then(crown_image) {
                    emoji.insert(crown_id(place.unwrap_or(0)), crown);
                }
                let body: AnyElement = if failed.contains(&message.message_id) {
                    div()
                        .italic()
                        .child(tr(keys::Q_LIVE_STORY_FAILED))
                        .into_any_element()
                } else {
                    rich_text_line(
                        &text,
                        &entities,
                        (call_id, message.message_id as u64),
                        false,
                        &self.message_ui.spoiler_revealed,
                        px(TEXT_SIZE),
                        &emoji,
                        cx,
                    )
                };
                comment_row(session, message, levels, body, cx)
            })
            .collect();
        div()
            .id("live-story-comments")
            .debug_selector(|| "live-story-comments".into())
            .absolute()
            .left(px(8.))
            .bottom(px(MESSAGE_SKIP))
            .w(px(FRAME_W - 16.))
            .max_h(px(FRAME_H / 2.))
            .overflow_y_scroll()
            .track_scroll(&self.stories.live.scroll)
            .flex()
            .flex_col()
            .items_start()
            .gap(px(MESSAGE_SKIP))
            .children(rows)
            .into_any_element()
    }

    /// Custom emoji in the comments whose sticker is known but whose file
    /// is not on disk yet: one-shot downloads, like chat thumbnails.
    pub(super) fn ensure_live_custom_emoji_downloads(&mut self) {
        let file_ids: Vec<FileId> = {
            let Some(session) = self.session() else {
                return;
            };
            let Some(live) = session.stories.live.as_ref() else {
                return;
            };
            let Some(call) = session
                .calls
                .story_stream
                .as_ref()
                .filter(|call| call.id == live.group_call_id)
            else {
                return;
            };
            let mut ids: Vec<FileId> = call
                .messages
                .iter()
                .flat_map(|message| message.entities.iter())
                .filter_map(|entity| match entity.kind {
                    TextEntityKind::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
                    _ => None,
                })
                .filter_map(|id| {
                    session
                        .stickers
                        .emoji
                        .custom_emoji_stickers
                        .iter()
                        .find(|sticker| sticker.custom_emoji_id == Some(id))
                        .and_then(|sticker| sticker.display_file_id())
                })
                .filter(|file_id| {
                    file_id.0 != 0
                        && !session.media.files.get(&file_id.0).is_some_and(|file| {
                            file.usable_path().is_some() || file.local.is_downloading_active
                        })
                })
                .collect();
            ids.sort_unstable_by_key(|id| id.0);
            ids.dedup();
            ids
        };
        if let Some(live) = self.live.as_mut() {
            for file_id in file_ids {
                let _ = live.driver.download_file(file_id, 1);
            }
        }
    }
}

/// One comment: userpic, the line, the price, on the background of its
/// kind (tdesktop `MessagesUi::setupMessagesWidget` paint).
fn comment_row(
    session: &Session,
    message: &ParsedGroupCallMessage,
    levels: &[GroupCallMessageLevel],
    body: AnyElement,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let stars = message.paid_message_star_count;
    let admin = message.is_from_owner;
    let line = px(TEXT_SIZE * 1.4);
    let radius = (3. + TEXT_SIZE * 1.4 + 2.) / 2.;
    let level = level_for_stars(levels, stars);
    let background = if stars > 0 {
        Some(level_color(level.first_color, PAID_BG_OPACITY))
    } else if admin {
        Some(level_color(GROUP_CALL_BG, ADMIN_BG_OPACITY))
    } else {
        None
    };
    let price = (stars > 0).then(|| {
        div()
            .flex_none()
            .px(px(5.))
            .rounded_full()
            .bg(hsla(0., 0., 0., DARK_OVER_OPACITY))
            .text_size(px(12.))
            .child(format!("\u{2605}{}", group_digits(stars)))
    });
    let admin_badge = admin.then(|| {
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(live_pill())
            .child(
                div()
                    .text_size(px(11.))
                    .opacity(ADMIN_BADGE_OPACITY)
                    .child(tr(keys::ADMIN_BADGE)),
            )
    });
    let message_id = message.message_id;
    let sender = message.sender_id;
    let text = message.text.clone();
    let can_delete = message.can_be_deleted;
    let when = quill::message_menu::sent_label(
        &quill::local_time::civil_local(i64::from(message.date)),
        &quill::local_time::civil_local(quill::local_time::now_unix()),
    );
    let failed = session
        .stories
        .live
        .as_ref()
        .is_some_and(|live| live.failed.contains(&message_id));
    let owner = cx.entity().downgrade();
    div()
        .id(("live-story-comment", message_id as u64))
        .debug_selector(move || format!("live-story-comment-{message_id}"))
        .max_w(px(FRAME_W - 16.))
        .flex()
        .items_start()
        .gap(px(4.))
        .pl(px(2.))
        .pr(px(8.))
        .pt(px(2.))
        .pb(px(2.))
        .rounded(px(radius))
        .when_some(background, |this, bg| this.bg(bg))
        .text_size(px(TEXT_SIZE))
        .line_height(line)
        .text_color(chrome::fg())
        .when(!failed, |this| {
            this.child(
                div()
                    .flex_none()
                    .child(sender_avatar(session, sender, USERPIC)),
            )
        })
        .when(failed, |this| this.pl(px(8.)))
        .child(
            div()
                .min_w_0()
                .pt(px(1.))
                .flex()
                .flex_col()
                .child(div().min_w_0().child(body))
                .children(admin_badge),
        )
        .children(price)
        .context_menu(move |menu, _, _| {
            let copy = owner.clone();
            let delete = owner.clone();
            let text = text.clone();
            menu.label(when.clone())
                .item(
                    PopupMenuItem::new(tr(keys::CONTEXT_COPY_TEXT)).on_click(move |_, _, cx| {
                        let text = text.clone();
                        let _ = copy.update(cx, |this, cx| this.copy_live_comment(text, cx));
                    }),
                )
                .when(can_delete, |menu| {
                    menu.item(
                        PopupMenuItem::element(|_, _| {
                            div()
                                .debug_selector(|| "live-comment-delete".into())
                                .child(tr(keys::BOX_DELETE))
                        })
                        .on_click(move |_, _, cx| {
                            let _ = delete.update(cx, |this, cx| {
                                this.ask_delete_live_comment(message_id, sender, cx);
                            });
                        }),
                    )
                })
        })
        .into_any_element()
}

/// The pinned paid comments in a row at the top: userpic, a short text and
/// the pin time running out (tdesktop `MessagesUi::setupPinnedWidget`).
pub(super) fn live_pinned_strip(
    session: &Session,
    pinned: &[Pinned],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let pills = pinned.iter().map(|pin| {
        let name = session.sender_label(pin.sender);
        let text = if pin.text.is_empty() {
            format!("\u{2605}{}", group_digits(pin.stars))
        } else {
            pin.text.clone()
        };
        div()
            .id(("live-story-pinned", pin.message_id as u64))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.))
            .pl(px(2.))
            .pr(px(10.))
            .py(px(2.))
            .rounded_full()
            .overflow_hidden()
            .bg(level_color(pin.color, PAID_BG_OPACITY))
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(name)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(relative(pin.gone))
                    .bg(hsla(0., 0., 0., DARK_OVER_OPACITY)),
            )
            .child(sender_avatar(session, pin.sender, PINNED_USERPIC))
            .child(
                div()
                    .max_w(px(PINNED_MAX_TEXT))
                    .truncate()
                    .text_size(px(TEXT_SIZE))
                    .text_color(chrome::fg())
                    .child(text),
            )
            .on_click(cx.listener(|this, _, _, cx| this.show_live_comments(cx)))
    });
    div()
        .id("live-story-pinned-strip")
        .debug_selector(|| "live-story-pinned-strip".into())
        .flex()
        .gap(px(MESSAGE_SKIP))
        .overflow_x_scroll()
        .children(pills)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{comment_line, crown_id, crown_svg};
    use quill::text::{TextEntity, TextEntityKind};

    #[test]
    fn the_line_is_the_crown_the_bold_name_and_the_text() {
        let italic = TextEntity {
            utf8_start: 0,
            utf8_end: 2,
            kind: TextEntityKind::Italic,
        };
        let (text, entities) = comment_line("Maya", Some(1), "hi there", &[italic]);
        assert_eq!(text, "\u{1f451} Maya hi there");
        let kinds: Vec<(usize, usize)> = entities
            .iter()
            .map(|e| (e.utf8_start, e.utf8_end))
            .collect();
        // The crown (4 bytes), the name, the italic "hi" moved with it.
        assert_eq!(kinds, vec![(0, 4), (5, 9), (10, 12)]);
        assert!(matches!(
            entities.first().map(|e| &e.kind),
            Some(TextEntityKind::CustomEmoji { custom_emoji_id }) if *custom_emoji_id == crown_id(1)
        ));
        let (text, entities) = comment_line("Noa", None, "yo", &[]);
        assert_eq!(text, "Noa yo");
        assert_eq!(entities.len(), 1);
    }

    #[test]
    fn crowns_never_take_a_real_custom_emoji_id_and_knock_out_the_place() {
        assert!((0..3).all(|place| crown_id(place) < 0));
        assert!(crown_svg(0).contains("mask=\"url(#m)\""));
        assert_ne!(crown_svg(0), crown_svg(2));
    }
}

//! Bubble headers: the reply strip, the "Forwarded from" line and
//! "via @bot" (tdesktop `HistoryMessageReply`, `HistoryMessageForwarded`,
//! `history/history_item_components.cpp`).

use super::app::QuillApp;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::state::{ForwardHeader, ForwardLink, ReplyHeader, ReplyState};
use quill::telegram::envelope::MessageSender;
use std::path::PathBuf;

/// Side of the media thumbnail in a reply strip (tdesktop
/// `st::historyReplyPreview`).
const REPLY_THUMB: f32 = 30.;

/// The strip above a reply's text: the replied-to sender's name in their
/// color, a media thumbnail, and the quote or a one-line preview. A click
/// jumps to the original.
pub(super) fn reply_header_strip(
    row_id: MessageId,
    header: ReplyHeader,
    thumb: Option<PathBuf>,
    // The still of the custom emoji repeated behind the strip, if the
    // replied sender has one and it is downloaded.
    pattern: Option<PathBuf>,
    on_fill: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let color = if on_fill {
        text_on_fill().into()
    } else {
        header
            .accent
            .map(super::chat_theme::peer_name_color)
            .unwrap_or_else(|| accent().into())
    };
    let (body, muted): (Hsla, Hsla) = if on_fill {
        (
            text_on_fill().into(),
            Hsla::from(text_on_fill()).opacity(0.75),
        )
    } else {
        (text_primary().into(), text_muted().into())
    };
    let name = header.name_line();
    let story = header.story;
    let (target_chat, target_id) = (header.target_chat, header.target_id);
    let rgb = color.to_rgb();
    let pattern = pattern.and_then(|path| {
        let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
        super::reply_pattern::copies(&path, [channel(rgb.r), channel(rgb.g), channel(rgb.b)])
    });
    let external = header.external_chat.is_some();
    let clickable = header.clickable;
    let state = header.state;
    let text = super::search_ui::one_line_preview(&header.text);
    let aria = match &name {
        Some(name) => format!("Reply to {name}: {text}"),
        None => format!("Reply: {text}"),
    };
    div()
        .id(("reply-quote", row_id.0 as u64))
        .role(if clickable { Role::Button } else { Role::Group })
        .aria_label(aria)
        .mt_1()
        .mb_1()
        .px_2()
        .py_1()
        .gap_2()
        .flex()
        .items_center()
        .relative()
        .overflow_hidden()
        .rounded_md()
        .border_l_2()
        .border_color(color)
        .bg(color.opacity(0.1))
        .when_some(pattern, |this, copies| {
            this.child(super::reply_pattern::layer(&copies, header.is_quote))
        })
        .when(clickable, |this| {
            this.tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(story) = story {
                        this.open_story_viewer(target_chat, story, cx);
                    } else if external {
                        this.select_search_message(target_chat, target_id, window, cx);
                    } else {
                        this.jump_to_replied_message(target_id, cx);
                    }
                }))
        })
        .when_some(thumb, |this, path| {
            this.child(
                img(super::image_budget::sized_media(
                    &path,
                    (px(REPLY_THUMB), px(REPLY_THUMB)),
                    None,
                    super::image_budget::Fit::Cover,
                ))
                .w(px(REPLY_THUMB))
                .h(px(REPLY_THUMB))
                .rounded_sm()
                .object_fit(ObjectFit::Cover)
                .flex_shrink_0(),
            )
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .when_some(name, |this, name| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(color)
                                    .child(name),
                            )
                            .when(header.is_quote, |this| {
                                this.child(
                                    Icon::new(gpui_kit::assets::IconName::Quote)
                                        .size(px(10.))
                                        .text_color(color)
                                        .flex_none(),
                                )
                            }),
                    )
                })
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(if state == ReplyState::Ready {
                            body
                        } else {
                            muted
                        })
                        .when(state != ReplyState::Ready, |this| this.italic())
                        .child(super::bidi_line::one_line_plain(text)),
                ),
        )
        .into_any_element()
}

/// The "Forwarded from <name>" line (tdesktop `lng_forwarded`): the name is
/// bold and colored, and leads to the origin's profile, chat or post.
pub(super) fn forward_header_line(
    row_id: MessageId,
    header: ForwardHeader,
    via_bot: Option<String>,
    original_date: Option<String>,
    on_fill: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let color: Hsla = if on_fill {
        text_on_fill().into()
    } else {
        header
            .accent
            .map(super::chat_theme::peer_name_color)
            .unwrap_or_else(|| accent().into())
    };
    let muted: Hsla = if on_fill {
        Hsla::from(text_on_fill()).opacity(0.75)
    } else {
        text_muted().into()
    };
    let link = header.link;
    let known = !header.name.is_empty();
    let name = if known {
        header.display_name()
    } else {
        "Forwarded message".to_string()
    };
    let tooltip = match (header.tooltip(), original_date) {
        (Some(note), Some(date)) => Some(format!("{note}\n{date}")),
        (Some(note), None) => Some(note.to_string()),
        (None, Some(date)) => Some(date),
        (None, None) => None,
    };
    let clickable = matches!(
        link,
        ForwardLink::User(_) | ForwardLink::Chat { .. } | ForwardLink::Imported
    );
    let imported_note = header.tooltip().map(str::to_string);
    let name_el = div()
        .id(("forward-name", row_id.0 as u64))
        .font_semibold()
        .text_color(color)
        .child(name.clone())
        .when_some(tooltip, |this, text| {
            this.tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
        })
        .when(clickable, |this| {
            this.role(Role::Button)
                .tab_index(0)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| match link {
                    ForwardLink::User(user_id) => {
                        this.open_avatar_profile(
                            MessageSender::User { user_id: user_id.0 },
                            window,
                            cx,
                        );
                    }
                    ForwardLink::Chat {
                        chat_id,
                        message_id: Some(message_id),
                    } => this.select_search_message(chat_id, message_id, window, cx),
                    ForwardLink::Chat {
                        chat_id,
                        message_id: None,
                    } => this.select_search_chat(chat_id, window, cx),
                    ForwardLink::Imported => {
                        this.status_note = imported_note.clone().unwrap_or_default();
                        cx.notify();
                    }
                    ForwardLink::Hidden | ForwardLink::None => {}
                }))
        });
    div()
        .id(("forward-from", row_id.0 as u64))
        .role(Role::Group)
        .aria_label(format!("Forwarded from {name}"))
        .mt_1()
        .mb_1()
        .flex()
        .flex_wrap()
        .items_baseline()
        .gap_x_1()
        .text_xs()
        .when(known, |this| {
            this.child(div().text_color(muted).child("Forwarded from"))
        })
        .child(name_el)
        .when_some(via_bot, |this, bot| {
            this.child(div().text_color(muted).child(format!("via {bot}")))
        })
        .into_any_element()
}

/// "via @bot" on a message that is not a forward.
pub(super) fn via_bot_line(row_id: MessageId, bot: String, on_fill: bool) -> AnyElement {
    div()
        .id(("via-bot", row_id.0 as u64))
        .mt_1()
        .text_xs()
        .text_color(if on_fill {
            Hsla::from(text_on_fill()).opacity(0.75)
        } else {
            text_muted().into()
        })
        .child(format!("via {bot}"))
        .into_any_element()
}

impl QuillApp {
    /// Local image for a reply strip's thumbnail candidates.
    /// Local still of the custom emoji behind a reply strip.
    pub(super) fn reply_pattern_path(
        header: &ReplyHeader,
        session: Option<&quill::state::Session>,
        files: &std::collections::HashMap<i32, quill::telegram::ParsedFile>,
        media_roots: &[PathBuf],
    ) -> Option<PathBuf> {
        let id = header.background_emoji?;
        let file = session?
            .emoji
            .custom_emoji_stickers
            .iter()
            .find(|sticker| sticker.custom_emoji_id == Some(id))?
            .display_file_id()?;
        files
            .get(&file.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| quill::local_path::sandboxed_display_path(path, media_roots))
    }

    pub(super) fn reply_thumb_path(
        header: &ReplyHeader,
        files: &std::collections::HashMap<i32, quill::telegram::ParsedFile>,
        media_roots: &[PathBuf],
    ) -> Option<PathBuf> {
        header.thumb.iter().find_map(|id| {
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| quill::local_path::sandboxed_display_path(path, media_roots))
        })
    }
}

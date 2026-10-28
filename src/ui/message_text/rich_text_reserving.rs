//! Methods moved out of `message_text.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr};

/// `rich_text_line`, optionally ending the last paragraph with `reserve`
/// of invisible trailing space. The bubble paints its time/receipt footer
/// over that space: the footer shares the last line when it fits and
/// moves to its own line when it doesn't, without measuring text.
pub(in crate::ui) fn rich_text_reserving(
    text: &str,
    entities: &[TextEntity],
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    font: Pixels,
    emoji_paths: &HashMap<i64, ImageSource>,
    // Animated custom emoji for the animation layer (see `paint_text_run`).
    layered: &HashMap<i64, StickerView>,
    reserve: Option<Pixels>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let runs = styled_runs(text, entities);
    let mut column = div()
        .id(("msg-rich-text", msg_key.1 * 2 + is_caption as u64))
        .text_size(font)
        .flex()
        .flex_col()
        .min_w_0();
    // Paragraph = maximal stretch of inline runs; quotes and `pre` blocks
    // break the flow and render as their own blocks.
    let mut index = 0;
    while let Some(run) = runs.get(index) {
        let start = index;
        if run.style.quote {
            while runs.get(index).is_some_and(|run| run.style.quote) {
                index += 1;
            }
            let Some(block) = runs.get(start..index) else {
                break;
            };
            column = column.child(quote_block(
                block,
                start,
                msg_key,
                is_caption,
                revealed,
                emoji_paths,
                layered,
                font,
                cx,
            ));
            continue;
        }
        if run.style.pre {
            column = column.child(paint_text_run(
                run,
                index,
                msg_key,
                is_caption,
                revealed,
                emoji_paths,
                layered,
                font,
                cx,
            ));
            index += 1;
            continue;
        }
        while runs
            .get(index)
            .is_some_and(|run| !run.style.quote && !run.style.pre)
        {
            index += 1;
        }
        let last = index == runs.len();
        let Some(paragraph) = runs.get(start..index) else {
            break;
        };
        column = column.child(inline_paragraph(
            paragraph,
            start,
            msg_key,
            is_caption,
            revealed,
            emoji_paths,
            layered,
            font,
            reserve.filter(|_| last),
            cx,
        ));
    }
    if let Some(reserve) = reserve
        && runs
            .last()
            .is_none_or(|run| run.style.quote || run.style.pre)
    {
        // Text ending in a block: the footer gets a line of its own.
        column = column.child(div().h(font * 1.2).w(reserve));
    }
    column.into_any_element()
}

/// One flowing paragraph of inline runs as a single `StyledText`, so
/// mixed formatting wraps like prose (a bold word mid-sentence stays on
/// its line). A resolved custom emoji is one invisible em-wide glyph in
/// that text, with its image painted over the glyph after layout
/// (`InlineEmoji`), so it wraps and aligns like any other character.
pub(super) fn inline_paragraph(
    runs: &[TextRun],
    first_index: usize,
    msg_key: (i64, u64),
    is_caption: bool,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    emoji_paths: &HashMap<i64, ImageSource>,
    layered: &HashMap<i64, StickerView>,
    font: Pixels,
    reserve: Option<Pixels>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let ParagraphText {
        mut text,
        mut highlights,
        mono,
        click_ranges,
        actions,
        link_flags,
        spoilers,
        inline_emoji,
    } = paragraph_text(
        runs,
        first_index,
        msg_key,
        is_caption,
        revealed,
        emoji_paths,
        layered,
    );
    let label = text.clone();
    // Right-to-left paragraphs align right in the bubble (Telegram Desktop).
    let rtl = quill::text::is_rtl_text(&text);
    if let Some(reserve) = reserve {
        // Em spaces track the font size, so `reserve / font` of them span
        // the footer's width at any text size.
        let count = (reserve / font).ceil().max(1.) as usize;
        let range = text.len()..text.len() + count * '\u{2003}'.len_utf8();
        text.extend(std::iter::repeat_n('\u{2003}', count));
        highlights.push((
            range,
            HighlightStyle {
                color: Some(gpui_kit::transparent_black()),
                ..Default::default()
            },
        ));
    }
    let full: SharedString = text.clone().into();
    let bidi_source = (highlights.clone(), mono.clone());
    let styled = StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(mono);
    let owner = cx.entity().downgrade();
    let actions = Rc::new(actions);
    let press_actions = actions.clone();
    let press_owner = owner.clone();
    let hover_actions = actions.clone();
    let hover_owner = owner.clone();
    let paragraph = super::super::selectable_text::SelectableRichText::new(
        format!("msg-par-{}-{}-{first_index}", msg_key.1, is_caption as u8),
        full,
        styled,
    )
    .bidi(bidi_source.0, bidi_source.1)
    .selection_color(accent().opacity(0.35).into())
    .message(msg_key)
    // Messages read top to bottom by id; paragraphs within one in order.
    .document_order(msg_key.1.saturating_mul(1024) + first_index as u64 * 2 + is_caption as u64)
    .on_click(click_ranges, move |ix, _, cx| {
        let Some(action) = actions.get(ix).cloned() else {
            return;
        };
        let _ = owner.update(cx, |this, cx| match action {
            InlineAction::Link(link) => this.queue_link(link, msg_key, cx),
            InlineAction::CopyCode(text) => this.copy_entity_text(text, cx),
            InlineAction::RevealSpoiler(key) => {
                this.message_ui.spoiler_revealed.insert(key);
                super::super::spoiler_fx::mark_revealed(key);
                cx.notify();
            }
        });
    })
    .spoilers(spoilers)
    .inline_emoji(inline_emoji)
    .link_underline(link_flags, accent_info().into())
    .on_secondary_press(move |ix, position, _, cx| {
        if let Some(InlineAction::Link(link)) = press_actions.get(ix) {
            let link = link.clone();
            let _ = press_owner.update(cx, |this, _| this.note_right_clicked_link(position, link));
        }
    })
    .on_range_hover(move |hover, _, cx| {
        let tooltip = hover.and_then(|(ix, position)| match hover_actions.get(ix) {
            Some(InlineAction::Link(link)) => link.tooltip().map(|text| (position, text)),
            _ => None,
        });
        let _ = hover_owner.update(cx, |this, cx| this.set_link_tooltip(tooltip, cx));
    });
    // `text_right` does not move this element: it paints its own glyphs.
    // A media bubble is wider than the caption, and without `justify_end`
    // a Hebrew block stays on the left (Telegram Desktop right-aligns it).
    div()
        .id(format!(
            "msg-par-wrap-{}-{}-{first_index}",
            msg_key.1, is_caption as u8
        ))
        .role(Role::Label)
        .aria_label(label)
        .min_w_0()
        .when(rtl, |this| this.w_full().flex().justify_end())
        .child(paragraph)
        .into_any_element()
}

/// Phase 4.1: plain/link message text (with optional link-preview card).
/// `msg_key` is (chat id, message id); the spoiler-reveal lookup needs the
/// chat id because message ids are only unique within a chat.
pub(in crate::ui) fn message_text_block(
    msg_key: (i64, u64),
    text: &quill::telegram::envelope::TextContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    // Resolved custom emoji stickers for inline rendering (EmojiPanel cache).
    custom_emoji: Option<&quill::emoji::CustomEmojiStickers>,
    // Animated frames of this message's custom emoji, when decoded; they
    // replace the still images.
    animated_emoji: &HashMap<i64, super::super::sticker_player::StickerView>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    big_emoji: bool,
    // Trailing space for the bubble's inline time footer (see
    // `rich_text_reserving`); only when the text is the bubble's last part.
    reserve: Option<Pixels>,
    // How wide the bubble may get: the link preview card fits it.
    room: crate::ui::history::bubble_width::BubbleRoom,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = msg_key.1;
    let font = if big_emoji && text.link_preview.is_none() {
        match quill::emoji_catalog::big_emoji_count_with_entities(&text.text, &text.entities) {
            // Telegram Desktop `largeEmojiSize`: the same size for one,
            // two or three emoji.
            Some(1..=3) => font.max(px(quill::bubble_layout::LARGE_EMOJI_SIZE as f32)),
            _ => font,
        }
    } else {
        font
    };
    let above = text
        .link_preview
        .as_ref()
        .is_some_and(|preview| preview.show_above_text);
    let card_below = !above
        && text
            .link_preview
            .as_ref()
            .is_some_and(|preview| preview.has_card());
    let images = custom_emoji_paths(&text.entities, custom_emoji, files, media_roots);
    let layered = animated_emoji;
    let line = rich_text_reserving(
        &text.text,
        &text.entities,
        msg_key,
        false,
        revealed,
        font,
        &images,
        layered,
        reserve.filter(|_| !card_below),
        cx,
    );
    let card = text.link_preview.as_ref().and_then(|preview| {
        preview.has_card().then(|| {
            link_preview_card(
                row_id,
                preview,
                files,
                downloading,
                media_roots,
                custom_emoji,
                font,
                room,
                cx,
            )
        })
    });
    let has_text = !text.text.is_empty();
    let mut block = div().id(("msg-text-block", row_id)).flex().flex_col();
    if above {
        block = block.when_some(card, |this, card| this.child(card));
        if has_text {
            block = block.child(line);
        }
    } else {
        if has_text {
            block = block.child(line);
        }
        block = block.when_some(card, |this, card| this.child(card));
    }
    // A lone emoji plays a larger effect when clicked (`emoji_effect`).
    if big_emoji
        && text.link_preview.is_none()
        && text.entities.is_empty()
        && quill::emoji_catalog::big_emoji_count(&text.text) == Some(1)
    {
        let glyph = f32::from(font);
        return super::super::emoji_effect::tappable(block, msg_key, Some(glyph), cx);
    }
    block.into_any_element()
}

pub(in crate::ui) fn link_preview_card(
    row_id: u64,
    preview: &quill::telegram::envelope::LinkPreview,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    custom_emoji: Option<&quill::emoji::CustomEmojiStickers>,
    // Settings → Appearance: message font size.
    font: Pixels,
    room: crate::ui::history::bubble_width::BubbleRoom,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let site_empty = preview.site_name.is_empty();
    let title_empty = preview.title.is_empty();
    let description_empty = preview.description.is_empty();
    let site = preview.site_name.clone();
    let title = preview.title.clone();
    let description = preview.description.clone();
    let display = if preview.display_url.is_empty() {
        preview.url.clone()
    } else {
        preview.display_url.clone()
    };
    let small = font * (12.0 / 14.0);
    let thumb = preview.photo.as_ref().map(|photo| {
        let frame = preview_frame(preview, photo, font, small, room, cx);
        preview_thumb(row_id, photo, frame, files, downloading, media_roots)
    });
    // MED4: embedded players get a play/duration badge over the
    // thumbnail (schema:4434/:4443/:4452). Tap opens the embed URL in
    // the browser — inline playback is out of slice (DECISIONS.md MED4).
    let thumb = match &preview.kind {
        quill::telegram::envelope::LinkPreviewKind::EmbeddedPlayer {
            duration_secs,
            audio,
            ..
        } => thumb.map(|thumb| {
            let badge = if *duration_secs > 0 {
                format!(
                    "{} {}:{:02}",
                    if *audio { "♪" } else { "▶" },
                    duration_secs / 60,
                    duration_secs % 60
                )
            } else if *audio {
                "♪".to_string()
            } else {
                "▶".to_string()
            };
            div()
                .relative()
                .child(thumb)
                .child(
                    div()
                        .absolute()
                        .bottom_1()
                        .right_1()
                        .px_1()
                        .rounded_sm()
                        .bg(bg_black())
                        .text_xs()
                        .text_color(text_on_fill())
                        .child(badge),
                )
                .into_any_element()
        }),
        _ => thumb,
    };
    let thumb = thumb.or_else(|| {
        super::link_preview_tile::style_emoji(
            row_id,
            preview,
            custom_emoji,
            files,
            media_roots,
            font,
        )
    });
    // MED4: album previews (`linkPreviewTypeAlbum`, schema:4392) show a
    // strip of up to 4 thumbnails under the card copy.
    let album_strip: Option<AnyElement> = match &preview.kind {
        quill::telegram::envelope::LinkPreviewKind::Album { thumbnails }
            if !thumbnails.is_empty() =>
        {
            let mut strip = div()
                .id(("link-preview-album", row_id))
                .flex()
                .gap_1()
                .mt_1();
            // Up to four 72 px tiles 4 px apart; a narrow card shrinks them.
            let tiles = thumbnails.len().min(4) as i32;
            let fit = (crate::ui::history::bubble_width::preview_inner_max(room) - 4 * (tiles - 1))
                / tiles.max(1);
            let side = px(fit.clamp(1, 72) as f32);
            for (index, thumb_photo) in thumbnails.iter().enumerate() {
                strip = strip.child(preview_thumb(
                    row_id * 100 + index as u64,
                    thumb_photo,
                    ThumbFrame::Fixed(side, side),
                    files,
                    downloading,
                    media_roots,
                ));
            }
            Some(strip.into_any_element())
        }
        _ => None,
    };
    // Settings → Appearance: the card copy scales with the message
    // font size — the title keeps body size, the meta lines stay one
    // step smaller (12px vs 14px at the default).
    let mut copy = div()
        .id(("link-preview-copy", row_id))
        .flex()
        .flex_col()
        .min_w_0()
        .gap_0();
    if !site_empty {
        copy = copy.child(
            div()
                .text_size(small)
                .font_medium()
                .text_color(accent())
                .child(site),
        );
    }
    if !title_empty {
        copy = copy.child(
            div()
                .text_size(font)
                .font_medium()
                .text_color(text_primary())
                .child(title),
        );
    }
    if !description_empty {
        copy = copy.child(
            div()
                .text_size(small)
                .text_color(text_primary())
                .child(description),
        );
    }
    if site_empty && title_empty && description_empty && !display.is_empty() {
        copy = copy.child(div().text_size(small).text_color(accent()).child(display));
    }
    // A wallpaper or theme link shows its picture under the copy
    // (`link_preview_tile`).
    let special = super::link_preview_tile::picture(row_id, preview, files, media_roots);
    let body = if let Some(picture) = special {
        div()
            .id(("link-preview-special", row_id))
            .flex()
            .flex_col()
            .gap_1()
            .child(copy)
            .child(picture)
            .into_any_element()
    } else if preview.show_large_media {
        let mut column = div()
            .id(("link-preview-large", row_id))
            .flex()
            .flex_col()
            .gap_1();
        if preview.show_media_above_description {
            column = column.when_some(thumb, |this, thumb| this.child(thumb));
            column = column.child(copy);
        } else {
            column = column.child(copy);
            column = column.when_some(thumb, |this, thumb| this.child(thumb));
        }
        column
            .when_some(album_strip, |this, strip| this.child(strip))
            .into_any_element()
    } else {
        div()
            .id(("link-preview-small", row_id))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(copy.flex_1())
                    .when_some(thumb, |this, thumb| this.child(thumb)),
            )
            .when_some(album_strip, |this, strip| this.child(strip))
            .into_any_element()
    };
    let preview_for_tap = preview.clone();
    div()
        .id(("link-preview", row_id))
        .role(Role::Link)
        .aria_label(tr(keys::Q_MISC_OPEN_LINK_PREVIEW))
        .tab_index(0)
        .mt_2()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(accent())
        .bg(bg_canvas())
        .cursor_pointer()
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, _, cx| {
            // MED4: `instant_view_version > 0` (schema:4570) opens the IV
            // reader (mode-gated); otherwise the browser. Embedded
            // players open their embed URL instead of the page URL.
            this.open_preview_url(&preview_for_tap, cx);
        }))
        .child(body)
        .when_some(
            preview.button_label(quill::local_time::now_unix()),
            |this, label| {
                // Telegram Desktop's call-to-action under entity previews. The
                // whole card is the click target, so this is a label.
                this.child(
                    div()
                        .id(("link-preview-view", row_id))
                        .mt_1()
                        .pt_1()
                        .border_t_1()
                        .border_color(text_muted().opacity(0.25))
                        .text_size(small)
                        .font_semibold()
                        .text_center()
                        .text_color(accent())
                        .child(label),
                )
            },
        )
        .into_any_element()
}

/// The box of a link preview's picture, as `WebPage::countCurrentSize`
/// sizes it: the large photo across the card's inside
/// (`bubble_width::preview_photo_frame`), the small thumbnail as tall as
/// the copy beside it up to five lines and at most square
/// (`bubble_width::article_thumb`).
fn preview_frame(
    preview: &quill::telegram::envelope::LinkPreview,
    photo: &quill::telegram::envelope::PhotoContent,
    font: Pixels,
    small: Pixels,
    room: crate::ui::history::bubble_width::BubbleRoom,
    cx: &App,
) -> ThumbFrame {
    use crate::ui::history::bubble_width::{
        ArticleCopy, article_thumb, preview_inner_max, preview_photo_frame,
    };
    use crate::ui::history::text_measure::longest_line_width;
    let dims = photo
        .largest_size()
        .or_else(|| photo.thumb_size())
        .map_or((0, 0), |size| (size.width, size.height));
    if preview.show_large_media {
        let (w, h) = preview_photo_frame(dims, room);
        return ThumbFrame::Fill {
            min_width: px(w as f32),
            ratio: w as f32 / h.max(1) as f32,
        };
    }
    let measure =
        |text: &str, size: Pixels| longest_line_width(cx, text, size, FontWeight::NORMAL, 0);
    let copy = ArticleCopy {
        site: measure(&preview.site_name, small),
        title: measure(&preview.title, font),
        description: preview
            .description
            .split('\n')
            .filter(|_| !preview.description.is_empty())
            .map(|line| measure(line, small))
            .collect(),
        // The bubble's line height (`relative(1.4)`).
        title_line: f32::from(font) * 1.4,
        small_line: f32::from(small) * 1.4,
    };
    let (w, h) = article_thumb(dims, &copy, preview_inner_max(room));
    ThumbFrame::Fixed(px(w as f32), px(h as f32))
}

/// M2: render a `messageRichMessage` (schema 1.8.67, line 5143) as a stack
/// of `pageBlock*` elements. Styled text reuses `rich_text_line` (M1
/// entities); inline buttons and button rows reuse
/// `inline_keyboard_button` (URL / callback / switchInline / copy
/// handlers — the same honest tap behavior as reply-markup keyboards).
/// A partial message (`is_full == false`, schema line 123) offers a
/// "Load full message" button that fetches the rest via
/// `getFullRichMessage` instead of pretending to be complete.
pub(in crate::ui) fn message_rich_block(
    msg_key: (i64, u64),
    chat_id: ChatId,
    message_id: MessageId,
    rich: &quill::telegram::envelope::RichMessageContent,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = msg_key.1;
    let mut stack = div()
        .id(format!("msg-rich-block-{row_id}"))
        .flex()
        .flex_col()
        .gap_2();
    for (index, block) in rich.blocks.iter().enumerate() {
        if let Some(child) = rich_block_element(
            index, block, msg_key, chat_id, message_id, revealed, font, cx,
        ) {
            stack = stack.child(child);
        }
    }
    if !rich.is_full {
        stack = stack.child(
            Button::new(format!("rich-load-full-{row_id}"))
                .label(tr(keys::Q_MISC_LOAD_FULL_MESSAGE))
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        match live.driver.fetch_full_rich_message(chat_id, message_id) {
                            Ok(_) => this.connection.set_note(StatusNote::silent(tr(
                                keys::Q_MISC_LOADING_FULL_MESSAGE,
                            ))),
                            Err(_) => this.connection.set_note(StatusNote::error(tr(
                                keys::Q_MISC_COULD_NOT_LOAD_FULL_MESSAGE,
                            ))),
                        }
                        cx.notify();
                    }
                })),
        );
    }
    stack.into_any_element()
}

/// M2: inline photo/video rich block — an emoji tile with the caption,
/// mirroring the document tile above.
pub(super) fn rich_media_tile(
    row_id: u64,
    index: usize,
    kind: &str,
    emoji: &str,
    caption: &str,
) -> AnyElement {
    // No caption → label the tile with the media kind instead of an empty row.
    let label = if caption.is_empty() {
        match kind {
            "photo" => tr(keys::Q_MISC_PHOTO),
            "video" => tr(keys::MEDIA_VIDEO),
            _ => kind,
        }
        .to_string()
    } else {
        caption.to_string()
    };
    div()
        .id(format!("rich-{kind}-{row_id}-{index}"))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_sm().child(emoji.to_string()))
                .child(div().text_sm().font_medium().child(label)),
        )
        .into_any_element()
}

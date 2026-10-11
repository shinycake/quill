//! Methods moved out of `message_text.rs` to keep files under 1000 lines.

use super::*;

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
    layered: &HashMap<i64, LayeredClip>,
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
    while index < runs.len() {
        let start = index;
        if runs[index].style.quote {
            while index < runs.len() && runs[index].style.quote {
                index += 1;
            }
            column = column.child(quote_block(
                &runs[start..index],
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
        if runs[index].style.pre {
            column = column.child(paint_text_run(
                &runs[index],
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
        while index < runs.len() && !runs[index].style.quote && !runs[index].style.pre {
            index += 1;
        }
        let last = index == runs.len();
        column = column.child(inline_paragraph(
            &runs[start..index],
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
    layered: &HashMap<i64, LayeredClip>,
    font: Pixels,
    reserve: Option<Pixels>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let mut text = String::new();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut mono: Vec<(std::ops::Range<usize>, SharedString)> = Vec::new();
    let mut click_ranges = Vec::new();
    let mut actions = Vec::new();
    // Per click range: whether it is a link (underlined on hover).
    let mut link_flags: Vec<bool> = Vec::new();
    let mut spoilers: Vec<(std::ops::Range<usize>, f32)> = Vec::new();
    let mut inline_emoji: Vec<super::super::selectable_text::InlineEmoji> = Vec::new();
    for (offset, run) in runs.iter().enumerate() {
        if run.text.is_empty() {
            continue;
        }
        let style = &run.style;
        let key = (
            msg_key.0,
            msg_key.1,
            (first_index + offset) as u64,
            is_caption,
        );
        let hidden = style.spoiler && !revealed.contains(&key);
        // A resolved custom emoji (animated first) becomes a placeholder
        // glyph; unresolved ones and hidden spoilers keep their text.
        let visual = run.custom_emoji_id.filter(|_| !hidden).and_then(|id| {
            layered
                .get(&id)
                .map(|clip| super::super::selectable_text::InlineEmojiVisual::Clip(clip.clone()))
                .or_else(|| {
                    emoji_paths.get(&id).map(|source| {
                        super::super::selectable_text::InlineEmojiVisual::Image(source.clone())
                    })
                })
        });
        let range = if visual.is_some() {
            let at = text.len();
            text.push(super::super::selectable_text::EMOJI_PLACEHOLDER);
            at..text.len()
        } else {
            let at = text.len();
            text.push_str(&run.text);
            at..text.len()
        };
        if let Some(visual) = visual {
            // The picture carries no text styling; a link around it keeps
            // its click target.
            if let Some(link) = &run.link {
                click_ranges.push(range.clone());
                actions.push(InlineAction::Link(link.clone()));
                link_flags.push(true);
            }
            inline_emoji.push(super::super::selectable_text::InlineEmoji {
                range,
                visual,
                fallback: run.text.clone(),
            });
            continue;
        }
        let mut highlight = HighlightStyle::default();
        if style.bold {
            highlight.font_weight = Some(FontWeight::BOLD);
        }
        if style.italic {
            highlight.font_style = Some(FontStyle::Italic);
        }
        if style.underline {
            highlight.underline = Some(UnderlineStyle {
                thickness: px(1.),
                ..Default::default()
            });
        }
        if style.strikethrough {
            highlight.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.),
                ..Default::default()
            });
        }
        if style.code {
            highlight.background_color = Some(fill_muted().into());
            mono.push((range.clone(), MONO_FONT.into()));
        }
        // tdesktop hides spoiler text under drifting specks of its color;
        // a revealed run's specks fade out over the text.
        if hidden {
            spoilers.push((range.clone(), 1.));
        } else if style.spoiler
            && let Some(fade) = super::super::spoiler_fx::reveal_fade(key)
        {
            spoilers.push((range.clone(), fade));
        }
        if hidden {
            // GPUI blends a highlight color over the text's, so a clear
            // color changes nothing: fade the glyphs out instead.
            highlight.color = None;
            highlight.fade_out = Some(1.);
            highlight.background_color = None;
            click_ranges.push(range.clone());
            actions.push(InlineAction::RevealSpoiler(key));
            link_flags.push(false);
        } else if let Some(link) = &run.link {
            highlight.color = Some(accent_info().into());
            click_ranges.push(range.clone());
            actions.push(InlineAction::Link(link.clone()));
            link_flags.push(true);
        } else if style.code && !style.pre {
            click_ranges.push(range.clone());
            actions.push(InlineAction::CopyCode(run.text.clone()));
            link_flags.push(false);
        }
        if highlight != HighlightStyle::default() {
            highlights.push((range, highlight));
        }
    }
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
    div()
        .id(format!(
            "msg-par-wrap-{}-{}-{first_index}",
            msg_key.1, is_caption as u8
        ))
        .role(Role::Label)
        .aria_label(label)
        .min_w_0()
        .when(rtl, |this| this.w_full().text_right())
        .child(paragraph)
        .into_any_element()
}

/// Phase 4.1: plain/link message text (with optional link-preview card).
/// `msg_key` is (chat id, message id); the spoiler-reveal lookup needs the
/// chat id because message ids are only unique within a chat.
pub(in crate::ui) fn message_text_block(
    msg_key: (i64, u64),
    text: &quill::telegram::envelope::TextContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    // Resolved custom emoji stickers for inline rendering (EmojiPanel cache).
    custom_emoji: &[StickerItem],
    // Animated frames of this message's custom emoji, when decoded; they
    // replace the still images.
    animated_emoji: &HashMap<i64, super::super::sticker_playback::AnimatedVisual>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    big_emoji: bool,
    // Trailing space for the bubble's inline time footer (see
    // `rich_text_reserving`); only when the text is the bubble's last part.
    reserve: Option<Pixels>,
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
    let mut images = custom_emoji_paths(&text.entities, custom_emoji, files, media_roots);
    let mut layered = HashMap::new();
    for (id, visual) in animated_emoji {
        match visual {
            super::super::sticker_playback::AnimatedVisual::Image(frame) => {
                images.insert(*id, ImageSource::from(frame.clone()));
            }
            super::super::sticker_playback::AnimatedVisual::Layered(clip) => {
                layered.insert(*id, clip.clone());
            }
        }
    }
    let line = rich_text_reserving(
        &text.text,
        &text.entities,
        msg_key,
        false,
        revealed,
        font,
        &images,
        &layered,
        reserve.filter(|_| !card_below),
        cx,
    );
    let card = text.link_preview.as_ref().and_then(|preview| {
        preview
            .has_card()
            .then(|| link_preview_card(row_id, preview, files, downloading, media_roots, font, cx))
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
    block.into_any_element()
}

pub(in crate::ui) fn link_preview_card(
    row_id: u64,
    preview: &quill::telegram::envelope::LinkPreview,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    // Settings → Appearance: message font size.
    font: Pixels,
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
    let thumb = preview.photo.as_ref().map(|photo| {
        preview_thumb(
            row_id,
            photo,
            preview.show_large_media,
            files,
            downloading,
            media_roots,
        )
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
            for (index, thumb_photo) in thumbnails.iter().enumerate() {
                strip = strip.child(preview_thumb(
                    row_id * 100 + index as u64,
                    thumb_photo,
                    false,
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
    let small = font * (12.0 / 14.0);
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
    let body = if preview.show_large_media {
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
        .aria_label("Open link preview")
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
        .when_some(preview.view_button, |this, label| {
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
        })
        .into_any_element()
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
                .label("Load full message")
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(live) = this.live.as_mut() {
                        match live.driver.fetch_full_rich_message(chat_id, message_id) {
                            Ok(_) => this.connection.status_note = "loading full message…".into(),
                            Err(_) => {
                                this.connection.status_note = "could not load full message".into()
                            }
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
            "photo" => "Photo",
            "video" => "Video",
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

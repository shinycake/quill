//! Methods moved out of `message_text.rs` to keep files under 1000 lines.

use super::*;

/// M2: one `RichBlock` as an element. `None` for invisible/unsupported
/// blocks (`pageBlockAnchor`, unknown types) — parsed, never rendered as
/// fake content.
#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn rich_block_element(
    index: usize,
    block: &RichBlock,
    msg_key: (i64, u64),
    chat_id: ChatId,
    message_id: MessageId,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    // Settings → Appearance: message font size.
    font: Pixels,
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let row_id = msg_key.1;
    match block {
        RichBlock::Paragraph {
            text,
            entities,
            buttons,
        } => {
            let mut col = div()
                .id(format!("rich-para-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1();
            if !text.is_empty() {
                col = col.child(rich_text_line(
                    text,
                    entities,
                    msg_key,
                    false,
                    revealed,
                    font,
                    // Instant View richText* has no custom emoji entities
                    // (richTextCustomEmoji degrades to alternative_text).
                    &HashMap::new(),
                    cx,
                ));
            }
            for (button_index, button) in buttons.iter().enumerate() {
                col = col.child(
                    div()
                        .id(format!("rich-para-btn-{row_id}-{index}-{button_index}"))
                        .flex()
                        .child(
                            inline_keyboard_button(
                                chat_id,
                                message_id,
                                index,
                                button_index,
                                button,
                                None,
                                cx,
                            )
                            .flex_1(),
                        ),
                );
            }
            Some(col.into_any_element())
        }
        RichBlock::Heading {
            level,
            text,
            entities,
        } => {
            let heading = div()
                .id(format!("rich-heading-{row_id}-{index}"))
                .font_semibold();
            let heading = match level {
                1 => heading.text_xl(),
                2 => heading.text_lg(),
                _ => heading.text_base(),
            };
            Some(
                heading
                    .child(rich_text_line(
                        text,
                        entities,
                        msg_key,
                        false,
                        revealed,
                        font,
                        // Instant View richText* has no custom emoji entities.
                        &HashMap::new(),
                        cx,
                    ))
                    .into_any_element(),
            )
        }
        RichBlock::List { ordered, items } => {
            let mut col = div()
                .id(format!("rich-list-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1();
            for (item_index, item) in items.iter().enumerate() {
                let marker = match item.checked {
                    Some(true) => "☑",
                    Some(false) => "☐",
                    None if *ordered => &format!("{}.", item_index + 1),
                    None => "•",
                };
                col = col.child(
                    div()
                        .id(format!("rich-list-item-{row_id}-{index}-{item_index}"))
                        .flex()
                        .gap_2()
                        .child(div().text_sm().child(marker.to_string()))
                        .child(div().text_sm().child(item.text.clone())),
                );
            }
            Some(col.into_any_element())
        }
        RichBlock::Collapsible { header, body, .. } => Some(
            div()
                .id(format!("rich-details-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().font_semibold().child(format!("▾ {header}")))
                .child(
                    div()
                        .ml_4()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(body.clone()),
                )
                .into_any_element(),
        ),
        RichBlock::Document {
            file_name, caption, ..
        } => {
            let mut col = div()
                .id(format!("rich-document-{row_id}-{index}"))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_sm().child("📄"))
                        .child(div().text_sm().font_medium().child(file_name.clone())),
                );
            if !caption.is_empty() {
                col = col.child(div().text_xs().child(caption.clone()));
            }
            Some(col.into_any_element())
        }
        // Inline photos/videos mirror the document tile: emoji + caption.
        // (Thumbnails need `media_roots` plumbing through both render call
        // sites — future work, not needed for parity here.)
        RichBlock::Photo { caption, .. } => {
            Some(rich_media_tile(row_id, index, "photo", "📷", caption))
        }
        RichBlock::Video { caption, .. } => {
            Some(rich_media_tile(row_id, index, "video", "🎬", caption))
        }
        RichBlock::Table { rows } => {
            // Compact bordered grid: hairline dividers, tight cell padding,
            // header row (row 0 — matches rich.rs serialize `is_header`)
            // in semibold on a muted band.
            let border = cx.theme().border;
            let mut table = div()
                .id(format!("rich-table-{row_id}-{index}"))
                .flex()
                .flex_col()
                .border_1()
                .border_color(border)
                .rounded_md()
                .overflow_hidden();
            for (row_index, row) in rows.iter().enumerate() {
                let mut line = div()
                    .id(format!("rich-table-row-{row_id}-{index}-{row_index}"))
                    .flex();
                if row_index > 0 {
                    line = line.border_t_1().border_color(border);
                }
                for (cell_index, cell) in row.iter().enumerate() {
                    let mut cell_div = div()
                        .id(format!(
                            "rich-table-cell-{row_id}-{index}-{row_index}-{cell_index}"
                        ))
                        .flex_1()
                        .px_2()
                        .py_1()
                        .text_sm()
                        .child(cell.clone());
                    if cell_index > 0 {
                        cell_div = cell_div.border_l_1().border_color(border);
                    }
                    if row_index == 0 {
                        cell_div = cell_div.font_semibold().bg(fill_muted());
                    }
                    line = line.child(cell_div);
                }
                table = table.child(line);
            }
            Some(table.into_any_element())
        }
        RichBlock::ButtonRow { buttons } => {
            let mut line = div()
                .id(format!("rich-button-row-{row_id}-{index}"))
                .flex()
                .gap_1();
            for (button_index, button) in buttons.iter().enumerate() {
                line = line.child(
                    inline_keyboard_button(
                        chat_id,
                        message_id,
                        index,
                        button_index,
                        button,
                        None,
                        cx,
                    )
                    .flex_1(),
                );
            }
            Some(line.into_any_element())
        }
        RichBlock::Divider => Some(
            div()
                .id(format!("rich-divider-{row_id}-{index}"))
                .h_px()
                .w_full()
                .bg(cx.theme().border)
                .into_any_element(),
        ),
        RichBlock::Empty | RichBlock::Unsupported { .. } => None,
    }
}

pub(in crate::ui) fn preview_thumb(
    row_id: u64,
    photo: &quill::telegram::envelope::PhotoContent,
    large: bool,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
) -> AnyElement {
    let (w, h) = if large {
        (px(240.), px(140.))
    } else {
        (px(72.), px(72.))
    };
    if let Some(path) = photo_display_path(photo, files, media_roots) {
        return img(super::super::image_budget::sized_media(
            &path,
            (w, h),
            photo
                .largest_size()
                .or_else(|| photo.thumb_size())
                .map(|size| (size.width, size.height)),
            super::super::image_budget::Fit::Cover,
        ))
        .id(("link-preview-img", row_id))
        .w(w)
        .h(h)
        .aspect_ratio(w / h)
        .rounded_md()
        .object_fit(ObjectFit::Cover)
        .flex_shrink_0()
        .with_fallback(move || {
            div()
                .w(w)
                .h(h)
                .rounded_md()
                .bg(fill_muted())
                .into_any_element()
        })
        .into_any_element();
    }
    let file_id = photo
        .thumb_size()
        .map(|size| size.file_id)
        .unwrap_or(FileId(0));
    let label = if file_is_downloading(file_id, files, downloading) {
        "…"
    } else {
        "Preview"
    };
    div()
        .id(("link-preview-ph", row_id))
        .w(w)
        .h(h)
        .rounded_md()
        .bg(fill_muted())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .child(div().text_xs().text_color(text_bright()).child(label))
        .into_any_element()
}

/// Full local timestamp (`28 September 2026, 21:42`).
pub(in crate::ui) fn format_unix_date_time(unix: i64) -> String {
    quill::local_time::full_stamp(&quill::local_time::civil_local(unix))
}

#[allow(dead_code)]
pub(in crate::ui) fn reply_quote_strip(
    row_id: MessageId,
    target_id: MessageId,
    preview: String,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    div()
        .id(("reply-quote", row_id.0 as u64))
        .role(Role::Button)
        .aria_label(format!("Go to replied message: {preview}"))
        .tab_index(0)
        .mt_1()
        .mb_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_l_2()
        .border_color(accent())
        .bg(bg_canvas())
        .cursor_pointer()
        .pressable(cx.theme())
        .on_click(cx.listener(move |this, _, _, cx| {
            this.jump_to_replied_message(target_id, cx);
        }))
        .child(
            div()
                .text_xs()
                .font_medium()
                .text_color(accent())
                .child("Reply"),
        )
        .child(div().text_xs().truncate().text_color(text_primary()).child(
            super::super::bidi_line::one_line_plain(super::super::search_ui::one_line_preview(
                &preview,
            )),
        ))
        .into_any_element()
}

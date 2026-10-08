//! Slice chatlist-list-style: the chat-list row preview line renderer.
//! Pure style inputs (`icon`, `entities`) come from
//! `quill::chatlist_style`; this module only paints them.

use super::message_text::MONO_FONT;
use super::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::text::{TextEntity, styled_runs};
/// The chat-row preview line — optional media icon plus the preview
/// text, plain or formatted via `styled_runs`. Single line: the
/// container truncates, so runs never wrap. Spoilers render hidden
/// (open the chat to reveal); links render unstyled and don't open from
/// the list row.
pub(crate) fn chat_list_preview_line(
    icon: Option<&str>,
    preview: &str,
    entities: &[TextEntity],
    // Images for the preview's custom emoji (animated when decoded).
    emoji: &std::collections::HashMap<i64, ImageSource>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    chat_list_preview_line_layered(
        icon,
        preview,
        entities,
        emoji,
        &std::collections::HashMap::new(),
        cx,
    )
}

/// [`chat_list_preview_line`] whose decoded animated emoji (`layered`)
/// are painted by the chat list's animation layer (`anim_layer`).
pub(crate) fn chat_list_preview_line_layered(
    icon: Option<&str>,
    preview: &str,
    entities: &[TextEntity],
    emoji: &std::collections::HashMap<i64, ImageSource>,
    layered: &std::collections::HashMap<i64, super::anim_layer::LayeredClip>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    // One line, as in Telegram Desktop: line breaks read as spaces (same
    // byte length, so entity offsets still hold).
    let preview = preview.replace(['\n', '\r'], " ");
    let muted = cx.theme().muted_foreground;
    if entities.is_empty() {
        // A single text node, so an overlong preview ends in an ellipsis.
        let text = match icon {
            Some(glyph) => format!("{glyph} {preview}"),
            None => preview,
        };
        return div()
            .text_xs()
            .truncate()
            .text_color(muted)
            .child(text)
            .into_any_element();
    }
    let mut parts: Vec<(bool, Div)> = Vec::new();
    if let Some(glyph) = icon {
        parts.push((false, div().child(format!("{glyph} "))));
    }
    let key = line_key(&preview);
    for (index, run) in styled_runs(&preview, entities).into_iter().enumerate() {
        if run.text.is_empty() {
            continue;
        }
        if let Some(clip) = run.custom_emoji_id.and_then(|id| layered.get(&id)) {
            let frames = super::anim_layer::frames(clip.clone(), 30);
            parts.push((false, div().child(frames.size(px(14.)).flex_none())));
            continue;
        }
        if let Some(source) = run.custom_emoji_id.and_then(|id| emoji.get(&id)) {
            parts.push((
                false,
                div().child(
                    img(source.clone())
                        .id(SharedString::from(format!("preview-emoji-{key}-{index}")))
                        .size(px(14.))
                        .aspect_square()
                        .object_fit(ObjectFit::Contain),
                ),
            ));
            continue;
        }
        let style = &run.style;
        let mut el = div().whitespace_nowrap().child(run.text);
        if style.bold {
            el = el.font_weight(FontWeight::BOLD);
        }
        if style.italic {
            el = el.italic();
        }
        if style.underline {
            el = el.underline();
        }
        if style.strikethrough {
            el = el.line_through();
        }
        if style.code || style.pre {
            el = el.font_family(MONO_FONT);
        }
        if style.spoiler {
            el = el.bg(fill_muted()).text_color(fill_muted()).rounded_sm();
        }
        parts.push((true, el));
    }
    // The last text run shrinks and ends in the ellipsis; everything
    // before it keeps its width.
    let last_text = parts.iter().rposition(|(is_text, _)| *is_text);
    let mut line = div()
        .text_xs()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(muted)
        .flex()
        .flex_row()
        .items_center();
    for (position, (_, part)) in parts.into_iter().enumerate() {
        line = line.child(if Some(position) == last_text {
            part.min_w_0().flex_shrink(1.).truncate()
        } else {
            part.flex_none()
        });
    }
    line.into_any_element()
}

/// A stable per-preview key for element ids (GPUI animates an image
/// only when it has an id).
fn line_key(preview: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    preview.hash(&mut hasher);
    hasher.finish()
}

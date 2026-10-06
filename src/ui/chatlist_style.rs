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
    let mut line = div()
        .text_xs()
        .truncate()
        .text_color(cx.theme().muted_foreground)
        .flex()
        .flex_row()
        .items_center();
    if let Some(glyph) = icon {
        line = line.child(div().child(format!("{glyph} ")));
    }
    if entities.is_empty() {
        return line.child(preview.to_string()).into_any_element();
    }
    let key = line_key(preview);
    for (index, run) in styled_runs(preview, entities).into_iter().enumerate() {
        if run.text.is_empty() {
            continue;
        }
        if let Some(source) = run.custom_emoji_id.and_then(|id| emoji.get(&id)) {
            line = line.child(
                img(source.clone())
                    .id(SharedString::from(format!("preview-emoji-{key}-{index}")))
                    .size(px(14.))
                    .aspect_square()
                    .object_fit(ObjectFit::Contain)
                    .flex_none(),
            );
            continue;
        }
        let style = &run.style;
        let mut el = div().child(run.text);
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
        line = line.child(el);
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

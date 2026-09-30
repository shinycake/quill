//! Slice chatlist-list-style: the chat-list row preview line renderer.
//! Pure style inputs (`icon`, `entities`) come from
//! `quill::chatlist_style`; this module only paints them.

use super::*;

/// The chat-row preview line — optional media icon plus the preview
/// text, plain or formatted via `styled_runs`. Single line: the
/// container truncates, so runs never wrap. Spoilers render hidden
/// (open the chat to reveal); links render unstyled and don't open from
/// the list row.
pub(crate) fn chat_list_preview_line(
    icon: Option<&str>,
    preview: &str,
    entities: &[TextEntity],
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
    for run in styled_runs(preview, entities) {
        if run.text.is_empty() {
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

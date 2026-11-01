use super::*;
use quill::lang::{keys, tr_with};
use quill::text_split::{split_markup_text, units_over_limit};
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextLimitNote {
    Fits,
    UnitsOver(usize),
    Parts(usize),
}

impl TextLimitNote {
    fn of(markup: &str, limit: i32, editing: bool) -> Self {
        let markup = markup.trim();
        let over = units_over_limit(markup, limit);
        if over == 0 {
            Self::Fits
        } else if editing {
            Self::UnitsOver(over)
        } else {
            Self::Parts(split_markup_text(markup, limit).len())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TextLimitKey {
    field: EntityId,
    revision: u64,
    rich_editor: bool,
    limit: i32,
    editing: bool,
}

#[derive(Default)]
pub(in crate::ui) struct TextLimitMemo(Cell<Option<(TextLimitKey, TextLimitNote)>>);

impl TextLimitMemo {
    fn get(&self, key: TextLimitKey, count: impl FnOnce() -> TextLimitNote) -> TextLimitNote {
        match self.0.get() {
            Some((known, note)) if known == key => note,
            _ => {
                let note = count();
                self.0.set(Some((key, note)));
                note
            }
        }
    }
}

impl QuillApp {
    fn text_limit_note(&self, editing: bool, cx: &App) -> TextLimitNote {
        let key = TextLimitKey {
            field: self.composer.entity_id(),
            revision: self.composer.read(cx).document_revision(),
            rich_editor: self.composer_ui.rich_editor_open,
            limit: self.text_length_limit(),
            editing,
        };
        self.composer_ui.text_limit.get(key, || {
            TextLimitNote::of(&self.composer_markup(cx), key.limit, editing)
        })
    }

    /// R8: tdesktop `CharactersLimitLabel` — only shown once the text is
    /// over `message_text_length_max`, as a red "−N" (units to remove) while
    /// editing. A new message over the limit is not an error (it is sent as
    /// several messages), so the label says how many.
    pub(in crate::ui) fn text_limit_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.pending_attachments().is_empty() {
            return None;
        }
        let editing = self.composer_ui.pending_edit.as_ref();
        if editing.is_some_and(|edit| !matches!(edit.kind, quill::composer::ComposerEditKind::Text))
        {
            return None;
        }
        let (label, color) = match self.text_limit_note(editing.is_some(), cx) {
            TextLimitNote::Fits => return None,
            TextLimitNote::UnitsOver(over) => {
                (format!("\u{2212}{}", over.min(999)), danger_bright())
            }
            TextLimitNote::Parts(parts) => (
                tr_with(
                    keys::Q_CHAT_WILL_BE_SENT_AS_MESSAGES,
                    &[("parts", &parts.to_string())],
                ),
                text_muted(),
            ),
        };
        Some(
            div()
                .id("composer-text-limit")
                .flex()
                .justify_end()
                .px_3()
                .py_1()
                .child(div().text_xs().text_color(color).child(label))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "text_limit_tests.rs"]
mod tests;

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

/// The note of the text at the last counted revision. Counting takes time in
/// proportion to the text, so a text that may be over the limit is counted off
/// the UI thread, and the note on screen stays until the count of the latest
/// revision comes back.
#[derive(Default)]
struct TextLimitCount {
    known: Cell<Option<(TextLimitKey, TextLimitNote)>>,
    running: Cell<Option<(TextLimitKey, Task<()>)>>,
}

#[derive(Default)]
pub(in crate::ui) struct TextLimitMemo(Box<TextLimitCount>);

impl TextLimitMemo {
    fn known(&self) -> Option<(TextLimitKey, TextLimitNote)> {
        self.0.known.get()
    }

    fn set(&self, key: TextLimitKey, note: TextLimitNote) {
        self.0.known.set(Some((key, note)));
        self.0.running.take();
    }

    fn is_counting(&self, key: TextLimitKey) -> bool {
        let running = self.0.running.take();
        let same = running.as_ref().is_some_and(|(known, _)| *known == key);
        self.0.running.set(running);
        same
    }
}

impl QuillApp {
    fn text_limit_note(&self, editing: bool, cx: &mut Context<Self>) -> TextLimitNote {
        let composer = self.composer.read(cx);
        let key = TextLimitKey {
            field: self.composer.entity_id(),
            revision: composer.document_revision(),
            rich_editor: self.composer_ui.rich_editor_open,
            limit: self.text_length_limit(),
            editing,
        };
        let memo = &self.composer_ui.text_limit;
        let shown = memo.known();
        if let Some((known, note)) = shown
            && known == key
        {
            return note;
        }
        // A UTF-8 byte is at least a UTF-16 unit, and parsing only removes.
        let fits = usize::try_from(key.limit).is_ok_and(|limit| composer.text().len() <= limit);
        if fits {
            memo.set(key, TextLimitNote::Fits);
            return TextLimitNote::Fits;
        }
        if !memo.is_counting(key) {
            let markup = self.composer_markup(cx);
            let count = cx
                .background_spawn(async move { TextLimitNote::of(&markup, key.limit, editing) });
            let task = Self::spawn_main(cx, async move |this, cx| {
                let note = count.await;
                this.update(cx, |this, cx| {
                    this.composer_ui.text_limit.set(key, note);
                    cx.notify();
                })
                .ok();
            });
            memo.0.running.set(Some((key, task)));
        }
        shown.map_or(TextLimitNote::Fits, |(_, note)| note)
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

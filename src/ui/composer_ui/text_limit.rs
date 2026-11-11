use super::*;
use quill::lang::{keys, tr_with};
use quill::text_split::{split_markup_text, units_over_limit};
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextLimitNote {
    Fits,
    UnitsOver(usize),
    Parts(usize),
    /// Over the limit by bytes, and the count of this text is not back yet.
    Counting,
}

/// Bytes of text counted in the frame, a few milliseconds of work. Longer text
/// is counted off the UI thread.
const COUNTED_IN_THE_FRAME: usize = 16 * 1024;

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
    limit: i32,
    editing: bool,
}

/// The note of the text at the last counted revision, which only ever answers
/// for that revision of that field. One value: a count in flight keeps the
/// last answer beside it, and dropping the value cancels the count.
#[derive(Default)]
enum TextLimitCount {
    #[default]
    Idle,
    Known(TextLimitKey, TextLimitNote),
    Counting {
        key: TextLimitKey,
        _task: Task<()>,
        last: Option<(TextLimitKey, TextLimitNote)>,
    },
}

#[derive(Default)]
pub(in crate::ui) struct TextLimitMemo(Box<RefCell<TextLimitCount>>);

impl TextLimitMemo {
    fn known(&self) -> Option<(TextLimitKey, TextLimitNote)> {
        match &*self.0.borrow() {
            TextLimitCount::Idle => None,
            TextLimitCount::Known(key, note) => Some((*key, *note)),
            TextLimitCount::Counting { last, .. } => *last,
        }
    }

    /// An answer: any count in flight is over.
    fn set(&self, key: TextLimitKey, note: TextLimitNote) {
        *self.0.borrow_mut() = TextLimitCount::Known(key, note);
    }

    fn is_counting(&self, key: TextLimitKey) -> bool {
        matches!(&*self.0.borrow(), TextLimitCount::Counting { key: counting, .. } if *counting == key)
    }

    /// A count for `key` runs as `task`; an earlier count is dropped.
    fn start(&self, key: TextLimitKey, task: Task<()>) {
        let last = self.known();
        *self.0.borrow_mut() = TextLimitCount::Counting {
            key,
            _task: task,
            last,
        };
    }
}

impl QuillApp {
    fn text_limit_note(&self, editing: bool, cx: &mut Context<Self>) -> TextLimitNote {
        let composer = self.composer.read(cx);
        let key = TextLimitKey {
            field: self.composer.entity_id(),
            revision: composer.document_revision(),
            limit: self.text_length_limit(),
            editing,
        };
        let memo = &self.composer_ui.text_limit;
        if let Some((known, note)) = memo.known()
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
        if composer.text().len() <= COUNTED_IN_THE_FRAME {
            let note = TextLimitNote::of(&self.composer_markup(cx), key.limit, editing);
            memo.set(key, note);
            return note;
        }
        if !memo.is_counting(key) {
            let markup = self.composer_markup(cx);
            let count =
                cx.background_spawn(async move { TextLimitNote::of(&markup, key.limit, editing) });
            // The count lands in the memo of the window that asked.
            let task = self.spawn_scoped(cx, async move |this, cx| {
                let note = count.await;
                this.update(cx, |this, cx| {
                    this.composer_ui.text_limit.set(key, note);
                    cx.notify();
                })
                .ok();
            });
            memo.start(key, task);
        }
        TextLimitNote::Counting
    }

    /// R8: tdesktop `CharactersLimitLabel` — only shown once the text is
    /// over `message_text_length_max`, as a red "−N" (units to remove) while
    /// editing. A new message over the limit is not an error (it is sent as
    /// several messages), so the label says how many.
    pub(in crate::ui) fn text_limit_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // The rich editor sends one message, and refuses text over its own limit.
        if self.chat_ui.rich_editor_open || !self.pending_attachments().is_empty() {
            return None;
        }
        let editing = self.chat_ui.pending_edit();
        if editing.is_some_and(|edit| !matches!(edit.kind, quill::composer::ComposerEditKind::Text))
        {
            return None;
        }
        let parts_label =
            |parts: &str| tr_with(keys::Q_CHAT_WILL_BE_SENT_AS_MESSAGES, &[("parts", parts)]);
        let (label, color) = match self.text_limit_note(editing.is_some(), cx) {
            TextLimitNote::Fits => return None,
            TextLimitNote::Counting if editing.is_some() => {
                ("\u{2212}\u{2026}".to_string(), danger_bright())
            }
            TextLimitNote::Counting => (parts_label("\u{2026}"), text_muted()),
            TextLimitNote::UnitsOver(over) => {
                (format!("\u{2212}{}", over.min(999)), danger_bright())
            }
            TextLimitNote::Parts(parts) => (parts_label(&parts.to_string()), text_muted()),
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

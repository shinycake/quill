//! Rich message drafts in the composer. A slot whose stored draft is a
//! rich message shows it in place of the field (tdesktop
//! `RichDraftPreview`, `HistoryWidget::shouldShowRichDraftPreview`); a click
//! opens it in the rich editor, Send sends its blocks
//! (`HistoryWidget::sendRichDraft`) and a button discards it
//! (`initDiscardRichDraftButton`, `clearRichDraft`).
#![deny(clippy::indexing_slicing)]

mod dialogs;
mod paste;

#[cfg(feature = "demo-capture")]
mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "rich_draft_tests.rs"]
mod tests;

use super::app::QuillApp;
use crate::ui::message_text::rich_block_element;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::DraftContent;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::rich::RichDraft;
use quill::state::Session;

/// The composer's rich draft surfaces that are not the stored draft
/// itself: the discard and Premium boxes, the preview's scroll position
/// and the paste notice.
#[derive(Default)]
pub(in crate::ui) struct RichDraftUi {
    discard_confirm: bool,
    /// "Remove Formatting" is open; holds the draft flattened to message
    /// markup for "Send Without Formatting".
    premium_choice: Option<String>,
    preview_scroll: ScrollHandle,
    paste: paste::PasteNotice,
}

/// tdesktop `historyComposeField.heightMax` (the preview's height cap is
/// the field's).
const PREVIEW_MAX_HEIGHT: f32 = 220.;

impl QuillApp {
    /// The open slot's rich draft while it shows in place of the field:
    /// not while a message is edited or the rich editor is open.
    pub(in crate::ui) fn shown_rich_draft(&self) -> Option<&RichDraft> {
        if self.composer_ui.pending_edit.is_some() || self.composer_ui.rich_editor_open {
            return None;
        }
        let chat_id = self.open_chat_id()?;
        let session = self.session()?;
        let topic = session.open_draft_topic(chat_id);
        if !session.accepts_composer_draft(chat_id, topic) {
            return None;
        }
        session.draft_for(chat_id, topic)?.rich.as_ref()
    }

    /// The draft's blocks drawn where the field is, capped at the field's
    /// height with the bottom fading out while more is below.
    pub(in crate::ui) fn rich_draft_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let rich = self.shown_rich_draft()?;
        let blocks = rich.preview_blocks();
        let label = rich.summary();
        let revealed = std::collections::HashSet::new();
        let font = self.msg_font();
        let mut body = div()
            .id("rich-draft-preview-blocks")
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1()
            .max_h(px(PREVIEW_MAX_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.composer_ui.rich_draft.preview_scroll);
        for (index, block) in blocks.iter().enumerate() {
            if let Some(child) = rich_block_element(
                index,
                block,
                (0, 0),
                ChatId(0),
                MessageId(0),
                &revealed,
                font,
                cx,
            ) {
                body = body.child(child);
            }
        }
        let scroll = &self.composer_ui.rich_draft.preview_scroll;
        let below = scroll.max_offset().y + scroll.offset().y;
        let fade = (below > px(1.)).then(|| {
            let background = cx.theme().background;
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(font * 3.)
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(background.opacity(0.), 0.),
                    linear_color_stop(background, 1.),
                ))
        });
        Some(
            div()
                .id("rich-draft-preview")
                .test_support()
                .relative()
                .flex_1()
                .min_w_0()
                .cursor_pointer()
                .role(Role::Button)
                .aria_label(label)
                .on_click(cx.listener(|this, _, window, cx| this.open_rich_draft(window, cx)))
                .child(body)
                .children(fade)
                .into_any_element(),
        )
    }

    /// tdesktop's discard button over the attach button while the preview
    /// shows. A secondary-modifier click discards without asking.
    pub(in crate::ui) fn rich_draft_discard_button(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.shown_rich_draft()?;
        Some(
            Button::new("rich-draft-discard")
                .icon(IconName::Trash)
                .ghost()
                .small()
                .tooltip(tr(keys::RECORD_LOCK_DISCARD))
                .accessibility_label(tr(keys::RECORD_LOCK_DISCARD))
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    if event.modifiers().secondary() {
                        this.discard_rich_draft(cx);
                    } else {
                        this.composer_ui.rich_draft.discard_confirm = true;
                        cx.notify();
                    }
                }))
                .into_any_element(),
        )
    }

    /// Open the shown draft in the rich editor (tdesktop
    /// `Iv::Editor::ShowComposeBox`).
    pub(in crate::ui) fn open_rich_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rich) = self.shown_rich_draft().cloned() else {
            return;
        };
        // The editor's markup has no media: editing such a draft would
        // drop its photos and files on the next save.
        if rich.input().is_none() {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_RICH_DRAFT_UNEDITABLE)));
            cx.notify();
            return;
        }
        self.composer_ui.rich_editor_open = true;
        self.set_composer_markup(&rich.markup(), window, cx);
        self.composer_ui.prev_text = self.composer.read(cx).value().to_string();
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// The editor's close button: blocks with structure go back to being
    /// a rich draft shown in place of the field (tdesktop saves the
    /// editor's draft when its box closes); plain text stays in the field.
    pub(in crate::ui) fn close_rich_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let markup = self.composer_markup(cx);
        self.composer_ui.rich_editor_open = false;
        let Some(chat_id) = self.open_chat_id() else {
            self.set_composer_markup(&markup, window, cx);
            cx.notify();
            return;
        };
        if let Some(rich) = RichDraft::from_markup(&markup) {
            let topic = self.open_draft_topic(chat_id);
            let reply = self.draft_reply(self.composer_ui.reply(), chat_id);
            self.save_chat_draft(
                chat_id,
                topic,
                DraftContent::Rich(rich.clone()),
                reply,
                false,
                cx,
            );
            // Only once the draft holds the blocks may the field let go of
            // them; a draft that could not be saved stays as text.
            if self.stored_rich_draft() == Some(&rich) {
                self.set_composer_markup("", window, cx);
                self.composer_ui.prev_text.clear();
                cx.notify();
                return;
            }
        }
        self.set_composer_markup(&markup, window, cx);
        cx.notify();
    }

    /// tdesktop `clearRichDraft`: the draft goes, a reply stays as a
    /// draft of its own.
    pub(in crate::ui) fn discard_rich_draft(&mut self, cx: &mut Context<Self>) {
        let Some(chat_id) = self.open_chat_id() else {
            return;
        };
        let topic = self.open_draft_topic(chat_id);
        let reply = self.draft_reply(self.composer_ui.reply(), chat_id);
        self.save_chat_draft(
            chat_id,
            topic,
            DraftContent::Text(String::new()),
            reply,
            false,
            cx,
        );
        cx.notify();
    }

    /// Send pressed while the draft shows. Returns whether the draft took
    /// the press.
    pub(in crate::ui) fn submit_rich_draft(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(rich) = self.shown_rich_draft().cloned() else {
            return false;
        };
        if rich.input().is_none() {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_RICH_DRAFT_UNEDITABLE)));
            cx.notify();
            return true;
        }
        let premium = self.session().is_some_and(Session::my_is_premium);
        if premium {
            self.submit_rich_composer(rich.markup(), window, cx);
        } else if rich.uses_premium_formatting() {
            self.refuse_rich_send(&rich, window, cx);
        } else if let Some(plain) = rich.plain_markup() {
            self.send_rich_draft_plain(plain, window, cx);
        }
        true
    }

    /// A non-Premium send of rich blocks (tdesktop `sendRichDraft`): offer
    /// to send the text without its formatting, or explain the
    /// requirement when the text cannot stand alone.
    pub(in crate::ui) fn refuse_rich_send(
        &mut self,
        rich: &RichDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let limit = self
            .session()
            .map_or(0, |session| session.messages.message_text_length_max);
        let limit = usize::try_from(limit).unwrap_or(0);
        let plain = rich.plain_markup().filter(|plain| {
            let shown = quill::composer::parse_format_markup(plain).0;
            !shown.trim().is_empty() && shown.chars().count() <= limit
        });
        match plain {
            Some(plain) => {
                self.composer_ui.rich_draft.premium_choice = Some(plain);
                cx.notify();
            }
            None => self.show_rich_premium_toast(window, cx),
        }
    }

    /// tdesktop `sendRichDraftWithoutFormatting`: the flattened text goes
    /// through the field's send, which drops the draft once it is out.
    fn send_rich_draft_plain(
        &mut self,
        plain: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer_ui.rich_editor_open = false;
        self.set_composer_markup(&plain, window, cx);
        self.submit_composer(plain.clone(), window, cx);
        // A refused send leaves the draft: the field it hides goes back
        // to empty.
        if self.shown_rich_draft().is_some() && self.composer_markup(cx) == plain {
            self.set_composer_markup("", window, cx);
        }
    }
}

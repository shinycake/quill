//! Rich message drafts in the composer. A slot whose stored draft is a
//! rich message shows it in place of the field (tdesktop
//! `RichDraftPreview`, `HistoryWidget::shouldShowRichDraftPreview`); a click
//! opens it in the rich editor, Send sends its blocks
//! (`HistoryWidget::sendRichDraft`) and a button discards it
//! (`initDiscardRichDraftButton`, `clearRichDraft`).
#![deny(clippy::indexing_slicing)]

mod dialogs;
mod paste;
mod preview;

#[cfg(feature = "demo-capture")]
mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "rich_draft_tests.rs"]
mod tests;

use super::app::QuillApp;
use crate::ui::composer_sends::SendOnce;
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

/// What the composer shows for a rich draft, and the box open over it:
/// one value, so the editor and the box move together through its
/// transitions and go together when the window shows another chat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::ui) enum RichComposer {
    /// The field (or the stored draft's preview).
    #[default]
    Field,
    /// The rich editor is open: the field is interpreted as block markup
    /// (`quill::rich::markup_to_blocks`) and sends via
    /// `inputMessageRichMessage`.
    Editor,
    /// "Discard this message?" over the field's preview (the preview shows
    /// only while the editor is closed).
    Discard,
    /// "Remove Formatting" over the field, with the draft flattened to
    /// message markup for "Send Without Formatting".
    Choice(String),
    /// The same box over the open editor; closing it leaves the editor.
    EditorChoice(String),
}

impl RichComposer {
    pub(in crate::ui) fn editor_open(&self) -> bool {
        matches!(self, Self::Editor | Self::EditorChoice(_))
    }

    /// Whether the discard box is open.
    pub(in crate::ui) fn discarding(&self) -> bool {
        matches!(self, Self::Discard)
    }

    /// The flattened text the "Remove Formatting" box holds.
    pub(in crate::ui) fn choice(&self) -> Option<&str> {
        match self {
            Self::Choice(plain) | Self::EditorChoice(plain) => Some(plain),
            _ => None,
        }
    }

    /// Open (`true`) or close the editor. A "Remove Formatting" box stays
    /// over it; the discard box (its draft moves into the editor) goes.
    pub(in crate::ui) fn set_editor(&mut self, open: bool) {
        *self = match (std::mem::take(self), open) {
            (Self::Field | Self::Editor, true) | (Self::Discard, true) => Self::Editor,
            (Self::Field | Self::Editor, false) => Self::Field,
            (Self::Discard, false) => Self::Discard,
            (Self::Choice(plain) | Self::EditorChoice(plain), true) => Self::EditorChoice(plain),
            (Self::Choice(plain) | Self::EditorChoice(plain), false) => Self::Choice(plain),
        };
    }

    /// Ask "discard this message?". The preview it belongs to shows only
    /// while the editor is closed, so an open editor ignores it.
    pub(in crate::ui) fn ask_discard(&mut self) {
        if !self.editor_open() {
            *self = Self::Discard;
        }
    }

    /// Ask "Remove Formatting" for `plain`, over the editor when it is open.
    pub(in crate::ui) fn ask_choice(&mut self, plain: String) {
        *self = if self.editor_open() {
            Self::EditorChoice(plain)
        } else {
            Self::Choice(plain)
        };
    }

    /// Close the box; the editor stays as it was.
    pub(in crate::ui) fn dismiss(&mut self) {
        *self = if self.editor_open() {
            Self::Editor
        } else {
            Self::Field
        };
    }

    /// Take the "Remove Formatting" text with its box; any other state
    /// stays as it was.
    pub(in crate::ui) fn take_choice(&mut self) -> Option<String> {
        let plain = self.choice()?.to_string();
        self.dismiss();
        Some(plain)
    }
}

/// The composer's rich draft surfaces that are not the stored draft
/// itself and not its boxes (`ChatUi::rich`): the preview's
/// scroll position and the paste notice.
#[derive(Default)]
pub(in crate::ui) struct RichDraftUi {
    preview_scroll: ScrollHandle,
    paste: paste::PasteNotice,
    pub(in crate::ui) preview: preview::RichPreview,
    /// The editor's text as blocks, for the preview and the draft.
    pub(in crate::ui) parsed: preview::EditorBlocks,
}

/// tdesktop `historyComposeField.heightMax` (the preview's height cap is
/// the field's).
const PREVIEW_MAX_HEIGHT: f32 = 220.;

impl QuillApp {
    /// The open slot's rich draft while it shows in place of the field:
    /// not while a message is edited or the rich editor is open.
    pub(in crate::ui) fn shown_rich_draft(&self) -> Option<&RichDraft> {
        if self.chat_ui.is_editing() || self.chat_ui.rich.editor_open() {
            return None;
        }
        let chat_id = self.open_chat_id()?;
        let session = self.session()?;
        let topic = session.open_draft_topic(chat_id);
        if !session.accepts_composer_draft(chat_id, topic) {
            return None;
        }
        session.draft_for(chat_id, topic)?.rich()
    }

    /// The draft's blocks drawn where the field is, capped at the field's
    /// height with the bottom fading out while more is below.
    pub(in crate::ui) fn rich_draft_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let rich = self.shown_rich_draft()?;
        let blocks = rich.preview_blocks();
        let label = rich.summary();
        let revealed = crate::ui::message_text::revealed::Revealed::default();
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
                    if event.modifiers().secondary(cx) {
                        this.discard_rich_draft(cx);
                    } else {
                        this.chat_ui.rich.ask_discard();
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
            self.set_note(StatusNote::info(tr(keys::Q_RICH_DRAFT_UNEDITABLE)));
            cx.notify();
            return;
        }
        self.chat_ui.rich.set_editor(true);
        self.set_composer_markup(&rich.markup(), window, cx);
        let value = self.composer.read(cx).value();
        self.composer_ui.echo.set(value);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// The editor's close button: blocks with structure go back to being
    /// a rich draft shown in place of the field (tdesktop saves the
    /// editor's draft when its box closes); plain text stays in the field.
    pub(in crate::ui) fn close_rich_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let markup = self.composer_markup(cx);
        self.chat_ui.rich.set_editor(false);
        let Some(chat_id) = self.open_chat_id() else {
            self.set_composer_markup(&markup, window, cx);
            cx.notify();
            return;
        };
        if let Some(rich) = RichDraft::from_markup(&markup) {
            let topic = self.open_draft_topic(chat_id);
            let reply = self.draft_reply(self.chat_ui.reply(), chat_id);
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
                self.composer_ui.echo.set(SharedString::default());
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
        let reply = self.draft_reply(self.chat_ui.reply(), chat_id);
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
        once: SendOnce,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(rich) = self.shown_rich_draft().cloned() else {
            return false;
        };
        if rich.input().is_none() {
            self.set_note(StatusNote::info(tr(keys::Q_RICH_DRAFT_UNEDITABLE)));
            cx.notify();
            return true;
        }
        let premium = self.session().is_some_and(Session::my_is_premium);
        if premium {
            self.submit_rich_composer(rich.blocks(), once, window, cx);
        } else if rich.uses_premium_formatting() {
            self.refuse_rich_send(&rich, window, cx);
        } else if let Some(plain) = rich.plain_markup() {
            self.send_rich_draft_plain(plain, once, window, cx);
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
                self.chat_ui.rich.ask_choice(plain);
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
        once: SendOnce,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.chat_ui.rich.set_editor(false);
        self.set_composer_markup(&plain, window, cx);
        self.submit_composer(plain.clone(), once, window, cx);
        // A refused send leaves the draft: the field it hides goes back
        // to empty.
        if self.shown_rich_draft().is_some() && self.composer_markup(cx) == plain {
            self.set_composer_markup("", window, cx);
        }
    }
}

//! The "Apply AI style to text" shortcut (tdesktop api_compose_with_ai.cpp
//! `TriggerAiApplyInPlace`): the composer's selection, or all of it, is
//! rewritten in the style bound to the shortcut.

use super::session_mut;
use super::style_notices::text_toast;
use crate::ui::ai_compose_state::ApplyInPlace;
use crate::ui::app::QuillApp;
use crate::ui::composer_field::{doc_content, field_doc};
use gpui_kit::*;
use quill::ai_compose::{AiBoxError, fresh_token, prefs};
use quill::composer_doc::ComposerDoc;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The shortcut. `false` (the key goes on) where the composer cannot
    /// send text.
    pub(in crate::ui) fn ai_compose_apply_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) || self.composer_ui.rich_editor_open {
            return false;
        }
        let Some(chat_id) = self.writable_open_chat() else {
            return false;
        };
        let Some(style_name) = prefs::current().bound_style_name else {
            self.push_ai_toast(text_toast(tr(keys::AI_COMPOSE_APPLY_UNBOUND)), window, cx);
            return true;
        };
        let (doc, selected) = {
            let state = self.composer.read(cx);
            (field_doc(state), state.selected_range())
        };
        let selection = (!selected.is_empty()).then(|| selected.clone());
        let part = match &selection {
            Some(range) => doc.slice(range.clone()),
            None => doc,
        };
        if part.text.trim().is_empty() {
            self.push_ai_toast(text_toast(tr(keys::AI_COMPOSE_APPLY_EMPTY)), window, cx);
            return true;
        }
        let Some(live) = self.live.as_mut() else {
            return true;
        };
        let token = fresh_token();
        if live
            .driver
            .ai_apply_in_place(chat_id, &part.to_markup(), &style_name, token)
            .is_err()
        {
            self.ai_compose_error_toast(&AiBoxError::Other(String::new()), window, cx);
            return true;
        }
        self.ai_compose.apply_in_place = Some(ApplyInPlace {
            token,
            chat_id,
            selection,
            cursor: selected.end,
        });
        true
    }

    /// The answer: it replaces the range it was asked for (clamped to the
    /// field as it is now), the caret after it, or for the whole field at
    /// its old offset. A style the server no longer has unbinds the
    /// shortcut.
    pub(super) fn ai_apply_in_place_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(answer) =
            session_mut(self).and_then(|s| s.messages.ai_compose.apply_in_place.take())
        else {
            return;
        };
        let Some(asked) = self
            .ai_compose
            .apply_in_place
            .take_if(|asked| asked.token == answer.token)
        else {
            return;
        };
        let (text, entities) = match answer.outcome {
            Ok(result) => result,
            Err(AiBoxError::StyleInvalid(_)) => {
                prefs::update(|prefs| prefs.bound_style_name = None);
                self.push_ai_toast(text_toast(tr(keys::AI_COMPOSE_TONE_INVALID)), window, cx);
                return;
            }
            Err(error) => {
                self.ai_compose_error_toast(&error, window, cx);
                return;
            }
        };
        if self.session().and_then(|s| s.open_chat) != Some(asked.chat_id) {
            return;
        }
        let replacement = ComposerDoc::from_entities(&text, &entities);
        let field = self.composer.read(cx).value();
        let range = match &asked.selection {
            Some(range) => {
                let start = floor_boundary(&field, range.start);
                start..floor_boundary(&field, range.end).max(start)
            }
            None => 0..field.len(),
        };
        let cursor = match asked.selection {
            Some(_) => range.start + replacement.text.len(),
            None => asked.cursor.min(replacement.text.len()),
        };
        self.replace_composer_content(range, doc_content(&replacement), window, cx);
        self.composer.update(cx, |input, cx| {
            let cursor = floor_boundary(&input.value(), cursor);
            input.set_selected_range(cursor..cursor, cx);
        });
        cx.notify();
    }
}

/// `at` moved back to a char boundary of `text`, at most its length.
fn floor_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0)
}

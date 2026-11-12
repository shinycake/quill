//! The "Apply AI style to text" shortcut (tdesktop api_compose_with_ai.cpp
//! `TriggerAiApplyInPlace`): the composer's selection, or all of it, is
//! rewritten in the style bound to the shortcut.

use super::session_mut;
use super::style_notices::text_toast;
use crate::ui::ai_compose_state::ApplyInPlace;
use crate::ui::app::QuillApp;
use crate::ui::composer_field::{doc_content, field_doc};
use gpui_kit::*;
use quill::ai_compose::{AiBoxError, prefs};
use quill::composer_doc::ComposerDoc;
use quill::lang::{keys, tr};

impl QuillApp {
    /// A rewrite asked by the shortcut is still out: its request is in the
    /// session's table, or it answered and the answer waits in
    /// `apply_answers` for the asking window's upkeep to land it.
    pub(in crate::ui) fn ai_apply_in_flight(&self) -> bool {
        self.ai_compose
            .apply_in_place
            .as_ref()
            .zip(self.session())
            .is_some_and(|(asked, session)| {
                session.requests.get(asked.request).is_some()
                    || session.messages.ai_compose.apply_answers.has(asked.request)
            })
    }

    pub(in crate::ui) fn ai_compose_apply_by_key(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.shortcut_blocked(window, cx) || self.chat_ui.rich.editor_open() {
            return false;
        }
        let Some(chat_id) = self.writable_open_chat() else {
            return false;
        };
        let Some(conversation) = self.open_conversation() else {
            return false;
        };
        let Some(account) = self.current_account() else {
            return false;
        };
        // One rewrite at a time: another press while the request is out
        // sends nothing (a second ask would spend the quota again and drop
        // the first one's answer). A request the session no longer holds
        // is lost, and the press asks afresh.
        if self.ai_apply_in_flight() {
            return true;
        }
        let Some(style_name) = prefs::current().bound_style(&account).map(str::to_string) else {
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
        let Ok(request) = live
            .driver
            .ai_apply_in_place(chat_id, &part.to_markup(), &style_name)
        else {
            self.ai_compose_error_toast(&AiBoxError::Other(String::new()), window, cx);
            return true;
        };
        self.ai_compose.apply_in_place = Some(ApplyInPlace {
            request,
            account,
            field: self.composer.read(cx).value().to_string(),
            conversation,
            window: self.windows.scope,
            selection,
            cursor: selected.end,
        });
        true
    }

    /// The shortcut's answer, in the frame of the window whose composer
    /// asked (its toasts show there, and it lands while the main window
    /// is minimized). An answer nothing waits for, or whose window closed,
    /// is dropped in any window's frame.
    pub(super) fn ai_apply_in_place_upkeep(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = self.windows.scope;
        let elsewhere = self
            .ai_compose
            .apply_in_place
            .as_ref()
            .is_some_and(|asked| {
                asked.window != here && asked.window.is_none_or(|chat| self.windows.has(chat))
            });
        if elsewhere {
            return;
        }
        // The answer of the request the shortcut waits for; with nothing
        // waiting the answers are dropped.
        let Some(waiting) = self
            .ai_compose
            .apply_in_place
            .as_ref()
            .map(|asked| asked.request)
        else {
            if let Some(session) = session_mut(self) {
                session.messages.ai_compose.apply_answers.clear();
            }
            return;
        };
        let Some(outcome) =
            session_mut(self).and_then(|s| s.messages.ai_compose.apply_answers.take(waiting))
        else {
            return;
        };
        let Some(asked) = self.ai_compose.apply_in_place.take() else {
            return;
        };
        if asked.window != here {
            // Its window closed.
            return;
        }
        let (text, entities) = match outcome {
            Ok(result) => result,
            Err(AiBoxError::StyleInvalid(_)) => {
                prefs::update(|prefs| prefs.bind_style(&asked.account, None));
                self.push_ai_toast(text_toast(tr(keys::AI_COMPOSE_TONE_INVALID)), window, cx);
                return;
            }
            Err(error) => {
                self.ai_compose_error_toast(&error, window, cx);
                return;
            }
        };
        // The composer that asked is the one in place.
        self.apply_ai_in_place_answer(&asked, &text, &entities, window, cx);
    }

    /// Rewrite the composer in place with the answer, while it still
    /// shows the conversation that asked (the same chat in another forum
    /// topic, thread or Saved sublist is another draft).
    fn apply_ai_in_place_answer(
        &mut self,
        asked: &ApplyInPlace,
        text: &str,
        entities: &[quill::text::TextEntity],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_conversation().as_ref() != Some(&asked.conversation) {
            return;
        }
        let field = self.composer.read(cx).value();
        if field.as_ref() != asked.field.as_str() {
            // Typed on while the answer was coming: the recorded range
            // covers other text now.
            self.ai_compose_error_toast(&AiBoxError::Other(String::new()), window, cx);
            return;
        }
        let replacement = ComposerDoc::from_entities(text, entities);
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

fn floor_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0)
}

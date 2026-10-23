//! The Edit box's channel switches and its history row (tdesktop
//! `fillAutoTranslateButton`, `fillSignaturesButton`,
//! `fillHistoryVisibilityButton`). The box keeps each choice and Save
//! sends what differs from TDLib's value (`saveAutotranslate`,
//! `saveSignatures`, `saveHistoryVisibility`).
#![deny(clippy::indexing_slicing)]

use super::manage::{divider_text, management_toggle_row, value_row};
use crate::ui::app::QuillApp;
use crate::ui::earn_state::{AskBoost, AskBoostReason};
use gpui_kit::assets::IconName as I;
use gpui_kit::*;
use quill::connect::ConnectSendError;
use quill::group_manage::{AutoTranslate, AutoTranslateClick, Signatures, history_value};
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr};
use quill::state::{ManageRows, Session};

/// The switches whose choice differs from TDLib's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ToggleEdits {
    pub(crate) auto_translate: Option<bool>,
    pub(crate) signatures: Option<Signatures>,
    pub(crate) history_visible: Option<bool>,
}

impl ToggleEdits {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl super::GroupEditBox {
    /// What Save sends for the switches.
    pub(crate) fn toggle_edits(&self, session: &Session) -> ToggleEdits {
        let rows = session.manage_rows(self.chat_id);
        let history = session.group_admin_controls(self.chat_id).history;
        ToggleEdits {
            auto_translate: self
                .auto_translate
                .filter(|on| rows.auto_translate.is_some_and(|now| now.on != *on)),
            signatures: self
                .signatures
                .filter(|wanted| rows.signatures.is_some_and(|now| now != *wanted)),
            history_visible: self
                .history_visible
                .filter(|visible| history.is_some_and(|now| now.visible != *visible)),
        }
    }
}

impl QuillApp {
    /// The auto-translate switch as the box shows it.
    fn edit_box_auto_translate(&self, rows: &ManageRows) -> Option<AutoTranslate> {
        let mut state = rows.auto_translate?;
        if let Some(on) = self.group_edit_box().and_then(|edit| edit.auto_translate) {
            state.on = on;
        }
        Some(state)
    }

    fn edit_box_signatures(&self, rows: &ManageRows) -> Option<Signatures> {
        let saved = rows.signatures?;
        Some(
            self.group_edit_box()
                .and_then(|edit| edit.signatures)
                .unwrap_or(saved),
        )
    }

    /// "Auto-translate messages", locked below the boost level it needs.
    pub(super) fn auto_translate_row(
        &self,
        chat_id: ChatId,
        rows: &ManageRows,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.edit_box_auto_translate(rows)?;
        Some(management_toggle_row(
            "tr-toggle-auto-translate",
            I::Languages,
            tr(keys::EDIT_AUTOTRANSLATE),
            state.on,
            state.locked(),
            cx.listener(move |this, _, _, cx| this.click_auto_translate(chat_id, cx)),
            cx,
        ))
    }

    /// tdesktop `CheckBoostLevel`: a locked switch opens the boost box and
    /// refreshes the level.
    pub(crate) fn click_auto_translate(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(rows) = self.session().map(|s| s.manage_rows(chat_id)) else {
            return;
        };
        let Some(state) = self.edit_box_auto_translate(&rows) else {
            return;
        };
        match state.click() {
            AutoTranslateClick::Toggle(on) => {
                if let Some(edit) = self.group_edit_box_mut() {
                    edit.auto_translate = Some(on);
                }
            }
            AutoTranslateClick::AskBoost(level) => {
                self.earn.ask_boost = Some(AskBoost {
                    chat_id: chat_id.0,
                    level,
                    reason: AskBoostReason::Autotranslate,
                });
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.fetch_chat_boost_link(chat_id);
                }
                self.refresh_boost_status(chat_id, cx);
            }
            AutoTranslateClick::Check => {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.fetch_chat_boost_features(chat_id);
                }
                self.refresh_boost_status(chat_id, cx);
            }
        }
        cx.notify();
    }

    /// "Sign messages", then "Show authors' profiles" while signing, and
    /// the text under them.
    pub(super) fn signature_rows(
        &self,
        rows: &ManageRows,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.edit_box_signatures(rows)?;
        let set = |next: Signatures| {
            move |this: &mut QuillApp,
                  _: &ClickEvent,
                  _: &mut Window,
                  cx: &mut Context<QuillApp>| {
                if let Some(edit) = this.group_edit_box_mut() {
                    edit.signatures = Some(next);
                }
                cx.notify();
            }
        };
        let mut column = div()
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .child(management_toggle_row(
                "g2-toggle-sign-messages",
                I::PenLine,
                tr(keys::EDIT_SIGN_MESSAGES),
                state.sign,
                false,
                cx.listener(set(state.toggle_sign())),
                cx,
            ));
        if state.shows_profiles() {
            column = column.child(management_toggle_row(
                "g2-toggle-show-authors",
                I::CircleUser,
                tr(keys::EDIT_SIGN_PROFILES),
                state.profiles,
                false,
                cx.listener(set(state.toggle_profiles())),
                cx,
            ));
        }
        Some(
            column
                .child(divider_text(tr(state.about()), cx))
                .into_any_element(),
        )
    }

    /// "Chat history for new members" with "Visible" or "Hidden"; hidden
    /// for public groups, discussion groups and forums.
    pub(super) fn group_history_row(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let control = self.session()?.group_admin_controls(chat_id).history?;
        let edit = self.group_edit_box()?;
        if edit
            .type_choice
            .as_ref()
            .is_some_and(|choice| choice.public)
        {
            return None;
        }
        let visible = edit.history_visible.unwrap_or(control.visible);
        Some(value_row(
            "b7-history",
            I::MessageCircle,
            tr(keys::MANAGE_HISTORY_VISIBILITY_TITLE),
            Some(tr(history_value(visible)).to_string()),
            cx.listener(move |this, _, _, cx| {
                if let Some(edit) = this.group_edit_box_mut() {
                    edit.history_box = Some(visible);
                }
                cx.notify();
            }),
            cx,
        ))
    }

    /// Send the switches Save changes; the requests the box waits for.
    pub(super) fn send_toggle_edits(
        &mut self,
        chat_id: ChatId,
        edits: ToggleEdits,
    ) -> Result<Vec<RequestId>, ConnectSendError> {
        let Some(live) = self.live.as_mut() else {
            return Ok(Vec::new());
        };
        let driver = &mut live.driver;
        let mut sent = Vec::new();
        if let Some(visible) = edits.history_visible {
            sent.extend(driver.set_group_history_visible(chat_id, visible)?);
        }
        if let Some(on) = edits.auto_translate {
            sent.extend(driver.toggle_auto_translate(chat_id, on)?);
        }
        if let Some(signatures) = edits.signatures {
            sent.extend(driver.toggle_sign_messages(
                chat_id,
                signatures.sign,
                signatures.profiles,
            )?);
        }
        Ok(sent)
    }
}

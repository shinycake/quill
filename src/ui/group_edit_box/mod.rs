//! The group and channel Edit box (tdesktop `EditPeerInfoBox`): the photo
//! with its sources menu, the name and the description, then the manage
//! rows, and Save. Save sends the title and the description when they
//! differ from what TDLib last reported, then the chosen photo
//! (`quill::group_info_edit`); the box stays open, saving, until TDLib
//! answers the text edits, and shows an error under the field that failed.
//! The box state is derived: a box whose save went through is gone
//! (`QuillApp::group_edit_box`).
#![deny(clippy::indexing_slicing)]

mod manage;
mod photo;
mod title_emoji;

use super::app::QuillApp;
use super::profile_photo_sources::{ChosenPhoto, PhotoSources};
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::form::{field, v_form};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::group_info_edit::{
    DESCRIPTION_MAX, GroupInfoFields, GroupInfoInvalid, TITLE_MAX, plan_group_info_save,
};
use quill::group_type::{GroupTypeData, plan_group_type_save};
use quill::ids::{ChatId, RequestId};
use quill::lang::{keys, tr, tr_with};
use quill::state::{GroupInfoField, Session};
use quill::telegram::envelope::ChatKind;

/// The description shows three lines and grows to eight.
const DESCRIPTION_ROWS: (usize, usize) = (3, 8);

pub(crate) struct GroupEditBox {
    pub(crate) chat_id: ChatId,
    pub(crate) title_input: Entity<InputState>,
    pub(crate) description_input: Entity<TextareaState>,
    /// The description when the box opened; `None` when TDLib had not
    /// reported it yet, so an untouched empty field never clears it.
    initial_description: Option<String>,
    /// The photo a source made, sent on Save.
    pub(crate) photo: Option<ChosenPhoto>,
    /// Why the last picked photo can't be used.
    pub(crate) photo_note: Option<String>,
    /// The camera and emoji panels.
    pub(crate) sources: PhotoSources,
    /// The name field's emoji toggle opened the emoji panel.
    pub(crate) title_emoji: bool,
    /// The text requests of the last Save; `None` until Save sent any.
    saving: Option<Vec<RequestId>>,
    /// Save found the name empty (tdesktop `validateTitle`).
    title_empty: bool,
    /// The Type box, open over this one.
    pub(crate) type_box: Option<crate::ui::group_type_box::GroupTypeBox>,
    /// What the Type box's Save chose; sent with this box's Save.
    pub(crate) type_choice: Option<GroupTypeData>,
}

/// Where a box is, derived from its requests and TDLib's answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EditBoxState {
    Editing,
    Saving,
    /// TDLib refused the title or the description.
    Failed {
        field: GroupInfoField,
        reason: String,
    },
    /// Everything answered without an error: the box is closed.
    Saved,
}

impl GroupEditBox {
    fn state(&self, session: Option<&Session>) -> EditBoxState {
        let Some(sent) = &self.saving else {
            return EditBoxState::Editing;
        };
        let Some(session) = session else {
            return EditBoxState::Saved;
        };
        if sent.iter().any(|id| session.requests.get(*id).is_some()) {
            return EditBoxState::Saving;
        }
        match session.groups.info_edit_errors.get(&self.chat_id.0) {
            Some(error) if error.field != GroupInfoField::Photo => EditBoxState::Failed {
                field: error.field,
                reason: error.reason.clone(),
            },
            _ => EditBoxState::Saved,
        }
    }

    /// Why TDLib refused the link of the last Save.
    pub(crate) fn username_failure(&self, session: Option<&Session>) -> Option<String> {
        match self.state(session) {
            EditBoxState::Failed {
                field: GroupInfoField::Username,
                reason,
            } => Some(reason),
            _ => None,
        }
    }

    fn fields(&self, cx: &App) -> GroupInfoFields {
        GroupInfoFields::new(
            &self.title_input.read(cx).value(),
            &self.description_input.read(cx).value(),
        )
    }
}

impl QuillApp {
    /// The open Edit box; one whose save went through counts as closed.
    pub(crate) fn group_edit_box(&self) -> Option<&GroupEditBox> {
        let edit = self.admin.group_edit_box.as_ref()?;
        (edit.state(self.session()) != EditBoxState::Saved).then_some(edit)
    }

    pub(crate) fn group_edit_box_mut(&mut self) -> Option<&mut GroupEditBox> {
        let saved = self
            .admin
            .group_edit_box
            .as_ref()
            .is_some_and(|edit| edit.state(self.session()) == EditBoxState::Saved);
        if saved {
            return None;
        }
        self.admin.group_edit_box.as_mut()
    }

    /// "Manage group" / "Manage channel": open the Edit box filled from
    /// the chat, and fetch the full info that holds the description.
    pub(in crate::ui) fn open_group_edit_box(
        &mut self,
        chat_id: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(chat) = session.chats.get(&chat_id.0) else {
            return;
        };
        let title = chat.title.clone();
        let kind = chat.kind.clone();
        let is_channel = chat.is_channel();
        let description = session.chat_description(chat_id).map(str::to_string);
        if let Some(live) = self.live.as_mut() {
            let _ = match kind {
                ChatKind::Supergroup { supergroup_id, .. } => {
                    live.driver.fetch_supergroup_full_info(supergroup_id)
                }
                _ => live.driver.fetch_basic_group_members(chat_id),
            };
        }
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(if is_channel {
                    tr(keys::DLG_NEW_CHANNEL_NAME)
                } else {
                    tr(keys::DLG_NEW_GROUP_NAME)
                })
                .validate(|text, _| text.chars().count() <= TITLE_MAX)
                .default_value(title)
        });
        let description_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder(tr(keys::CREATE_GROUP_DESCRIPTION))
                .auto_grow(DESCRIPTION_ROWS.0, DESCRIPTION_ROWS.1)
                .submit_on_enter(false);
            state.set_value(description.clone().unwrap_or_default(), window, cx);
            state
        });
        // The counter and the name's error follow every keystroke.
        cx.subscribe(&title_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(edit) = this.admin.group_edit_box.as_mut() {
                    edit.title_empty = false;
                }
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&description_input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        self.admin.group_edit_box = Some(GroupEditBox {
            chat_id,
            title_input,
            description_input,
            initial_description: description,
            photo: None,
            photo_note: None,
            sources: PhotoSources::default(),
            saving: None,
            title_empty: false,
            title_emoji: false,
            type_box: None,
            type_choice: None,
        });
        cx.notify();
    }

    pub(in crate::ui) fn close_group_edit_box(&mut self, cx: &mut Context<Self>) {
        self.close_group_title_emoji(cx);
        self.admin.group_edit_box = None;
        cx.notify();
    }

    /// Save (tdesktop `Controller::save`): validate the name, then send
    /// what changed. Nothing changed closes the box.
    pub(in crate::ui) fn save_group_edit_box(&mut self, cx: &mut Context<Self>) {
        self.close_group_title_emoji(cx);
        let Some(edit) = self.group_edit_box() else {
            return;
        };
        if edit.state(self.session()) == EditBoxState::Saving {
            return;
        }
        let chat_id = edit.chat_id;
        let now = edit.fields(cx);
        let session = self.session();
        let title = session
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_default();
        let live_description = session.and_then(|s| s.chat_description(chat_id));
        // An empty field over a description the box never showed is no
        // edit.
        let description = match (&edit.initial_description, live_description) {
            (None, _) if now.description.is_empty() => String::new(),
            (_, Some(live)) => live.to_string(),
            (Some(initial), None) => initial.clone(),
            (None, None) => String::new(),
        };
        let current = GroupInfoFields::new(&title, &description);
        let photo = edit.photo.as_ref().map(|photo| &photo.input);
        let edits = match plan_group_info_save(&current, &now, photo) {
            Ok(edits) => edits,
            Err(GroupInfoInvalid::EmptyTitle | GroupInfoInvalid::TitleTooLong) => {
                if let Some(edit) = self.admin.group_edit_box.as_mut() {
                    edit.title_empty = true;
                }
                cx.notify();
                return;
            }
            // The field's counter is already red; Save waits.
            Err(GroupInfoInvalid::DescriptionTooLong) => return,
        };
        let basic = matches!(
            session
                .and_then(|s| s.chats.get(&chat_id.0))
                .map(|chat| &chat.kind),
            Some(ChatKind::BasicGroup { .. })
        );
        let type_edits = match (&edit.type_choice, session) {
            (Some(wanted), Some(session)) => {
                plan_group_type_save(&session.group_type_current(chat_id), wanted, basic)
            }
            _ => Vec::new(),
        };
        if edits.is_empty() && type_edits.is_empty() {
            self.admin.group_edit_box = None;
            cx.notify();
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            self.admin.group_edit_box = None;
            cx.notify();
            return;
        };
        let sent = live.driver.save_group_type(chat_id, &type_edits, &edits);
        let Some(edit) = self.admin.group_edit_box.as_mut() else {
            return;
        };
        match sent {
            Ok(sent) => {
                // The photo uploads after the box closes, as in tdesktop.
                edit.photo = None;
                if sent.is_empty() {
                    self.admin.group_edit_box = None;
                } else {
                    edit.saving = Some(sent);
                }
            }
            Err(err) => {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_SAVE_GROUP_INFO,
                    &err,
                )));
            }
        }
        cx.notify();
    }

    /// The box's name, description and photo, or the open source panel.
    fn group_edit_box_body(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let emoji_toggle = self.group_title_emoji_toggle(window, cx);
        let edit = self.group_edit_box()?;
        if let Some(panel) = self.photo_sources_panel(cx) {
            return Some(panel);
        }
        let session = self.session();
        let chat = session.and_then(|s| s.chats.get(&edit.chat_id.0))?;
        let is_channel = chat.is_channel();
        let state = edit.state(session);
        let theme = cx.theme();
        let danger = theme.danger;
        let muted = theme.muted_foreground;
        let error_line = |text: String| {
            div()
                .text_xs()
                .text_color(danger)
                .child(text)
                .into_any_element()
        };
        let failed = |wanted: GroupInfoField| match &state {
            EditBoxState::Failed { field, reason } if *field == wanted => Some(error_line(
                tr_with(keys::Q_PEOPLE_COULDNT_SAVE_REASON, &[("reason", reason)]),
            )),
            _ => None,
        };
        let title_error = edit
            .title_empty
            .then(|| error_line(tr(keys::Q_PEOPLE_TITLE_MUST_BE_1_128).to_string()))
            .or_else(|| failed(GroupInfoField::Title));
        let used = edit.description_input.read(cx).value().chars().count();
        let left = DESCRIPTION_MAX as i64 - used as i64;
        let counter = div()
            .id("group-edit-description-counter")
            .flex()
            .justify_end()
            .text_xs()
            .text_color(if left < 0 { danger } else { muted })
            .child(left.to_string());
        let name_label = if is_channel {
            tr(keys::DLG_NEW_CHANNEL_NAME)
        } else {
            tr(keys::DLG_NEW_GROUP_NAME)
        };
        let form = v_form()
            .child(
                field().label(name_label).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Input::new(&edit.title_input)
                                .aria_label(name_label)
                                .suffix(emoji_toggle),
                        )
                        .children(title_error),
                ),
            )
            .child(
                field().label(tr(keys::CREATE_GROUP_DESCRIPTION)).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Textarea::new(&edit.description_input)
                                .aria_label(tr(keys::CREATE_GROUP_DESCRIPTION)),
                        )
                        .child(counter)
                        .children(failed(GroupInfoField::Description)),
                ),
            );
        Some(
            div()
                .id("group-edit-box")
                .flex()
                .flex_col()
                .gap_4()
                .child(self.group_edit_photo(edit, cx))
                .child(form)
                .child(self.group_edit_box_manage_rows(edit.chat_id, cx))
                .into_any_element(),
        )
    }

    pub(super) fn build_group_edit_box_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::GroupEditBox, |this, _, cx| {
                this.close_group_edit_box(cx);
            });
        let save_app = app.clone();
        let on_ok = move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            save_app.update(cx, |this, cx| {
                this.save_group_edit_box(cx);
                this.close_kit_dialog_if_done(DialogKind::GroupEditBox, window, cx);
            });
            false
        };
        app.update(cx, |this, cx| {
            this.photo_sources_prepare_render(cx);
            let is_channel = this
                .group_edit_box()
                .and_then(|edit| this.session()?.chats.get(&edit.chat_id.0))
                .is_some_and(|chat| chat.is_channel());
            let dialog =
                dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(if is_channel {
                        tr(keys::EDIT_CHANNEL_TITLE)
                    } else {
                        tr(keys::EDIT_GROUP)
                    }));
            if this.group_edit_box().is_none() {
                return dialog.on_close(on_close);
            }
            let saving = this
                .group_edit_box()
                .is_some_and(|edit| edit.state(this.session()) == EditBoxState::Saving);
            let panel_open = this
                .group_edit_box()
                .is_some_and(|edit| edit.sources.is_open());
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("group-edit-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_group_edit_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupEditBox, window, cx);
                        })),
                )
                .child(
                    Button::new("group-edit-save")
                        .label(tr(keys::SETTINGS_SAVE))
                        .primary()
                        .loading(saving)
                        .disabled(saving || panel_open)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_group_edit_box(cx);
                            this.close_kit_dialog_if_done(DialogKind::GroupEditBox, window, cx);
                        })),
                );
            let body_app = cx.entity();
            dialog
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, window, cx| {
                        let body = body_app
                            .update(cx, |this, cx| this.group_edit_box_body(window, cx))
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_ok(on_ok)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    // Above every Manage dialog it opens (members 4300 … welcome 5800),
    // which show over it and hand it back when they close.
    GroupEditBox => DialogSpec::new(
        7700,
        |app| app.group_edit_box().is_some(),
        QuillApp::build_group_edit_box_dialog,
    ),
}

mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod ui_tests;

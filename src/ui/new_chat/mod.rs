//! New Group and New Channel (tdesktop `GroupInfoBox`, `SetupChannelBox`,
//! `AddParticipantsBoxController::Start`), as one dialog whose body
//! follows the steps of `quill::new_chat`: photo and name (and the
//! description), then a group's members, or a channel's type and its
//! subscribers. What the dialog shows is derived from the stage the person
//! reached and the session's answers to its requests
//! (`QuillApp::new_chat_view`).
#![deny(clippy::indexing_slicing)]

mod steps;

use super::app::QuillApp;
use super::group_type_box::GroupTypeBox;
use super::profile_photo_sources::{ChosenPhoto, PhotoSources};
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::select::SelectState;
use gpui_kit::*;
use quill::auto_delete::TTL_CHOICES;
use quill::group_info_edit::TITLE_MAX;
use quill::group_type::{GroupTypeData, USERNAME_MAX, plan_group_type_save};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::new_chat::{
    NewChatBack, NewChatInvalid, NewChatKind, NewChatStage, NewChatStep, NewChatView,
    plan_new_chat, view,
};

/// The description shows three lines and grows to eight.
const DESCRIPTION_ROWS: (usize, usize) = (3, 8);

pub(crate) struct NewChatDialog {
    pub(crate) kind: NewChatKind,
    pub(crate) stage: NewChatStage,
    pub(crate) title_input: Entity<InputState>,
    pub(crate) description_input: Entity<TextareaState>,
    pub(crate) search_input: Entity<TextareaState>,
    /// A group's members, or the new chat's subscribers.
    pub(crate) selected_users: Vec<i64>,
    /// The photo a source made, set once the chat exists.
    pub(crate) photo: Option<ChosenPhoto>,
    /// Why the last picked photo can't be used.
    pub(crate) photo_note: Option<String>,
    /// The camera and emoji panels.
    pub(crate) sources: PhotoSources,
    /// The group's auto-delete period; `None` keeps the account default.
    pub(crate) auto_delete: Option<i32>,
    /// The auto-delete step's picker over [`TTL_CHOICES`].
    auto_delete_picker: Entity<SelectState<Vec<SharedString>>>,
    /// Confirm found the name empty or too long.
    title_error: bool,
    /// The type step of a supergroup or channel (tdesktop
    /// `SetupChannelBox`, which starts on Public).
    type_box: Option<GroupTypeBox>,
}

impl QuillApp {
    /// What the open New Group / New Channel dialog shows.
    pub(crate) fn new_chat_view(&self) -> Option<NewChatView> {
        let dialog = self.admin.create_chat_dialog.as_ref()?;
        let facts = self
            .session()
            .map(|session| session.new_chat_facts(&dialog.stage))
            .unwrap_or_default();
        Some(view(dialog.kind, &dialog.stage, &facts))
    }

    fn new_chat_step(&self) -> Option<NewChatStep> {
        self.new_chat_view().map(|view| view.step)
    }

    /// The open dialog; one whose group was made counts as closed.
    pub(crate) fn new_chat_dialog(&self) -> Option<&NewChatDialog> {
        match self.new_chat_step()? {
            NewChatStep::Done(_) => None,
            _ => self.admin.create_chat_dialog.as_ref(),
        }
    }

    pub(crate) fn new_chat_dialog_mut(&mut self) -> Option<&mut NewChatDialog> {
        match self.new_chat_step()? {
            NewChatStep::Done(_) => None,
            _ => self.admin.create_chat_dialog.as_mut(),
        }
    }

    /// The photo button and its sources belong to the first step.
    pub(super) fn new_chat_photo_open(&self) -> bool {
        self.new_chat_step() == Some(NewChatStep::Info)
    }

    /// The chat of the type step.
    pub(super) fn new_chat_type_chat(&self) -> Option<ChatId> {
        match self.new_chat_step()? {
            NewChatStep::Type(chat) => Some(chat),
            _ => None,
        }
    }

    pub(super) fn new_chat_type_box(&self) -> Option<&GroupTypeBox> {
        self.new_chat_type_chat()?;
        self.admin.create_chat_dialog.as_ref()?.type_box.as_ref()
    }

    pub(super) fn new_chat_type_box_mut(&mut self) -> Option<&mut GroupTypeBox> {
        self.new_chat_type_chat()?;
        self.admin.create_chat_dialog.as_mut()?.type_box.as_mut()
    }

    /// The auto-delete period the group will be created with.
    fn new_chat_auto_delete(&self, dialog: &NewChatDialog) -> i32 {
        dialog.auto_delete.unwrap_or_else(|| {
            self.session()
                .and_then(|s| s.settings.default_auto_delete_secs)
                .unwrap_or(0)
        })
    }

    /// The sidebar's New Group / New supergroup / New Channel, and the
    /// window menu's.
    pub(in crate::ui) fn open_new_chat(
        &mut self,
        kind: NewChatKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(tr(kind.name_label()))
                .validate(|text, _| text.chars().count() <= TITLE_MAX)
        });
        let description_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::CREATE_GROUP_DESCRIPTION))
                .auto_grow(DESCRIPTION_ROWS.0, DESCRIPTION_ROWS.1)
                .submit_on_enter(false)
        });
        let search_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_SEARCH_CONTACTS))
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let choices: Vec<SharedString> = TTL_CHOICES
            .iter()
            .map(|secs| SharedString::from(quill::auto_delete::format_ttl(*secs)))
            .collect();
        let auto_delete_picker = cx.new(|cx| SelectState::new(choices, None, window, cx));
        cx.subscribe(&title_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(dialog) = this.admin.create_chat_dialog.as_mut() {
                    dialog.title_error = false;
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
        cx.subscribe(&search_input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let type_box = kind.is_supergroup().then(|| {
            let link_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr(keys::CREATE_GROUP_LINK))
                    .validate(|text, _| text.chars().count() <= USERNAME_MAX)
            });
            cx.subscribe(&link_input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.group_link_changed(cx);
                }
            })
            .detach();
            let draft = GroupTypeData {
                public: true,
                ..GroupTypeData::default()
            };
            GroupTypeBox::new(draft, link_input)
        });
        if let Some(live) = self.live.as_mut() {
            let _ = if kind.has_auto_delete_menu() {
                live.driver.get_default_auto_delete()
            } else {
                live.driver.check_public_chats_limit()
            };
        }
        title_input.update(cx, |input, cx| input.focus(window, cx));
        self.admin.create_chat_dialog = Some(NewChatDialog {
            kind,
            stage: NewChatStage::Info,
            title_input,
            description_input,
            search_input,
            selected_users: Vec::new(),
            photo: None,
            photo_note: None,
            sources: PhotoSources::default(),
            auto_delete: None,
            auto_delete_picker,
            title_error: false,
            type_box,
        });
        cx.notify();
    }

    pub(in crate::ui) fn close_new_chat(&mut self, cx: &mut Context<Self>) {
        self.admin.create_chat_dialog = None;
        cx.notify();
    }

    pub(in crate::ui) fn toggle_new_chat_user(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(dialog) = self.new_chat_dialog_mut() {
            if let Some(position) = dialog.selected_users.iter().position(|id| *id == user_id) {
                dialog.selected_users.remove(position);
            } else {
                dialog.selected_users.push(user_id);
            }
            cx.notify();
        }
    }

    fn set_new_chat_stage(&mut self, stage: NewChatStage, cx: &mut Context<Self>) {
        if let Some(dialog) = self.new_chat_dialog_mut() {
            dialog.stage = stage;
            dialog.search_input.update(cx, |_, cx| cx.notify());
        }
        cx.notify();
    }

    /// The first step's Next (a group) or Create: tdesktop `submit`.
    fn confirm_new_chat_info(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.new_chat_dialog() else {
            return;
        };
        if self.new_chat_view().is_some_and(|view| view.busy) {
            return;
        }
        let kind = dialog.kind;
        let title = dialog.title_input.read(cx).value().to_string();
        let description = dialog.description_input.read(cx).value().to_string();
        match plan_new_chat(kind, &title, &description, &[], None, 0) {
            Err(NewChatInvalid::EmptyTitle | NewChatInvalid::TitleTooLong) => {
                if let Some(dialog) = self.new_chat_dialog_mut() {
                    dialog.title_error = true;
                    let input = dialog.title_input.clone();
                    input.update(cx, |input, cx| input.focus(window, cx));
                }
                cx.notify();
            }
            Err(NewChatInvalid::DescriptionTooLong) => {}
            Ok(_) if kind == NewChatKind::Group => {
                self.set_new_chat_stage(NewChatStage::Members, cx);
            }
            Ok(_) => self.send_new_chat(NewChatBack::Info, cx),
        }
    }

    /// The members step's Create, or a supergroup's or channel's Create.
    fn send_new_chat(&mut self, back: NewChatBack, cx: &mut Context<Self>) {
        let Some(dialog) = self.new_chat_dialog() else {
            return;
        };
        let title = dialog.title_input.read(cx).value().to_string();
        let description = dialog.description_input.read(cx).value().to_string();
        let self_id = self.session().and_then(|s| s.my_user_id);
        let auto_delete = self.new_chat_auto_delete(dialog);
        let request = match plan_new_chat(
            dialog.kind,
            &title,
            &description,
            &dialog.selected_users,
            self_id,
            auto_delete,
        ) {
            Ok(request) => request,
            Err(_) => {
                self.set_new_chat_stage(NewChatStage::Info, cx);
                return;
            }
        };
        let photo = dialog.photo.as_ref().map(|photo| photo.input.clone());
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_CREATING_CHATS_NEEDS_A_LIVE,
            )));
            cx.notify();
            return;
        };
        match live.driver.create_new_chat(&request, photo) {
            Ok(Some(request)) => {
                self.set_new_chat_stage(NewChatStage::Creating { request, back }, cx);
            }
            Ok(None) => {}
            Err(err) => {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NEW_CHAT_FAILED,
                    &err,
                )));
                cx.notify();
            }
        }
    }

    /// The type step's Save (tdesktop `SetupChannelBox::save`): a link is
    /// set at once; Private needs nothing. Then the subscribers step.
    pub(super) fn save_new_chat_type(&mut self, draft: GroupTypeData, cx: &mut Context<Self>) {
        let Some(chat) = self.new_chat_type_chat() else {
            return;
        };
        let current = self
            .session()
            .map(|s| s.group_type_current(chat))
            .unwrap_or_default();
        let edits = plan_group_type_save(&current, &draft, false);
        if edits.is_empty() {
            self.set_new_chat_stage(NewChatStage::Subscribers { chat }, cx);
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection
                .set_note(StatusNote::info(tr(keys::Q_PEOPLE_DEMO_MODE_THIS_NEEDS_A)));
            cx.notify();
            return;
        };
        match live.driver.save_group_type(chat, &edits, &[]) {
            Ok(sent) if sent.is_empty() => {
                self.set_new_chat_stage(NewChatStage::Subscribers { chat }, cx);
            }
            Ok(sent) => self.set_new_chat_stage(
                NewChatStage::Type {
                    chat,
                    saving: Some(sent),
                },
                cx,
            ),
            Err(err) => {
                self.connection.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_SAVE_GROUP_INFO,
                    &err,
                )));
                cx.notify();
            }
        }
    }

    /// A typed link hides the refusal of the last Save.
    pub(super) fn new_chat_link_edited(&mut self) {
        if let Some(dialog) = self.admin.create_chat_dialog.as_mut()
            && let NewChatStage::Type { saving, .. } = &mut dialog.stage
        {
            *saving = None;
        }
    }

    /// Why TDLib refused the type step's link.
    pub(super) fn new_chat_username_failure(&self) -> Option<String> {
        let dialog = self.new_chat_dialog()?;
        let NewChatStage::Type {
            chat,
            saving: Some(_),
        } = &dialog.stage
        else {
            return None;
        };
        let session = self.session()?;
        if !session.new_chat_facts(&dialog.stage).type_failed {
            return None;
        }
        session
            .groups
            .info_edit_errors
            .get(&chat.0)
            .map(|error| error.reason.clone())
    }

    /// The subscribers step's Add (tdesktop `inviteSelectedUsers`).
    fn add_new_chat_subscribers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(NewChatStep::Subscribers(chat)) = self.new_chat_step() else {
            return;
        };
        let users = self
            .new_chat_dialog()
            .map(|dialog| dialog.selected_users.clone())
            .unwrap_or_default();
        if users.is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live.driver.add_chat_members(chat, &users).is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_PEOPLE_COULD_NOT_ADD_MEMBERS)));
        }
        self.finish_new_chat(chat, window, cx);
    }

    /// The last step closed: open the new chat (tdesktop `boxClosing` ->
    /// `showPeerHistory`).
    fn finish_new_chat(&mut self, chat: ChatId, window: &mut Window, cx: &mut Context<Self>) {
        self.admin.create_chat_dialog = None;
        if self
            .session()
            .is_some_and(|s| s.chats.contains_key(&chat.0))
        {
            self.select_listed_chat(chat, window, cx);
        }
        cx.notify();
    }

    /// The auto-delete step's Save, or Disable with `None`.
    fn save_new_chat_auto_delete(&mut self, disable: bool, cx: &mut Context<Self>) {
        let Some(dialog) = self.new_chat_dialog_mut() else {
            return;
        };
        let chosen = dialog
            .auto_delete_picker
            .read(cx)
            .selected_index(cx)
            .and_then(|ix| TTL_CHOICES.get(ix.row).copied());
        if disable {
            dialog.auto_delete = Some(0);
        } else if let Some(secs) = chosen {
            dialog.auto_delete = Some(secs);
        }
        self.set_new_chat_stage(NewChatStage::Info, cx);
    }

    /// The group's "Auto-Delete" menu item: the picker on the current
    /// period.
    fn open_new_chat_auto_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.new_chat_dialog() else {
            return;
        };
        let current = self.new_chat_auto_delete(dialog);
        let picker = dialog.auto_delete_picker.clone();
        let index = TTL_CHOICES
            .iter()
            .position(|secs| *secs == current)
            .or_else(|| TTL_CHOICES.iter().position(|secs| *secs >= current))
            .unwrap_or(0);
        picker.update(cx, |picker, cx| {
            picker.set_selected_index(Some(gpui_kit::component::IndexPath::new(index)), window, cx);
        });
        self.set_new_chat_stage(NewChatStage::AutoDelete, cx);
    }

    /// Escape and the close button: tdesktop closes the topmost box, so a
    /// later step steps back, the type step skips and the subscribers step
    /// finishes.
    fn dismiss_new_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.new_chat_step() {
            Some(NewChatStep::AutoDelete | NewChatStep::Members) => {
                self.set_new_chat_stage(NewChatStage::Info, cx);
            }
            Some(NewChatStep::Type(chat)) => {
                self.set_new_chat_stage(NewChatStage::Subscribers { chat }, cx);
            }
            Some(NewChatStep::Subscribers(chat)) => self.finish_new_chat(chat, window, cx),
            _ => self.close_new_chat(cx),
        }
    }

    pub(super) fn build_new_chat_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CreateChat, |this, window, cx| {
                this.dismiss_new_chat(window, cx);
            });
        app.update(cx, |this, cx| {
            this.photo_sources_prepare_render(cx);
            let dialog = dialog.overlay(true);
            let Some(view) = this
                .new_chat_view()
                .filter(|_| this.new_chat_dialog().is_some())
            else {
                return dialog.on_close(on_close);
            };
            let title = this.new_chat_title(&view, cx);
            let footer = this.new_chat_footer(&view, cx);
            let body_app = cx.entity();
            dialog
                .title(title)
                .content(crate::ui::shell::scrollable_dialog_content(
                    move |content, window, cx| {
                        let body = body_app
                            .update(cx, |this, cx| this.new_chat_body(window, cx))
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    // Under the photo crop (5444) and the video editor, which show over
    // it and hand it back.
    CreateChat => DialogSpec::new(
        4200,
        |app| app.new_chat_dialog().is_some(),
        QuillApp::build_new_chat_dialog,
    ),
}

mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod ui_tests;

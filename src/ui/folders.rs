//! folders.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::ChatList;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
/// `ReadyFolders` fixture (Phase 7.1): inject `updateChatFolders` with two
/// folders ("Work" id 1, "News" id 2) and `updateChatPosition` folder
/// positions for the seeded demo chats — chat 11 (Demo chat A) and chat 13
/// (Demo channel) go to "News", chat 12 (Demo chat B) goes to "Work".
/// The demo block then selects the "News" folder tab.
pub(super) fn apply_ready_folders(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let folders = r#"{"@type":"updateChatFolders","chat_folders":[{"@type":"chatFolderInfo","id":1,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"Work","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Work"},"color_id":2,"is_shareable":false,"has_my_invite_links":false},{"@type":"chatFolderInfo","id":2,"name":{"@type":"chatFolderName","text":{"@type":"formattedText","text":"News","entities":[]},"animate_custom_emoji":false},"icon":{"@type":"chatFolderIcon","name":"Channels"},"color_id":4,"is_shareable":false,"has_my_invite_links":false}],"main_chat_list_position":0,"are_tags_enabled":false}"#;
    if let Some(owned) = copy_and_parse(folders, seq, &dyn_sink) {
        session.apply(owned);
    }
    for (chat_id, folder_id, order) in [(11, 2, "70"), (13, 2, "60"), (12, 1, "65")] {
        let json = format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat_id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListFolder","chat_folder_id":{folder_id}}},"order":"{order}","is_pinned":false}}}}"#
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

impl QuillApp {
    /// kit Phase 2 (redo): folder editor hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    /// The existing `folder_editor_panel` builds the form (stripped of its
    /// old modal chrome/title, which the kit `Dialog` now provides);
    /// Cancel/Save live in the dialog footer.
    pub(super) fn build_folder_editor_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FolderEditor, |this, _, cx| {
                this.folder_editor = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let title = match this.folder_editor.as_ref().and_then(|d| d.folder_id) {
                Some(_) => "Edit folder",
                None => "New folder",
            };
            let body = this.folder_editor_panel(cx);
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("folder-editor-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.folder_editor = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::FolderEditor, window, cx);
                        })),
                )
                .child(
                    Button::new("folder-editor-save")
                        .label("Save")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_folder_editor(cx);
                            this.close_kit_dialog_if_done(DialogKind::FolderEditor, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): folder delete confirm hosted in a kit `Dialog`
    /// via `window.open_dialog`. Esc / backdrop / ✕ clear state via
    /// `on_close`.
    pub(super) fn build_folder_delete_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FolderDelete, |this, _, cx| {
                this.folder_delete_confirm = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(confirm) = this.folder_delete_confirm.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Delete folder"))
                    .on_close(on_close.clone());
            };
            let leave_count = this
                .session()
                .and_then(|s| s.folder_chats_to_leave.get(&confirm.folder_id))
                .map(|ids| ids.len())
                .unwrap_or(0);
            let leave_label = if leave_count > 0 {
                format!(
                    "Also leave {leave_count} suggested chat{}",
                    if leave_count == 1 { "" } else { "s" },
                )
            } else {
                "Also leave suggested chats".to_string()
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Chats stay in your main list unless you leave them."),
                )
                .child(
                    // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                    Checkbox::new("folder-delete-leave-toggle")
                        .checked(confirm.leave_with_folder)
                        .label(leave_label)
                        .on_click(cx.listener(|this, &on, window, cx| {
                            if let Some(confirm) = this.folder_delete_confirm.as_mut() {
                                confirm.leave_with_folder = on;
                            }
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::FolderDelete, window, cx);
                        })),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("folder-delete-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.folder_delete_confirm = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::FolderDelete, window, cx);
                        })),
                )
                .child(
                    Button::new("folder-delete-confirm")
                        .label("Delete")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_folder_delete(cx);
                            this.close_kit_dialog_if_done(DialogKind::FolderDelete, window, cx);
                        })),
                );
            let title = format!("Delete “{}”?", confirm.name);
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// kit Phase 2 (redo): folder manager hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_folder_manage_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::FolderManage, |this, _, cx| {
                this.close_folder_manage(cx);
            });
        app.update(cx, |this, cx| {
            let session = this.session();
            let tags_enabled = session.as_ref().is_some_and(|s| s.are_folder_tags_enabled);
            let folders: Vec<(i32, String, usize)> = session
                .as_ref()
                .map(|s| {
                    s.chat_folders
                        .iter()
                        .map(|f| {
                            let count = s
                                .chats
                                .values()
                                .filter(|c| c.folder_positions.contains_key(&f.id))
                                .count();
                            (f.id, f.name.clone(), count)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut list = div()
                .id("folder-manage-list")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(300.))
                .overflow_y_scroll();
            if folders.is_empty() {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No folders yet. Create one to organize your chats."),
                );
            }
            for (index, (folder_id, name, count)) in folders.iter().enumerate() {
                let folder_id = *folder_id;
                let is_first = index == 0;
                let is_last = index + 1 == folders.len();
                let name_label = format!("{name} ({count})");
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .child(div().text_sm().font_medium().child(name_label))
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .child(
                                    Button::new(format!("folder-up-{folder_id}"))
                                        .label("↑")
                                        .ghost()
                                        .when(is_first, |this| this.disabled(true))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.move_folder(folder_id, true, cx);
                                            this.close_kit_dialog_if_done(
                                                DialogKind::FolderManage,
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                                .child(
                                    Button::new(format!("folder-down-{folder_id}"))
                                        .label("↓")
                                        .ghost()
                                        .when(is_last, |this| this.disabled(true))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.move_folder(folder_id, false, cx);
                                            this.close_kit_dialog_if_done(
                                                DialogKind::FolderManage,
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                                .child(
                                    Button::new(format!("folder-edit-{folder_id}"))
                                        .label("Edit")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_folder_edit(folder_id, window, cx);
                                            this.close_kit_dialog_if_done(
                                                DialogKind::FolderManage,
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                                .child(
                                    Button::new(format!("folder-delete-{folder_id}"))
                                        .label("Delete")
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_folder_delete(folder_id, cx);
                                            this.close_kit_dialog_if_done(
                                                DialogKind::FolderManage,
                                                window,
                                                cx,
                                            );
                                        })),
                                ),
                        ),
                );
            }
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(list)
                .child(
                    div().flex().items_center().gap_2().child(
                        // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                        Checkbox::new("folder-tags-toggle")
                            .checked(tags_enabled)
                            .label("Show folder tags")
                            .on_click(cx.listener(move |this, &on, window, cx| {
                                if on != tags_enabled {
                                    this.toggle_folder_tags_ui(cx);
                                }
                                this.close_kit_dialog_if_done(DialogKind::FolderManage, window, cx);
                            })),
                    ),
                )
                .child(
                    Button::new("folder-create")
                        .label("New folder")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_folder_create(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::FolderManage, window, cx);
                        })),
                )
                .into_any_element();
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title("Folders"))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .on_close(on_close)
        })
    }

    /// Parity slice: create/edit folder form — name, include-type filters,
    /// per-chat include/exclude multi-select, exclude flags.
    pub(super) fn folder_editor_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.folder_editor.as_ref() else {
            return div().into_any_element();
        };
        // kit Phase 2 (redo): plain form content — the kit `Dialog`
        // provides the title, padding, and chrome via `.title()`.
        let mut panel = div().flex().flex_col().gap_2();
        panel = panel.child(Textarea::new(&dialog.name_input).aria_label("Chat folder name"));
        if let Some(error) = dialog.error.clone() {
            panel = panel.child(
                div()
                    .id("folder-editor-error")
                    .text_xs()
                    .text_color(danger_dark())
                    .child(error),
            );
        }
        if dialog.fetch_pending {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading folder…"),
                )
                .into_any_element();
        }
        // Include-type filters.
        let include_filters = [
            (
                "Contacts",
                dialog.editor.include_contacts,
                "include-contacts",
            ),
            (
                "Non-contacts",
                dialog.editor.include_non_contacts,
                "include-non-contacts",
            ),
            ("Groups", dialog.editor.include_groups, "include-groups"),
            (
                "Channels",
                dialog.editor.include_channels,
                "include-channels",
            ),
            ("Bots", dialog.editor.include_bots, "include-bots"),
        ];
        let mut filters_row = div().flex().flex_row().flex_wrap().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child("Include types:"),
        );
        for (label, checked, key) in include_filters {
            // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
            filters_row = filters_row.child(
                Checkbox::new(format!("folder-filter-{key}"))
                    .checked(checked)
                    .label(label)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if let Some(dialog) = this.folder_editor.as_mut() {
                            match key {
                                "include-contacts" => {
                                    dialog.editor.include_contacts = on;
                                }
                                "include-non-contacts" => {
                                    dialog.editor.include_non_contacts = on;
                                }
                                "include-groups" => {
                                    dialog.editor.include_groups = on;
                                }
                                "include-channels" => {
                                    dialog.editor.include_channels = on;
                                }
                                _ => {
                                    dialog.editor.include_bots = on;
                                }
                            }
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(filters_row);
        // Per-chat include/exclude multi-select.
        let mut chats: Vec<(i64, String)> = self
            .session()
            .map(|s| {
                s.chats
                    .values()
                    .map(|c| (c.id.0, c.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        chats.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        let mut chat_list = div()
            .id("folder-editor-chats")
            .flex()
            .flex_col()
            .gap_1()
            .overflow_y_scroll()
            .max_h(px(220.))
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Chats (include / exclude):"),
            );
        for (chat_id, chat_title) in chats {
            let included = dialog.editor.included.contains(&chat_id);
            let excluded = dialog.editor.excluded.contains(&chat_id);
            chat_list = chat_list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .child(div().text_sm().min_w_0().child(chat_title))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
                                // toggle_* flip membership, so fire only when the
                                // requested value differs from the rendered one.
                                Checkbox::new(("folder-include-chat", chat_id as u64))
                                    .checked(included)
                                    .label("In")
                                    .on_click(cx.listener(move |this, &on, _, cx| {
                                        if on != included {
                                            if let Some(dialog) = this.folder_editor.as_mut() {
                                                dialog.editor.toggle_included(chat_id);
                                            }
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                Checkbox::new(("folder-exclude-chat", chat_id as u64))
                                    .checked(excluded)
                                    .label("Out")
                                    .on_click(cx.listener(move |this, &on, _, cx| {
                                        if on != excluded {
                                            if let Some(dialog) = this.folder_editor.as_mut() {
                                                dialog.editor.toggle_excluded(chat_id);
                                            }
                                            cx.notify();
                                        }
                                    })),
                            ),
                    ),
            );
        }
        panel = panel.child(chat_list);
        // Exclude flags.
        let exclude_flags = [
            ("Muted", dialog.editor.exclude_muted, "exclude-muted"),
            ("Read", dialog.editor.exclude_read, "exclude-read"),
            (
                "Archived",
                dialog.editor.exclude_archived,
                "exclude-archived",
            ),
        ];
        let mut exclude_row = div().flex().flex_row().flex_wrap().gap_1().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child("Exclude:"),
        );
        for (label, checked, key) in exclude_flags {
            // Phase 6: kit Checkbox (was: ghost button with ☑/☐ label).
            exclude_row = exclude_row.child(
                Checkbox::new(format!("folder-exclude-flag-{key}"))
                    .checked(checked)
                    .label(label)
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if let Some(dialog) = this.folder_editor.as_mut() {
                            match key {
                                "exclude-muted" => {
                                    dialog.editor.exclude_muted = on;
                                }
                                "exclude-read" => {
                                    dialog.editor.exclude_read = on;
                                }
                                _ => {
                                    dialog.editor.exclude_archived = on;
                                }
                            }
                        }
                        cx.notify();
                    })),
            );
        }
        panel = panel.child(exclude_row);
        panel.into_any_element()
    }

    pub(super) fn open_folder_manage(&mut self, cx: &mut Context<Self>) {
        self.folder_manage_open = true;
        self.folder_editor = None;
        self.folder_delete_confirm = None;
        cx.notify();
    }

    pub(super) fn close_folder_manage(&mut self, cx: &mut Context<Self>) {
        self.folder_manage_open = false;
        self.folder_editor = None;
        self.folder_delete_confirm = None;
        cx.notify();
    }

    pub(super) fn open_folder_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.folder_editor = Some(FolderEditorDialog::new(window, cx, None));
        cx.notify();
    }

    pub(super) fn open_folder_edit(
        &mut self,
        folder_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Reuse the cached spec when the manage dialog just created it;
        // otherwise fetch the full folder for the prefill.
        let cached = self
            .session()
            .and_then(|s| s.folder_specs.get(&folder_id).cloned());
        let mut dialog = FolderEditorDialog::new(window, cx, Some(folder_id));
        if let Some(spec) = cached {
            dialog.prefill_from_spec(&spec, window, cx);
        } else if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_folder(folder_id)
        {
            self.status_note = format!("could not load folder: {err:?}");
        }
        self.folder_editor = Some(dialog);
        cx.notify();
    }

    /// Edit flow: once the `getChatFolder` spec arrives, prefill the open
    /// editor (no-op for create, or when already prefilled).
    pub(super) fn maybe_prefill_folder_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder_id = match self.folder_editor.as_ref() {
            Some(dialog) if dialog.fetch_pending => dialog.folder_id,
            _ => return,
        };
        let Some(folder_id) = folder_id else {
            return;
        };
        let Some(spec) = self
            .session()
            .and_then(|s| s.folder_specs.get(&folder_id).cloned())
        else {
            return;
        };
        if let Some(dialog) = self.folder_editor.as_mut() {
            dialog.prefill_from_spec(&spec, window, cx);
        }
        cx.notify();
    }

    pub(super) fn save_folder_editor(&mut self, cx: &mut Context<Self>) {
        let (name, folder_id) = match self.folder_editor.as_ref() {
            Some(dialog) => (dialog.name(cx), dialog.folder_id),
            None => return,
        };
        if let Some(dialog) = self.folder_editor.as_mut() {
            dialog.editor.name = name;
            if let Some(err) = dialog.editor.validate() {
                dialog.error = Some(err.to_string());
                cx.notify();
                return;
            }
        }
        let spec = self
            .folder_editor
            .as_ref()
            .map(|dialog| dialog.editor.to_spec());
        let Some(spec) = spec else { return };
        let result = match self.live.as_mut() {
            Some(live) => match folder_id {
                Some(id) => live.driver.edit_chat_folder(id, &spec).map(|_| ()),
                None => live.driver.create_chat_folder(&spec).map(|_| ()),
            },
            None => {
                // Screenshot demo: apply locally so the manage dialog shows
                // the change without live Telegram.
                self.apply_demo_folder_save(folder_id, spec);
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.folder_editor = None;
                self.status_note = if folder_id.is_some() {
                    "folder updated".into()
                } else {
                    "folder created".into()
                };
            }
            Err(err) => {
                if let Some(dialog) = self.folder_editor.as_mut() {
                    dialog.error = Some(format!("could not save folder: {err:?}"));
                }
            }
        }
        cx.notify();
    }

    pub(super) fn open_folder_delete(&mut self, folder_id: i32, cx: &mut Context<Self>) {
        let name = self
            .session()
            .and_then(|s| s.chat_folders.iter().find(|f| f.id == folder_id))
            .map(|f| f.name.clone())
            .unwrap_or_else(|| format!("Folder {folder_id}"));
        self.folder_delete_confirm = Some(FolderDeleteConfirm {
            folder_id,
            name,
            leave_with_folder: false,
        });
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_folder_chats_to_leave(folder_id)
        {
            self.status_note = format!("could not load folder chats: {err:?}");
        }
        cx.notify();
    }

    pub(super) fn confirm_folder_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.folder_delete_confirm.take() else {
            return;
        };
        let leave: Vec<i64> = if confirm.leave_with_folder {
            self.session()
                .and_then(|s| s.folder_chats_to_leave.get(&confirm.folder_id).cloned())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let result = match self.live.as_mut() {
            Some(live) => live
                .driver
                .delete_chat_folder(confirm.folder_id, &leave)
                .map(|_| ()),
            None => {
                self.apply_demo_folder_delete(confirm.folder_id);
                Ok(())
            }
        };
        self.status_note = match result {
            Ok(()) => "folder deleted".into(),
            Err(err) => format!("could not delete folder: {err:?}"),
        };
        if self.folder_tab == Some(confirm.folder_id) {
            self.folder_tab = None;
        }
        cx.notify();
    }

    pub(super) fn move_folder(&mut self, folder_id: i32, up: bool, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let mut ids: Vec<i32> = session.chat_folders.iter().map(|f| f.id).collect();
        let Some(pos) = ids.iter().position(|&id| id == folder_id) else {
            return;
        };
        let target = if up { pos.saturating_sub(1) } else { pos + 1 };
        if target >= ids.len() || target == pos {
            return;
        }
        ids.swap(pos, target);
        let result = match self.live.as_mut() {
            Some(live) => live.driver.reorder_chat_folders(&ids).map(|_| ()),
            None => {
                self.apply_demo_folder_reorder(&ids);
                Ok(())
            }
        };
        self.status_note = match result {
            Ok(()) => "folders reordered".into(),
            Err(err) => format!("could not reorder folders: {err:?}"),
        };
        cx.notify();
    }

    pub(super) fn toggle_folder_tags_ui(&mut self, cx: &mut Context<Self>) {
        let enabled = self
            .session()
            .map(|s| !s.are_folder_tags_enabled)
            .unwrap_or(true);
        let result = match self.live.as_mut() {
            Some(live) => live.driver.toggle_chat_folder_tags(enabled).map(|_| ()),
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.are_folder_tags_enabled = enabled;
                }
                Ok(())
            }
        };
        self.status_note = match result {
            Ok(()) if enabled => "folder tags on".into(),
            Ok(()) => "folder tags off".into(),
            Err(err) => format!("could not toggle folder tags: {err:?}"),
        };
        cx.notify();
    }

    pub(super) fn open_folder_menu(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.folder_menu_open = true;
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_lists_to_add_chat(chat_id)
        {
            self.status_note = format!("could not load folder options: {err:?}");
        }
        cx.notify();
    }

    pub(super) fn close_folder_menu(&mut self, cx: &mut Context<Self>) {
        self.folder_menu_open = false;
        cx.notify();
    }

    pub(super) fn add_open_chat_to_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
        cx: &mut Context<Self>,
    ) {
        let result = match self.live.as_mut() {
            Some(live) => live
                .driver
                .add_chat_to_folder(chat_id, folder_id)
                .map(|_| ()),
            None => Ok(()),
        };
        self.status_note = match result {
            Ok(()) => "adding chat to folder…".into(),
            Err(err) => format!("could not add chat to folder: {err:?}"),
        };
        self.folder_menu_open = false;
        cx.notify();
    }

    pub(super) fn remove_open_chat_from_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
        cx: &mut Context<Self>,
    ) {
        let result = match self.live.as_mut() {
            Some(live) => live.driver.remove_chat_from_folder(chat_id, folder_id),
            None => Ok(()),
        };
        self.status_note = match result {
            Ok(()) => "removing chat from folder…".into(),
            Err(err) => format!("could not remove chat from folder: {err:?}"),
        };
        self.folder_menu_open = false;
        cx.notify();
    }

    /// Parity slice: per-chat folder picker below the header. Destinations
    /// come from `getChatListsToAddChat` (as the schema intends); current
    /// folder memberships render as remove rows (`editChatFolder` chain —
    /// there is no `removeChatFromList` in 1.8.67).
    pub(super) fn folder_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session();
        let open = session.as_ref().and_then(|s| s.open_chat);
        let mut panel = div()
            .id("folder-menu")
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child("Chat folders"))
                    .child(
                        Button::new("close-folder-menu")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_folder_menu(cx);
                            })),
                    ),
            );
        let (Some(session), Some(chat_id)) = (session, open) else {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No chat open."),
                )
                .into_any_element();
        };
        let folder_name = |id: i32| {
            session
                .chat_folders
                .iter()
                .find(|f| f.id == id)
                .map(|f| f.name.clone())
                .unwrap_or_else(|| format!("Folder {id}"))
        };
        let Some(lists) = session.chat_lists_for_add.get(&chat_id.0) else {
            return panel
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Loading folder options…"),
                )
                .into_any_element();
        };
        if lists.is_empty() {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("No folder destinations available for this chat."),
            );
        }
        for list in lists {
            if let ChatList::Unknown = list {
                continue;
            }
            let (label, id_suffix) = match list {
                ChatList::Main => ("Move to main list".to_string(), "main".to_string()),
                ChatList::Archive => ("Archive chat".to_string(), "archive".to_string()),
                ChatList::Folder(folder_id) => (
                    format!("Add to {}", folder_name(*folder_id)),
                    format!("add-{folder_id}"),
                ),
                // Filtered above; kept for exhaustiveness.
                ChatList::Unknown => (String::new(), "unknown".to_string()),
            };
            let list = *list;
            panel = panel.child(
                Button::new(format!("folder-menu-add-{id_suffix}"))
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| match list {
                        ChatList::Main => {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.unarchive_chat(chat_id)
                            {
                                this.status_note = format!("could not move chat: {err:?}");
                            }
                            this.folder_menu_open = false;
                            cx.notify();
                        }
                        ChatList::Archive => {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.archive_chat(chat_id)
                            {
                                this.status_note = format!("could not archive chat: {err:?}");
                            }
                            this.folder_menu_open = false;
                            cx.notify();
                        }
                        ChatList::Folder(folder_id) => {
                            this.add_open_chat_to_folder(chat_id, folder_id, cx);
                        }
                        ChatList::Unknown => {}
                    })),
            );
        }
        // Current folder memberships (positional) render as remove rows.
        if let Some(chat) = session.chats.get(&chat_id.0) {
            for folder_id in chat.folder_positions.keys() {
                let folder_id = *folder_id;
                panel = panel.child(
                    Button::new(format!("folder-menu-remove-{folder_id}"))
                        .label(format!("Remove from {}", folder_name(folder_id)))
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_open_chat_from_folder(chat_id, folder_id, cx);
                        })),
                );
            }
        }
        panel.into_any_element()
    }
}

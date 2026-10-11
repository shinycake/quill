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
use quill::folder_picker::PickerMode;
use quill::ids::ChatId;
use quill::telegram::envelope::ChatList;
use std::cell::RefCell;
use std::rc::Rc;

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
                this.folders.editor = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let title = match this.folders.editor.as_ref().and_then(|d| d.folder_id) {
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
                            this.folders.editor = None;
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
                this.folders.delete_confirm = None;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let Some(confirm) = this.folders.delete_confirm.as_ref() else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Delete folder"))
                    .on_close(on_close.clone());
            };
            let muted = cx.theme().muted_foreground;
            let folder_id = confirm.folder_id;
            let suggested: Vec<i64> = if confirm.shared {
                this.session()
                    .and_then(|s| s.chat_list.folder_chats_to_leave.get(&folder_id).cloned())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let leaving = suggested
                .iter()
                .filter(|id| !confirm.keep.contains(id))
                .count();
            // tdesktop `lng_filters_delete_sure` / `lng_filters_remove_sure`.
            let intro = if confirm.has_links {
                "Are you sure you want to delete this folder? This will also deactivate all the invite links created to share this folder."
            } else {
                "This will remove the folder, your chats will not be deleted."
            };
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().id("folder-delete-text").text_sm().child(intro));
            if !suggested.is_empty() {
                let all_ticked = leaving == suggested.len();
                let select_ids = suggested.clone();
                body = body
                    .child(div().text_sm().child(format!(
                        "Do you also want to quit the chats included in the folder {}?",
                        confirm.name
                    )))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_semibold()
                                    .text_color(muted)
                                    .child(match leaving {
                                        1 => "1 chat to quit".to_string(),
                                        n => format!("{n} chats to quit"),
                                    }),
                            )
                            .child(
                                Button::new("folder-delete-select-all")
                                    .label(if all_ticked { "Deselect all" } else { "Select all" })
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(confirm) = this.folders.delete_confirm.as_mut() {
                                            if all_ticked {
                                                confirm.keep = select_ids.iter().copied().collect();
                                            } else {
                                                confirm.keep.clear();
                                            }
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
                let mut list = div()
                    .id("folder-delete-chats")
                    .flex()
                    .flex_col()
                    .gap_1()
                    .max_h(px(220.))
                    .overflow_y_scroll();
                for chat_id in suggested.iter().copied() {
                    let ticked = !confirm.keep.contains(&chat_id);
                    list = list.child(
                        Checkbox::new(("folder-delete-chat", chat_id as u64))
                            .checked(ticked)
                            .label(super::folder_share::chat_title(this.session(), chat_id))
                            .on_click(cx.listener(move |this, &on, _, cx| {
                                if let Some(confirm) = this.folders.delete_confirm.as_mut() {
                                    if on {
                                        confirm.keep.remove(&chat_id);
                                    } else {
                                        confirm.keep.insert(chat_id);
                                    }
                                }
                                cx.notify();
                            })),
                    );
                }
                body = body.child(list).child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("You can deselect the chats you don’t want to quit."),
                );
            }
            let body = body.into_any_element();
            // tdesktop: "Remove Folder and Keep Chats" until a chat is ticked.
            let confirm_label = if !suggested.is_empty() {
                match leaving {
                    0 => "Remove folder and keep chats".to_string(),
                    1 => "Remove folder and chat".to_string(),
                    _ => "Remove folder and chats".to_string(),
                }
            } else if confirm.has_links {
                "Delete".to_string()
            } else {
                "Remove".to_string()
            };
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("folder-delete-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.folders.delete_confirm = None;
                            cx.notify();
                            this.close_kit_dialog_if_done(DialogKind::FolderDelete, window, cx);
                        })),
                )
                .child(
                    Button::new("folder-delete-confirm")
                        .label(confirm_label)
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_folder_delete(cx);
                            this.close_kit_dialog_if_done(DialogKind::FolderDelete, window, cx);
                        })),
                );
            let title = if confirm.shared && !suggested.is_empty() {
                "Remove Folder".to_string()
            } else {
                format!("Delete “{}”?", confirm.name)
            };
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
            let tags_enabled = session
                .as_ref()
                .is_some_and(|s| s.chat_list.are_folder_tags_enabled);
            let folders: Vec<(i32, String, usize, String)> = session
                .as_ref()
                .map(|s| {
                    s.chat_list
                        .chat_folders
                        .iter()
                        .map(|f| {
                            let count = s
                                .chats
                                .values()
                                .filter(|c| c.folder_positions.contains_key(&f.id))
                                .count();
                            (f.id, f.name.clone(), count, f.icon_name.clone())
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Telegram's suggestions, minus the folders already made
            // (tdesktop hides a recommendation once it is added).
            let recommended: Vec<(usize, String, String, String)> = session
                .as_ref()
                .and_then(|s| s.chat_list.recommended_folders.as_ref())
                .map(|list| {
                    list.iter()
                        .enumerate()
                        .filter(|(_, r)| {
                            !folders.iter().any(|(_, name, _, _)| *name == r.spec.name)
                        })
                        .map(|(ix, r)| {
                            (
                                ix,
                                r.spec.name.clone(),
                                r.description.clone(),
                                r.spec.icon_name.clone().unwrap_or_else(|| {
                                    quill::folder_icons::default_icon_name(&r.spec).to_string()
                                }),
                            )
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
            for (index, (folder_id, name, count, icon)) in folders.iter().enumerate() {
                let folder_id = *folder_id;
                let is_first = index == 0;
                let is_last = index + 1 == folders.len();
                let name_label = format!("{name} ({count})");
                let glyph = super::folder_glyphs::folder_glyph(icon);
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .child(
                                    Icon::new(glyph)
                                        .size(px(16.))
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(div().text_sm().font_medium().truncate().child(name_label)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_none()
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
                                    Button::new(format!("folder-share-{folder_id}"))
                                        .icon(gpui_kit::assets::IconName::Link)
                                        .ghost()
                                        .tooltip("Share folder")
                                        .accessibility_label("Share folder")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_folder_share(folder_id, window, cx);
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
            let tabs_settings = this.folder_tabs_settings(cx);
            let mut recommended_section = div().flex().flex_col().gap_1();
            if !recommended.is_empty() {
                recommended_section = recommended_section.child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("Recommended folders"),
                );
            }
            for (ix, name, description, icon) in recommended {
                recommended_section = recommended_section.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .child(
                                    Icon::new(super::folder_glyphs::folder_glyph(&icon))
                                        .size(px(16.))
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .min_w_0()
                                        .child(div().text_sm().font_medium().truncate().child(name))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .truncate()
                                                .child(description),
                                        ),
                                ),
                        )
                        .child(
                            Button::new(("folder-recommended-add", ix))
                                .label("Add")
                                .small()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.add_recommended_folder(ix, cx);
                                    this.close_kit_dialog_if_done(
                                        DialogKind::FolderManage,
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                );
            }
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(list)
                .child(recommended_section)
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
                .child(tabs_settings)
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
        let Some(dialog) = self.folders.editor.as_ref() else {
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
        if let Some(mode) = dialog.picker {
            return self.folder_chat_picker(mode, cx);
        }
        // Icon picker (tdesktop `FilterIconPanel`): six per row. Until one
        // is chosen the folder shows the icon its rules produce.
        let chosen = dialog.editor.icon_name.clone();
        let default_icon = quill::folder_icons::default_icon_name(&dialog.editor.to_spec());
        let mut icon_header = div().flex().items_center().gap_2().child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(cx.theme().muted_foreground)
                .child("Choose an icon"),
        );
        if chosen.is_none() {
            icon_header = icon_header.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Default: {default_icon}")),
            );
        }
        let mut icon_grid = div().id("folder-editor-icons").flex().flex_col().gap_1();
        for (row_ix, names) in quill::folder_icons::ICON_NAMES
            .chunks(quill::folder_icons::ICONS_PER_ROW)
            .enumerate()
        {
            let mut row = div().flex().flex_row().gap_1();
            for (col_ix, name) in names.iter().copied().enumerate() {
                let ix = row_ix * quill::folder_icons::ICONS_PER_ROW + col_ix;
                let selected = chosen.as_deref() == Some(name);
                row = row.child(
                    Button::new(("folder-icon", ix))
                        .icon(super::folder_glyphs::folder_glyph(name))
                        .ghost()
                        .selected(selected)
                        .tooltip(name)
                        .accessibility_label(format!("{name} icon"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(dialog) = this.folders.editor.as_mut() {
                                // A second click returns to the default icon.
                                dialog.editor.icon_name = if selected {
                                    None
                                } else {
                                    Some(name.to_string())
                                };
                            }
                            cx.notify();
                        })),
                );
            }
            icon_grid = icon_grid.child(row);
        }
        panel = panel.child(icon_header).child(icon_grid);
        // Folder tag colour (Premium, with folder tags on).
        if let Some(picker) = self.folder_tag_picker(cx) {
            panel = panel.child(picker);
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
                        if let Some(dialog) = this.folders.editor.as_mut() {
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
        // Included / excluded chats: a counter, the chosen chats and the
        // button that opens the picker (tdesktop `edit_filter_box.cpp`).
        panel = panel
            .child(self.folder_chat_section(PickerMode::Include, cx))
            .child(self.folder_chat_section(PickerMode::Exclude, cx));
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
                        if let Some(dialog) = this.folders.editor.as_mut() {
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

    /// Making one more folder would pass the account's limit.
    fn folders_full(&self) -> bool {
        self.session().is_some_and(|s| {
            s.chat_list
                .folder_limits
                .folders_full(s.chat_list.chat_folders.len(), s.my_is_premium())
        })
    }

    /// Add recommended folder `ix` of `Session::recommended_folders`
    /// (`createChatFolder` with Telegram's own spec, as tdesktop's "Add").
    pub(super) fn add_recommended_folder(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.folders_full() {
            self.show_folder_limit(quill::folder_limits::FolderLimitKind::Folders, cx);
            return;
        }
        let Some(spec) = self
            .session()
            .and_then(|s| s.chat_list.recommended_folders.as_ref())
            .and_then(|list| list.get(ix))
            .map(|r| r.spec.clone())
        else {
            return;
        };
        let result = match self.live.as_mut() {
            Some(live) => live.driver.create_chat_folder(&spec).map(|_| ()),
            None => {
                self.apply_demo_folder_save(None, spec.clone());
                Ok(())
            }
        };
        self.connection.status_note = match result {
            Ok(()) => format!("folder {} added", spec.name),
            Err(err) => format!("could not add folder: {err:?}"),
        };
        cx.notify();
    }

    pub(super) fn open_folder_manage(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_recommended_chat_folders()
        {
            self.connection.status_note = format!("could not load recommended folders: {err:?}");
        }
        self.folders.manage_open = true;
        self.folders.editor = None;
        self.folders.delete_confirm = None;
        cx.notify();
    }

    pub(super) fn close_folder_manage(&mut self, cx: &mut Context<Self>) {
        self.folders.manage_open = false;
        self.folders.editor = None;
        self.folders.delete_confirm = None;
        cx.notify();
    }

    pub(super) fn open_folder_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.folders_full() {
            self.show_folder_limit(quill::folder_limits::FolderLimitKind::Folders, cx);
            return;
        }
        self.folders.editor = Some(FolderEditorDialog::new(window, cx, None));
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
            .and_then(|s| s.chat_list.folder_specs.get(&folder_id).cloned());
        let mut dialog = FolderEditorDialog::new(window, cx, Some(folder_id));
        if let Some(spec) = cached {
            dialog.prefill_from_spec(&spec, window, cx);
        } else if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_folder(folder_id)
        {
            self.connection.status_note = format!("could not load folder: {err:?}");
        }
        self.folders.editor = Some(dialog);
        cx.notify();
    }

    /// Edit flow: once the `getChatFolder` spec arrives, prefill the open
    /// editor (no-op for create, or when already prefilled).
    pub(super) fn maybe_prefill_folder_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder_id = match self.folders.editor.as_ref() {
            Some(dialog) if dialog.fetch_pending => dialog.folder_id,
            _ => return,
        };
        let Some(folder_id) = folder_id else {
            return;
        };
        let Some(spec) = self
            .session()
            .and_then(|s| s.chat_list.folder_specs.get(&folder_id).cloned())
        else {
            return;
        };
        if let Some(dialog) = self.folders.editor.as_mut() {
            dialog.prefill_from_spec(&spec, window, cx);
        }
        cx.notify();
    }
}

crate::ui::shell::register_dialogs! {
    FolderEditor => DialogSpec::new(
        1700,
        |app| app.folders.editor.is_some(),
        QuillApp::build_folder_editor_dialog,
    ),

    FolderDelete => DialogSpec::new(
        1800,
        |app| app.folders.delete_confirm.is_some(),
        QuillApp::build_folder_delete_dialog,
    ),

    FolderManage => DialogSpec::new(
        2300,
        |app| app.folders.manage_open,
        QuillApp::build_folder_manage_dialog,
    ),
}

mod save_folder_editor;

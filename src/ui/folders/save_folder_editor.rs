//! Methods moved out of `folders.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    pub(in crate::ui) fn save_folder_editor(&mut self, cx: &mut Context<Self>) {
        let (name, folder_id) = match self.folders.editor.as_ref() {
            Some(dialog) => (dialog.name(cx), dialog.folder_id),
            None => return,
        };
        if let Some(dialog) = self.folders.editor.as_mut() {
            dialog.editor.name = name;
            if let Some(err) = dialog.editor.validate() {
                dialog.error = Some(err.to_string());
                cx.notify();
                return;
            }
        }
        let spec = self
            .folders
            .editor
            .as_ref()
            .map(|dialog| dialog.editor.to_spec());
        let Some(spec) = spec else { return };
        // tdesktop checks the chosen-chat limits before saving.
        let over = self.session().and_then(|s| {
            s.chat_list.folder_limits.chats_over(
                spec.pinned_chat_ids.len() + spec.included_chat_ids.len(),
                spec.excluded_chat_ids.len(),
                s.my_is_premium(),
            )
        });
        if let Some(kind) = over {
            self.show_folder_limit(kind, cx);
            return;
        }
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
                self.folders.editor = None;
                self.connection.status_note = if folder_id.is_some() {
                    "folder updated".into()
                } else {
                    "folder created".into()
                };
            }
            Err(err) => {
                if let Some(dialog) = self.folders.editor.as_mut() {
                    dialog.error = Some(format!("could not save folder: {err:?}"));
                }
            }
        }
        cx.notify();
    }

    pub(in crate::ui) fn open_folder_delete(&mut self, folder_id: i32, cx: &mut Context<Self>) {
        let info = self
            .session()
            .and_then(|s| s.chat_list.chat_folders.iter().find(|f| f.id == folder_id))
            .map(|f| (f.name.clone(), f.is_shareable, f.has_my_invite_links));
        let (name, shared, has_links) =
            info.unwrap_or_else(|| (format!("Folder {folder_id}"), false, false));
        self.folders.delete_confirm = Some(FolderDeleteConfirm {
            folder_id,
            name,
            has_links,
            shared,
            keep: std::collections::HashSet::new(),
        });
        // Only a shared folder has chats worth leaving with it.
        if shared
            && let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_folder_chats_to_leave(folder_id)
        {
            self.connection.status_note = format!("could not load folder chats: {err:?}");
        }
        cx.notify();
    }

    pub(in crate::ui) fn confirm_folder_delete(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.folders.delete_confirm.take() else {
            return;
        };
        let leave: Vec<i64> = if confirm.shared {
            self.session()
                .and_then(|s| {
                    s.chat_list
                        .folder_chats_to_leave
                        .get(&confirm.folder_id)
                        .cloned()
                })
                .unwrap_or_default()
                .into_iter()
                .filter(|id| !confirm.keep.contains(id))
                .collect()
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
        self.connection.status_note = match result {
            Ok(()) => "folder deleted".into(),
            Err(err) => format!("could not delete folder: {err:?}"),
        };
        if self.folders.tab == Some(confirm.folder_id) {
            self.folders.tab = None;
        }
        cx.notify();
    }

    pub(in crate::ui) fn move_folder(&mut self, folder_id: i32, up: bool, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let mut ids: Vec<i32> = session
            .chat_list
            .chat_folders
            .iter()
            .map(|f| f.id)
            .collect();
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
        self.connection.status_note = match result {
            Ok(()) => "folders reordered".into(),
            Err(err) => format!("could not reorder folders: {err:?}"),
        };
        cx.notify();
    }

    pub(in crate::ui) fn toggle_folder_tags_ui(&mut self, cx: &mut Context<Self>) {
        let enabled = self
            .session()
            .map(|s| !s.chat_list.are_folder_tags_enabled)
            .unwrap_or(true);
        let result = match self.live.as_mut() {
            Some(live) => live.driver.toggle_chat_folder_tags(enabled).map(|_| ()),
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.chat_list.are_folder_tags_enabled = enabled;
                }
                Ok(())
            }
        };
        self.connection.status_note = match result {
            Ok(()) if enabled => "folder tags on".into(),
            Ok(()) => "folder tags off".into(),
            Err(err) => format!("could not toggle folder tags: {err:?}"),
        };
        cx.notify();
    }

    pub(in crate::ui) fn open_folder_menu(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        self.folders.menu_open = true;
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_lists_to_add_chat(chat_id)
        {
            self.connection.status_note = format!("could not load folder options: {err:?}");
        }
        cx.notify();
    }

    pub(in crate::ui) fn close_folder_menu(&mut self, cx: &mut Context<Self>) {
        self.folders.menu_open = false;
        cx.notify();
    }

    /// "{chat} added to {folder} folder" for the folder picker.
    pub(super) fn membership_toast(&self, chat_id: ChatId, folder_id: i32, added: bool) -> String {
        let session = self.session();
        let chat = session
            .and_then(|s| s.chats.get(&chat_id.0))
            .map_or_else(|| "Chat".to_string(), |c| c.title.clone());
        let folder = session
            .and_then(|s| s.chat_list.chat_folders.iter().find(|f| f.id == folder_id))
            .map_or_else(|| format!("Folder {folder_id}"), |f| f.name.clone());
        quill::folders::folder_membership_toast(&chat, &folder, added)
    }

    pub(in crate::ui) fn add_open_chat_to_folder(
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
        let toast = self.membership_toast(chat_id, folder_id, true);
        self.connection.status_note = match result {
            Ok(()) => toast,
            Err(err) => format!("could not add chat to folder: {err:?}"),
        };
        self.folders.menu_open = false;
        cx.notify();
    }

    pub(in crate::ui) fn remove_open_chat_from_folder(
        &mut self,
        chat_id: ChatId,
        folder_id: i32,
        cx: &mut Context<Self>,
    ) {
        let result = match self.live.as_mut() {
            Some(live) => live.driver.remove_chat_from_folder(chat_id, folder_id),
            None => Ok(()),
        };
        let toast = self.membership_toast(chat_id, folder_id, false);
        self.connection.status_note = match result {
            Ok(()) => toast,
            Err(err) => format!("could not remove chat from folder: {err:?}"),
        };
        self.folders.menu_open = false;
        cx.notify();
    }

    /// Parity slice: per-chat folder picker below the header. Destinations
    /// come from `getChatListsToAddChat` (as the schema intends); current
    /// folder memberships render as remove rows (`editChatFolder` chain —
    /// there is no `removeChatFromList` in 1.8.67).
    pub(in crate::ui) fn folder_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .icon(gpui_kit::assets::IconName::X)
                            .tooltip("Close")
                            .accessibility_label("Close")
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
                .chat_list
                .chat_folders
                .iter()
                .find(|f| f.id == id)
                .map(|f| f.name.clone())
                .unwrap_or_else(|| format!("Folder {id}"))
        };
        let Some(lists) = session.chat_list.chat_lists_for_add.get(&chat_id.0) else {
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
                                this.connection.status_note =
                                    format!("could not move chat: {err:?}");
                            }
                            this.folders.menu_open = false;
                            cx.notify();
                        }
                        ChatList::Archive => {
                            if let Some(live) = this.live.as_mut()
                                && let Err(err) = live.driver.archive_chat(chat_id)
                            {
                                this.connection.status_note =
                                    format!("could not archive chat: {err:?}");
                            }
                            this.folders.menu_open = false;
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

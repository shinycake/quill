//! The folder editor's stages and the recommended folders' adds: each waits
//! for the answer to its own request (`DialogAnswers`).

use super::*;

impl QuillApp {
    /// Add the recommended folder called `name` from
    /// `Session::recommended_folders` (`createChatFolder` with Telegram's
    /// own spec, as tdesktop's "Add"). A recommendation already being added
    /// waits for its answer, so a double tap makes one folder.
    pub(in crate::ui) fn add_recommended_folder(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.folders.recommended_adds.contains_key(name) {
            return;
        }
        if self.folders_full() {
            self.show_folder_limit(quill::folder_limits::FolderLimitKind::Folders, cx);
            return;
        }
        let Some(spec) = self
            .session()
            .and_then(|s| s.chat_list.recommended_folders.as_ref())
            .and_then(|list| list.iter().find(|r| r.spec.name == name))
            .map(|r| r.spec.clone())
        else {
            return;
        };
        match self.live.as_mut() {
            Some(live) => match live.driver.create_chat_folder(&spec) {
                Ok(request) => {
                    self.folders.recommended_adds.insert(spec.name, request);
                }
                Err(err) => self.set_note(StatusNote::error(user_error_for(
                    keys::Q_NOTES_FAILED_ADD_FOLDER,
                    &err,
                ))),
            },
            None => {
                self.apply_demo_folder_save(None, spec.clone());
                self.set_note(StatusNote::silent(tr_with(
                    keys::Q_CHATLIST_FOLDER_ADDED_2,
                    &[("name", &spec.name.to_string())],
                )));
            }
        }
        cx.notify();
    }

    /// Poll-loop half: a recommended folder's add answered. Its row ends in
    /// the toast (the row itself goes once the folder is in the list).
    pub(in crate::ui) fn settle_recommended_adds(&mut self, cx: &mut Context<Self>) -> bool {
        if self.folders.recommended_adds.is_empty() {
            return false;
        }
        let waiting: Vec<(String, quill::ids::RequestId)> = self
            .folders
            .recommended_adds
            .iter()
            .map(|(name, request)| (name.clone(), *request))
            .collect();
        let mut changed = false;
        for (name, request) in waiting {
            let Some(answer) = self
                .session_mut()
                .and_then(|s| s.chat_list.dialog_answers.take(request))
            else {
                continue;
            };
            self.folders.recommended_adds.remove(&name);
            use quill::state::DialogAnswer;
            match answer {
                DialogAnswer::Failed(line) => self.set_note(StatusNote::error(line)),
                // The limit box says it; no toast on top.
                DialogAnswer::LimitHit => {}
                DialogAnswer::Done | DialogAnswer::Checked(_) => {
                    self.set_note(StatusNote::silent(tr_with(
                        keys::Q_CHATLIST_FOLDER_ADDED_2,
                        &[("name", &name.to_string())],
                    )))
                }
            }
            changed = true;
        }
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::ui) fn open_folder_edit(
        &mut self,
        folder_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Reuse the cached spec while nothing changed the folder since it
        // was fetched; otherwise fetch the full folder for the prefill (a
        // Save writes the prefilled chat lists back).
        let cached = self
            .session()
            .and_then(|s| s.chat_list.fresh_spec(folder_id).cloned());
        let mut dialog = FolderEditorDialog::new(window, cx, Some(folder_id));
        if let Some(spec) = cached {
            dialog.prefill_from_spec(&spec, window, cx);
        } else if let Some(live) = self.live.as_mut()
            && let Err(err) = live.driver.fetch_chat_folder(folder_id)
        {
            let line = user_error_for(keys::Q_NOTES_FAILED_FOLDER, &err);
            dialog.load = FolderLoad::Failed(line.clone());
            self.set_note(StatusNote::error(line));
        }
        self.folders.editor = Some(dialog);
        self.open_box(DialogKind::FolderEditor);
        cx.notify();
    }

    /// Edit flow: while the editor loads, a `getChatFolder` spec that
    /// arrives prefills it, and a failed one ends it in the error with a
    /// Retry (no-op for create, or once settled).
    pub(in crate::ui) fn maybe_prefill_folder_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(FolderLoad::Saving(request)) = self.folders.editor.as_ref().map(|d| &d.load) {
            self.settle_folder_editor_save(*request, window, cx);
            return;
        }
        if self
            .folders
            .editor
            .as_ref()
            .is_some_and(|d| d.load == FolderLoad::Merging)
        {
            self.settle_folder_editor_merge(cx);
            return;
        }
        let folder_id = match self.folders.editor.as_ref() {
            Some(dialog) if dialog.load == FolderLoad::Loading => dialog.folder_id,
            _ => return,
        };
        let Some(folder_id) = folder_id else {
            return;
        };
        let session = self.session();
        let spec = session.and_then(|s| s.chat_list.fresh_spec(folder_id).cloned());
        let error = session.and_then(|s| s.chat_list.folder_spec_errors.get(&folder_id).cloned());
        let Some(dialog) = self.folders.editor.as_mut() else {
            return;
        };
        if let Some(spec) = spec {
            dialog.prefill_from_spec(&spec, window, cx);
        } else if let Some(error) = error {
            dialog.load = FolderLoad::Failed(error);
        } else {
            return;
        }
        cx.notify();
    }

    /// Save found no fresh copy of the folder and asked for it
    /// (`start_folder_editor_merge`). Once it is here the edits go onto it
    /// and the save goes out; a failed fetch puts the form back with the
    /// error and the edits kept. Nothing is asked here while the request
    /// is in flight; only a copy that went stale again with nothing in
    /// flight is asked for once more, and a send that fails ends the stage.
    fn settle_folder_editor_merge(&mut self, cx: &mut Context<Self>) {
        let Some(folder_id) = self.folders.editor.as_ref().and_then(|d| d.folder_id) else {
            return;
        };
        let fresh = self
            .session()
            .and_then(|s| s.chat_list.fresh_spec(folder_id).cloned());
        if fresh.is_some() {
            let merged = self
                .folders
                .editor
                .as_ref()
                .and_then(|d| d.editor.spec_onto_current(&d.base, fresh.as_ref()));
            let Some(spec) = merged else { return };
            if let Some(dialog) = self.folders.editor.as_mut() {
                dialog.load = FolderLoad::Ready;
            }
            self.send_folder_editor_spec(Some(folder_id), spec, cx);
            return;
        }
        let error = self
            .session()
            .and_then(|s| s.chat_list.folder_spec_errors.get(&folder_id).cloned());
        let in_flight = self.session().is_some_and(|s| {
            s.requests
                .has_purpose_for_folder(quill::state::RequestPurpose::GetChatFolder, folder_id)
        });
        let error = match error {
            Some(error) => error,
            None if in_flight => return,
            None => {
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.fetch_chat_folder(folder_id));
                match sent {
                    Some(Ok(_)) => return,
                    Some(Err(err)) => user_error_for(keys::Q_NOTES_FAILED_FOLDER, &err),
                    None => return,
                }
            }
        };
        if let Some(dialog) = self.folders.editor.as_mut() {
            dialog.load = FolderLoad::Ready;
            dialog.error = Some(error);
        }
        cx.notify();
    }

    /// The editor's Save waits for the answer to its request: the folder
    /// is saved and the box closes, or the error shows in the box.
    fn settle_folder_editor_save(
        &mut self,
        request: quill::ids::RequestId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use quill::state::DialogAnswer;
        let answer = self
            .session_mut()
            .and_then(|s| s.chat_list.dialog_answers.take(request));
        match answer {
            None => return,
            Some(DialogAnswer::Failed(line)) => {
                if let Some(dialog) = self.folders.editor.as_mut() {
                    dialog.load = FolderLoad::Ready;
                    dialog.error = Some(line);
                }
            }
            // The limit box is up; the form stays as it was, with no line.
            Some(DialogAnswer::LimitHit) => {
                if let Some(dialog) = self.folders.editor.as_mut() {
                    dialog.load = FolderLoad::Ready;
                    dialog.error = None;
                }
            }
            Some(DialogAnswer::Done | DialogAnswer::Checked(_)) => {
                let edited = self
                    .folders
                    .editor
                    .as_ref()
                    .is_some_and(|dialog| dialog.folder_id.is_some());
                self.folder_editor_saved(edited);
                self.close_kit_dialog_if_done(DialogKind::FolderEditor, window, cx);
            }
        }
        cx.notify();
    }

    /// The editor's Retry after a failed `getChatFolder`.
    pub(in crate::ui) fn retry_folder_editor_load(&mut self, cx: &mut Context<Self>) {
        let Some(folder_id) = self.folders.editor.as_ref().and_then(|d| d.folder_id) else {
            return;
        };
        let sent = match self.live.as_mut() {
            Some(live) => live.driver.fetch_chat_folder(folder_id).map(|_| ()),
            None => Ok(()),
        };
        let failed = sent
            .err()
            .map(|err| user_error_for(keys::Q_NOTES_FAILED_FOLDER, &err));
        if let Some(dialog) = self.folders.editor.as_mut() {
            dialog.load = match failed {
                Some(line) => FolderLoad::Failed(line),
                None => FolderLoad::Loading,
            };
        }
        cx.notify();
    }
}

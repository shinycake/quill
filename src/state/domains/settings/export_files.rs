//! The session's side of the cache copies exports download for themselves
//! (`export::leftovers`): the files the view asked for, the deletes held
//! back for a chat on screen, the deletes and checks in flight, and which
//! entry each one clears when TDLib answers.
use crate::export::leftovers::{Earlier, Leftovers};
use crate::ids::RequestId;
use crate::json_access::JsonExt;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ExportFiles {
    leftovers: Option<Leftovers>,
    /// Files the view asked for while an export ran. Their cache copy
    /// stays even when an export downloaded them first.
    view_files: HashSet<i32>,
    /// Deletes held back while their chat is on screen: (chat, file). A
    /// row that loads meanwhile finds the file in the cache.
    held: Vec<(i64, i32)>,
    /// `getRemoteFile` for an entry an earlier run left.
    checks: HashMap<RequestId, Earlier>,
    /// A `deleteFile` in flight and the entry its answer clears.
    deletes: HashMap<RequestId, String>,
    /// Earlier entries found untouched: this run's file id and the entry.
    due: Vec<(i32, String)>,
}

impl ExportFiles {
    pub fn leftovers(&self) -> Option<&Leftovers> {
        self.leftovers.as_ref()
    }

    pub(crate) fn set_leftovers(&mut self, leftovers: Leftovers) {
        self.leftovers = Some(leftovers);
    }

    pub(crate) fn earlier(&self) -> Vec<Earlier> {
        self.leftovers
            .as_ref()
            .map(Leftovers::earlier)
            .unwrap_or_default()
    }

    pub(crate) fn checking(&mut self, request: RequestId, earlier: Earlier) {
        self.checks.insert(request, earlier);
    }

    /// `request` deletes this run's `file_id`: its answer clears the entry.
    pub(crate) fn deleting(&mut self, request: RequestId, file_id: i32) {
        if let Some(leftovers) = &self.leftovers {
            self.deletes.insert(request, leftovers.name(file_id));
        }
    }

    pub(crate) fn deleting_entry(&mut self, request: RequestId, name: String) {
        self.deletes.insert(request, name);
    }

    /// Something else keeps `file_id`'s cache copy: nothing to delete.
    pub(crate) fn kept(&self, file_id: i32) {
        if let Some(leftovers) = &self.leftovers {
            leftovers.forget(&leftovers.name(file_id));
        }
    }

    pub(crate) fn note_view_file(&mut self, file_id: i32) {
        self.view_files.insert(file_id);
    }

    pub(crate) fn view_keeps(&self, file_id: i32) -> bool {
        self.view_files.contains(&file_id)
    }

    pub(crate) fn holds_any(&self) -> bool {
        !self.held.is_empty()
    }

    pub(crate) fn hold(&mut self, chat: i64, file_id: i32) {
        self.held.push((chat, file_id));
    }

    /// The held deletes that go now: each whose chat left the screen, or
    /// every one once no export runs. A file the view asked for meanwhile
    /// stays in the cache and leaves the list.
    pub(crate) fn release_held(
        &mut self,
        shown: impl Fn(i64) -> bool,
        exporting: bool,
    ) -> Vec<i32> {
        let (keep, release): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|(chat, _)| exporting && shown(*chat));
        self.held = keep;
        let (kept, gone): (Vec<_>, Vec<_>) = release
            .into_iter()
            .map(|(_, file_id)| file_id)
            .partition(|file_id| self.view_files.contains(file_id));
        for file_id in kept {
            self.kept(file_id);
        }
        gone
    }

    /// With no export left and nothing held, what the view asked for no
    /// longer matters.
    pub(crate) fn forget_view_files(&mut self) {
        if self.held.is_empty() {
            self.view_files.clear();
        }
    }

    pub(crate) fn take_due(&mut self) -> Vec<(i32, String)> {
        std::mem::take(&mut self.due)
    }

    /// TDLib answered `request`, maybe one of ours.
    pub(crate) fn answer(&mut self, request: RequestId, value: &Value) {
        let Some(leftovers) = &self.leftovers else {
            return;
        };
        if let Some(name) = self.deletes.remove(&request) {
            leftovers.forget(&name);
        }
        if let Some(earlier) = self.checks.remove(&request) {
            match value
                .at("id")
                .as_i64()
                .and_then(|id| i32::try_from(id).ok())
            {
                Some(file_id) if untouched(value, earlier.entry.path.as_deref()) => {
                    self.due.push((file_id, earlier.name));
                }
                _ => leftovers.forget(&earlier.name),
            }
        }
    }
}

/// Whether the cache holds nothing of `file` but what the export left: a
/// part of its download, or the copy it finished at `downloaded`.
fn untouched(file: &Value, downloaded: Option<&Path>) -> bool {
    let local = file.at("local");
    let at = local
        .at("path")
        .as_str()
        .and_then(|path| Path::new(path).canonicalize().ok());
    file.at("@type") == "file"
        && local.at("is_downloading_active") == false
        && (local.at("is_downloading_completed") == false
            || at.is_some_and(|at| Some(at) == downloaded.map(PathBuf::from)))
}

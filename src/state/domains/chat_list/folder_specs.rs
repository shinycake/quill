//! The cached full specs of the account's chat folders (`getChatFolder`).
//!
//! Each folder's entry is one stage: the spec as fetched, the spec that may
//! be behind the server, or the failure of the last fetch. An edit must
//! start from a fresh spec, or it writes the old chat lists back for every
//! device.
use crate::state::*;
use std::collections::HashMap;

/// One folder's cached spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecEntry {
    /// The spec as it was when fetched.
    Fresh(ChatFolderSpec),
    /// The folder changed since it was fetched (`updateChatFolders`, an
    /// edit or an "Add to folder"), possibly from another device: the spec
    /// stays for display, but the next edit fetches it again.
    Stale(ChatFolderSpec),
    /// `getChatFolder` failed with this line: the folder editor waiting on
    /// the spec ends in it (and a Retry) instead of loading forever. The
    /// last spec, if any, stays for display. Asking again or a landed spec
    /// ends it.
    Failed {
        error: String,
        last: Option<ChatFolderSpec>,
    },
}

/// The specs by folder id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderSpecs(HashMap<i32, SpecEntry>);

impl FolderSpecs {
    /// The spec held for display, fresh or not.
    pub fn get(&self, folder_id: &i32) -> Option<&ChatFolderSpec> {
        match self.0.get(folder_id)? {
            SpecEntry::Fresh(spec) | SpecEntry::Stale(spec) => Some(spec),
            SpecEntry::Failed { last, .. } => last.as_ref(),
        }
    }

    /// The spec when nothing changed the folder since it was fetched: the
    /// only spec an edit may start from.
    pub fn fresh(&self, folder_id: i32) -> Option<&ChatFolderSpec> {
        match self.0.get(&folder_id)? {
            SpecEntry::Fresh(spec) => Some(spec),
            SpecEntry::Stale(_) | SpecEntry::Failed { .. } => None,
        }
    }

    /// The failure line of the folder's last fetch.
    pub fn error(&self, folder_id: i32) -> Option<&str> {
        match self.0.get(&folder_id)? {
            SpecEntry::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Some folder's fetch failed.
    pub fn any_error(&self) -> bool {
        self.0
            .values()
            .any(|entry| matches!(entry, SpecEntry::Failed { .. }))
    }

    /// A spec is held for the folder.
    pub fn contains_key(&self, folder_id: &i32) -> bool {
        self.get(folder_id).is_some()
    }

    /// A `getChatFolder` answer: the folder's spec as it is now.
    pub fn insert(&mut self, folder_id: i32, spec: ChatFolderSpec) {
        self.0.insert(folder_id, SpecEntry::Fresh(spec));
    }

    /// The folder was edited through the dialog or deleted.
    pub fn remove(&mut self, folder_id: &i32) {
        self.0.remove(folder_id);
    }

    /// `getChatFolder` failed.
    pub fn fail(&mut self, folder_id: i32, error: String) {
        let last = self.get(&folder_id).cloned();
        self.0.insert(folder_id, SpecEntry::Failed { error, last });
    }

    /// The spec is asked again: the last failure is gone.
    pub fn clear_error(&mut self, folder_id: i32) {
        match self.0.remove(&folder_id) {
            Some(SpecEntry::Failed {
                last: Some(spec), ..
            }) => {
                self.0.insert(folder_id, SpecEntry::Stale(spec));
            }
            Some(SpecEntry::Failed { last: None, .. }) | None => {}
            Some(other) => {
                self.0.insert(folder_id, other);
            }
        }
    }

    /// Something changed the folder: its cached spec is fetched again
    /// before an edit.
    pub fn mark_stale(&mut self, folder_id: i32) {
        if let Some(SpecEntry::Fresh(spec)) = self.0.get(&folder_id) {
            let spec = spec.clone();
            self.0.insert(folder_id, SpecEntry::Stale(spec));
        }
    }

    /// Some folder changed (`updateChatFolders` names no folder): every
    /// cached spec is fetched again before an edit.
    pub fn mark_all_stale(&mut self) {
        let all: Vec<(i32, SpecEntry)> = self.0.drain().collect();
        for (folder_id, entry) in all {
            let entry = match entry {
                SpecEntry::Fresh(spec) => SpecEntry::Stale(spec),
                other => other,
            };
            self.0.insert(folder_id, entry);
        }
    }
}

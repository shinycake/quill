//! The cached full specs of the account's chat folders (`getChatFolder`).
//!
//! Each folder's entry is one [`Fetch`]: `Ready` is the spec as fetched,
//! `Outdated` the spec that may be behind the server, `Loading` the request
//! that asked (only its own answer lands), `Failed` the failure of the last
//! fetch. An edit must start from a `Ready` spec, or it writes the old chat
//! lists back for every device.
use crate::state::fetch::Fetch;
use crate::state::*;
use std::collections::HashMap;

/// The specs by folder id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderSpecs(HashMap<i32, Fetch<ChatFolderSpec>>);

impl FolderSpecs {
    /// The spec held for display, fresh or not.
    pub fn get(&self, folder_id: &i32) -> Option<&ChatFolderSpec> {
        self.0.get(folder_id)?.data()
    }

    /// The spec when nothing changed the folder since it was fetched: the
    /// only spec an edit may start from.
    pub fn fresh(&self, folder_id: i32) -> Option<&ChatFolderSpec> {
        match self.0.get(&folder_id)? {
            Fetch::Ready(spec) => Some(spec),
            _ => None,
        }
    }

    /// The failure line of the folder's last fetch.
    pub fn error(&self, folder_id: i32) -> Option<&str> {
        self.0.get(&folder_id)?.error()
    }

    /// Some folder's fetch failed.
    pub fn any_error(&self) -> bool {
        self.0.values().any(|entry| entry.error().is_some())
    }

    /// A spec is held for the folder.
    pub fn contains_key(&self, folder_id: &i32) -> bool {
        self.get(folder_id).is_some()
    }

    /// The folder's `getChatFolder` is out.
    pub fn is_loading(&self, folder_id: i32) -> bool {
        self.0.get(&folder_id).is_some_and(Fetch::is_loading)
    }

    /// The request the folder's fetch waits for.
    pub fn request(&self, folder_id: i32) -> Option<RequestId> {
        self.0.get(&folder_id)?.request()
    }

    /// The folder's fetch, for the sender to `begin()` and `sent()`.
    pub fn fetch_mut(&mut self, folder_id: i32) -> &mut Fetch<ChatFolderSpec> {
        self.0.entry(folder_id).or_default()
    }

    /// A spec the reducer was handed whole (a demo seed).
    pub fn insert(&mut self, folder_id: i32, spec: ChatFolderSpec) {
        self.0.insert(folder_id, Fetch::Ready(spec));
    }

    /// The folder was edited through the dialog or deleted: its fetch goes
    /// with it, so an answer still out finds nothing waiting.
    pub fn remove(&mut self, folder_id: &i32) {
        self.0.remove(folder_id);
    }

    /// `getChatFolder` answered as `request`: taken only by the fetch that
    /// waits for it.
    pub fn answered(&mut self, folder_id: i32, request: Option<RequestId>, spec: ChatFolderSpec) {
        if let Some(fetch) = self.0.get_mut(&folder_id) {
            fetch.answered(request, spec);
        }
    }

    /// `getChatFolder` failed as `request`.
    /// `false` for an older request: the fetch that replaced it asks again.
    pub fn failed(&mut self, folder_id: i32, request: Option<RequestId>, error: String) -> bool {
        self.0
            .get_mut(&folder_id)
            .is_some_and(|fetch| fetch.failed(request, error, false))
    }

    /// `getChatFolder` was refused before it left.
    pub fn fail(&mut self, folder_id: i32, error: String) {
        self.0.entry(folder_id).or_default().fail(error);
    }

    /// Something changed the folder: its spec stays for display, a fetch
    /// still out is dropped (it asked before the change), and the next edit
    /// fetches again. A failure stays what it is.
    pub fn mark_stale(&mut self, folder_id: i32) {
        if let Some(fetch) = self.0.get_mut(&folder_id) {
            Self::outdate(fetch);
        }
    }

    /// Some folder changed (`updateChatFolders` names no folder): every
    /// cached spec is fetched again before an edit.
    pub fn mark_all_stale(&mut self) {
        self.0.values_mut().for_each(Self::outdate);
    }

    fn outdate(fetch: &mut Fetch<ChatFolderSpec>) {
        if matches!(fetch, Fetch::Ready(_) | Fetch::Loading { .. }) {
            fetch.outdate();
        }
    }
}

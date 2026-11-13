//! The notification defaults per scope (`getScopeNotificationSettings`):
//! one value per scope that says whether the defaults are unknown, on their
//! way under a request, in, or refused. Readers see the settings only while
//! they are held; the stage never sits beside them in a second collection.
use crate::state::fetch::Fetch;
use crate::state::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScopeSettings {
    scopes: HashMap<NotificationSettingsScope, Fetch<ScopeNotificationSettings>>,
}

impl ScopeSettings {
    /// The defaults of `scope`, while they are held (also under a refetch).
    pub fn get(&self, scope: &NotificationSettingsScope) -> Option<&ScopeNotificationSettings> {
        self.scopes.get(scope)?.data()
    }

    pub fn get_mut(
        &mut self,
        scope: &NotificationSettingsScope,
    ) -> Option<&mut ScopeNotificationSettings> {
        self.scopes.get_mut(scope)?.data_mut()
    }

    pub fn contains_key(&self, scope: &NotificationSettingsScope) -> bool {
        self.get(scope).is_some()
    }

    /// The defaults are known: an update, or an edit shown at once. A fetch
    /// still out is dropped (its answer is older than this value).
    pub fn insert(
        &mut self,
        scope: NotificationSettingsScope,
        settings: ScopeNotificationSettings,
    ) {
        self.scopes.insert(scope, Fetch::Ready(settings));
    }

    /// The defaults go (a refused edit with nothing held before it).
    pub fn remove(&mut self, scope: &NotificationSettingsScope) {
        self.scopes.remove(scope);
    }

    /// A `getScopeNotificationSettings` for `scope` goes out as `request`.
    pub fn begin(&mut self, scope: NotificationSettingsScope, request: RequestId) {
        let fetch = self.scopes.entry(scope).or_default();
        fetch.begin_with(request);
    }

    /// Whether the defaults of `scope` are asked for: never asked and not
    /// refused. A refused scope waits for [`ScopeSettings::reopen`].
    pub fn wants_fetch(&self, scope: &NotificationSettingsScope) -> bool {
        self.scopes.get(scope).is_none_or(Fetch::is_idle)
    }

    pub fn answered(
        &mut self,
        scope: NotificationSettingsScope,
        request: Option<RequestId>,
        settings: ScopeNotificationSettings,
    ) -> bool {
        self.scopes
            .get_mut(&scope)
            .is_some_and(|fetch| fetch.answered(request, settings))
    }

    pub fn failed(
        &mut self,
        scope: &NotificationSettingsScope,
        request: Option<RequestId>,
        error: String,
    ) {
        if let Some(fetch) = self.scopes.get_mut(scope) {
            fetch.failed(request, error, false);
        }
    }

    /// The request never left: back to not asked.
    pub fn cancel(&mut self, scope: &NotificationSettingsScope) {
        if let Some(fetch) = self.scopes.get_mut(scope) {
            fetch.cancel();
            if fetch.is_idle() {
                self.scopes.remove(scope);
            }
        }
    }

    /// The Notifications page opened: scopes whose load was refused may be
    /// asked again.
    pub fn reopen(&mut self) {
        self.scopes.retain(|_, fetch| match fetch {
            Fetch::Failed { stale: None, .. } => false,
            Fetch::Failed { stale: Some(_), .. } => {
                fetch.clear_error();
                true
            }
            _ => true,
        });
    }

    /// Why the defaults of `scope` could not be loaded.
    pub fn error(&self, scope: &NotificationSettingsScope) -> Option<&str> {
        self.scopes.get(scope)?.error()
    }
}

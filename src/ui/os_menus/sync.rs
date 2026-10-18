//! When the Dock and Window menus are rebuilt. Applied update batches wake
//! the menu task, but a batch rarely changes what the menus show: the
//! model is built from memory (cheap) against the registry last read, and
//! the registry is read from disk again only when the model moved, the
//! registry is old, nothing was read yet, the registry was written since
//! (`quill::settings::registry_writes`), or the set of background clients
//! changed.
#![deny(clippy::indexing_slicing)]

use super::{DockModel, Registry, WindowMenuState};
use quill::ids::AccountKey;
use std::time::{Duration, Instant};

/// How long the account registry read from disk is trusted when nothing
/// else changed (the registry is what no update announces).
pub(super) const REGISTRY_MAX_AGE: Duration = Duration::from_secs(10);

/// What the menu task installed and the registry it built it from.
#[derive(Default)]
pub(super) struct MenuSync {
    registry: Registry,
    registry_read: Option<Instant>,
    /// `registry_writes()` and the background client keys when it was read.
    read_at: (u64, Vec<AccountKey>),
    installed: Option<(DockModel, WindowMenuState)>,
}

impl MenuSync {
    /// The registry last read.
    pub(super) fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Whether the registry must be read again before the menus are
    /// compared: nothing read yet, the read is old, the registry was written
    /// or a background client was added or removed since, or the model built
    /// from what is in memory differs from the installed one.
    pub(super) fn needs_registry(
        &self,
        now: Instant,
        current: &(DockModel, WindowMenuState),
        writes: u64,
        clients: &[AccountKey],
    ) -> bool {
        match self.registry_read {
            None => true,
            Some(read) => {
                now.saturating_duration_since(read) >= REGISTRY_MAX_AGE
                    || self.read_at.0 != writes
                    || self.read_at.1 != clients
                    || self.installed.as_ref() != Some(current)
            }
        }
    }

    /// A fresh registry read at `now`, taken when the registry had been
    /// written `writes` times and `clients` were running.
    pub(super) fn set_registry(
        &mut self,
        registry: Registry,
        now: Instant,
        writes: u64,
        clients: Vec<AccountKey>,
    ) {
        self.registry = registry;
        self.registry_read = Some(now);
        self.read_at = (writes, clients);
    }

    /// Record the menus now shown; returns whether the Dock menu and the
    /// Window menu need to be set again.
    pub(super) fn install(&mut self, model: DockModel, state: WindowMenuState) -> (bool, bool) {
        let changed = match &self.installed {
            Some((old_model, old_state)) => (old_model != &model, old_state != &state),
            None => (true, true),
        };
        self.installed = Some((model, state));
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuSync, REGISTRY_MAX_AGE};
    use crate::ui::os_menus::{DockModel, Registry, WindowMenuState};
    use quill::ids::AccountKey;
    use std::time::{Duration, Instant};

    fn pair(locked: bool) -> (DockModel, WindowMenuState) {
        (
            DockModel {
                locked,
                ..DockModel::default()
            },
            WindowMenuState::default(),
        )
    }

    #[test]
    fn the_first_pass_reads_the_registry_and_installs_both_menus() {
        let sync = MenuSync::default();
        assert!(sync.needs_registry(Instant::now(), &pair(false), 0, &[]));
        let mut sync = sync;
        let (model, state) = pair(false);
        assert_eq!(sync.install(model, state), (true, true));
    }

    #[test]
    fn update_batches_that_change_nothing_rebuild_nothing() {
        let mut sync = MenuSync::default();
        let t0 = Instant::now();
        sync.set_registry(Registry::default(), t0, 0, Vec::new());
        let (model, state) = pair(false);
        sync.install(model, state);
        // Many batches inside the registry's age: no disk read, no rebuild.
        for ms in [50u64, 400, 2_000, 9_000] {
            let at = t0 + Duration::from_millis(ms);
            assert!(!sync.needs_registry(at, &pair(false), 0, &[]), "at {ms} ms");
            let (model, state) = pair(false);
            assert_eq!(sync.install(model, state), (false, false));
        }
    }

    #[test]
    fn a_change_the_menus_show_reads_the_registry_and_rebuilds_the_dock() {
        let mut sync = MenuSync::default();
        let t0 = Instant::now();
        sync.set_registry(Registry::default(), t0, 0, Vec::new());
        let (model, state) = pair(false);
        sync.install(model, state);
        let soon = t0 + Duration::from_millis(100);
        assert!(
            sync.needs_registry(soon, &pair(true), 0, &[]),
            "locked changed"
        );
        let (model, state) = pair(true);
        assert_eq!(sync.install(model, state), (true, false));
    }

    #[test]
    fn an_old_registry_is_read_again_even_when_nothing_else_moved() {
        let mut sync = MenuSync::default();
        let t0 = Instant::now();
        sync.set_registry(Registry::default(), t0, 0, Vec::new());
        let (model, state) = pair(false);
        sync.install(model, state);
        assert!(sync.needs_registry(t0 + REGISTRY_MAX_AGE, &pair(false), 0, &[]));
    }

    #[test]
    fn a_registry_write_or_a_client_change_reads_the_registry_at_once() {
        let mut sync = MenuSync::default();
        let t0 = Instant::now();
        let key = AccountKey::primary();
        sync.set_registry(Registry::default(), t0, 4, vec![key.clone()]);
        let (model, state) = pair(false);
        sync.install(model, state);
        let soon = t0 + Duration::from_millis(100);
        assert!(!sync.needs_registry(soon, &pair(false), 4, std::slice::from_ref(&key)));
        // A sign-in, removal or rename wrote accounts.json: the model built
        // from the old registry is unchanged, but the registry is stale.
        assert!(sync.needs_registry(soon, &pair(false), 5, std::slice::from_ref(&key)));
        // A background client was added or removed.
        assert!(sync.needs_registry(soon, &pair(false), 4, &[]));
    }
}

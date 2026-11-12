//! The listed downloads a low or full disk paused: the files of this TDLib
//! instance, the messages they came from (saved in the account's data
//! preferences, so a later instance finds them again) and whether the saved
//! set was written since it last changed. One owner changes all three
//! together, so a pause or a resume cannot reach one and miss the others.
use std::collections::{BTreeSet, HashSet};

/// A message a pause is saved under: its chat and its id.
pub type PauseKey = (i64, i64);

/// Whether the saved pauses (`DataStoragePrefs::disk_paused`) match what
/// was last written to the preferences.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PauseSave {
    #[default]
    Written,
    /// The set changed; the next save writes it.
    Changed,
}

#[derive(Debug, Default)]
pub struct DiskPauses {
    files: HashSet<i32>,
    save: PauseSave,
}

impl DiskPauses {
    pub fn contains(&self, file_id: &i32) -> bool {
        self.files.contains(file_id)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The paused files, to resume them.
    pub fn ids(&self) -> Vec<i32> {
        self.files.iter().copied().collect()
    }

    /// The disk paused `file_id`, which came from `origin`: kept for this
    /// instance and saved by message. `false` when it was paused already.
    pub fn pause(
        &mut self,
        file_id: i32,
        origin: Option<PauseKey>,
        saved: &mut BTreeSet<PauseKey>,
    ) -> bool {
        if let Some(key) = origin
            && saved.insert(key)
        {
            self.save = PauseSave::Changed;
        }
        self.files.insert(file_id)
    }

    /// The disk's pause of `file_id` is undone (it resumed, or the user took
    /// the pause over).
    pub fn resume(
        &mut self,
        file_id: i32,
        origin: Option<PauseKey>,
        saved: &mut BTreeSet<PauseKey>,
    ) {
        self.files.remove(&file_id);
        if let Some(key) = origin
            && saved.remove(&key)
        {
            self.save = PauseSave::Changed;
        }
    }

    /// A download an earlier instance paused came back as `file_id` and the
    /// disk is low again: it is held, its message is saved already.
    pub fn hold(&mut self, file_id: i32) {
        self.files.insert(file_id);
    }

    /// Saved pauses were dropped without a resume (the search found them
    /// gone).
    pub fn saved_changed(&mut self) {
        self.save = PauseSave::Changed;
    }

    /// The saved set differs from what was last written.
    pub fn needs_save(&self) -> bool {
        self.save == PauseSave::Changed
    }

    /// The saved set was written.
    pub fn written(&mut self) {
        self.save = PauseSave::Written;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pause_and_its_resume_move_the_file_and_the_saved_message_together() {
        let mut pauses = DiskPauses::default();
        let mut saved = BTreeSet::new();
        assert!(pauses.pause(5, Some((7, 99)), &mut saved));
        assert!(pauses.contains(&5));
        assert!(saved.contains(&(7, 99)));
        assert!(pauses.needs_save());
        pauses.written();
        assert!(
            !pauses.pause(5, Some((7, 99)), &mut saved),
            "paused already"
        );
        assert!(!pauses.needs_save(), "nothing changed");
        pauses.resume(5, Some((7, 99)), &mut saved);
        assert!(pauses.is_empty());
        assert!(saved.is_empty());
        assert!(pauses.needs_save());
    }

    #[test]
    fn a_file_with_no_message_is_paused_for_this_instance_only() {
        let mut pauses = DiskPauses::default();
        let mut saved = BTreeSet::new();
        assert!(pauses.pause(6, None, &mut saved));
        assert!(saved.is_empty());
        assert!(!pauses.needs_save());
    }
}

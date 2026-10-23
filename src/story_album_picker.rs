//! The story album picker's selection (tdesktop `EditAlbumBox` over the
//! `StoryAlbumEdit` info wrap): the stories the user ticks, what that adds
//! to and removes from the album, and which stories can be offered.
#![deny(clippy::indexing_slicing)]

use std::collections::BTreeSet;

/// What the picker is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumPickerTarget {
    /// The stories of an album that exists.
    Album(i32),
    /// The stories a new album starts with.
    NewAlbum,
}

/// The stories ticked in the picker, against those the album had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumPicker {
    pub target: AlbumPickerTarget,
    initial: BTreeSet<i32>,
    selected: BTreeSet<i32>,
}

impl AlbumPicker {
    /// A picker whose ticks start as the stories the album already holds.
    pub fn new(target: AlbumPickerTarget, initial: impl IntoIterator<Item = i32>) -> Self {
        let initial: BTreeSet<i32> = initial.into_iter().collect();
        Self {
            target,
            selected: initial.clone(),
            initial,
        }
    }

    pub fn is_selected(&self, story_id: i32) -> bool {
        self.selected.contains(&story_id)
    }

    pub fn toggle(&mut self, story_id: i32) {
        if !self.selected.remove(&story_id) {
            self.selected.insert(story_id);
        }
    }

    pub fn count(&self) -> usize {
        self.selected.len()
    }

    /// Stories ticked now that the album did not have, newest first.
    pub fn added(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self.selected.difference(&self.initial).copied().collect();
        ids.reverse();
        ids
    }

    /// Stories the album had that are no longer ticked.
    pub fn removed(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self.initial.difference(&self.selected).copied().collect();
        ids.reverse();
        ids
    }

    /// Whether saving would change nothing.
    pub fn unchanged(&self) -> bool {
        self.initial == self.selected
    }

    /// The ticked stories, newest first (a new album's stories).
    pub fn selected_ids(&self) -> Vec<i32> {
        self.selected.iter().rev().copied().collect()
    }

    /// tdesktop's button: "Add Stories" while the edit only adds, "Save"
    /// when it removes too.
    pub fn only_adds(&self) -> bool {
        !self.added().is_empty() && self.removed().is_empty()
    }
}

/// The stories to offer, newest first: the archive and the chat page
/// together with the album's own, minus those the server says cannot be
/// added to albums (`can_be_added_to_album`) unless the album holds them.
pub fn candidate_ids(
    archived: &[i32],
    chat_page: &[i32],
    album: &[i32],
    can_add: impl Fn(i32) -> bool,
) -> Vec<i32> {
    let mut all: BTreeSet<i32> = BTreeSet::new();
    all.extend(archived.iter().copied());
    all.extend(chat_page.iter().copied());
    all.extend(album.iter().copied());
    all.into_iter()
        .rev()
        .filter(|id| album.contains(id) || can_add(*id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{AlbumPicker, AlbumPickerTarget, candidate_ids};

    #[test]
    fn toggling_tracks_added_and_removed() {
        let mut picker = AlbumPicker::new(AlbumPickerTarget::Album(3), [5, 7]);
        assert!(picker.unchanged());
        picker.toggle(9);
        picker.toggle(7);
        assert_eq!(picker.added(), vec![9]);
        assert_eq!(picker.removed(), vec![7]);
        assert_eq!(picker.count(), 2);
        assert!(!picker.only_adds());
        picker.toggle(7);
        assert!(picker.only_adds());
        picker.toggle(9);
        assert!(picker.unchanged());
    }

    #[test]
    fn a_new_album_starts_empty() {
        let mut picker = AlbumPicker::new(AlbumPickerTarget::NewAlbum, []);
        picker.toggle(2);
        picker.toggle(8);
        assert_eq!(picker.selected_ids(), vec![8, 2]);
    }

    #[test]
    fn candidates_merge_sources_and_hide_unaddable() {
        let ids = candidate_ids(&[4, 2], &[4, 6], &[1], |id| id != 6);
        assert_eq!(ids, vec![4, 2, 1]);
    }
}

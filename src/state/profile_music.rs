//! Managing the current user's saved music (tdesktop `Data::SavedMusic`):
//! which songs are in the profile (the "Save to..." menu hides "Profile"
//! for those), adding, removing and reordering tracks on the Playlist page.
//!
//! tdesktop changes its local list first and sends the request after
//! (`save`, `remove`, `reorder`), so the page answers at once. The same
//! happens here. A failed request marks the playlist stale, and the driver
//! loads it again, which puts the server order back.
use super::*;
use crate::telegram::envelope::ProfileAudio;

impl Session {
    /// Whether a song is in the current user's saved music, once known
    /// (`isProfileAudio`). `None` until the check has answered.
    pub fn profile_audio_known(&self, file_id: FileId) -> Option<bool> {
        self.media.profile_audio_ids.get(&file_id.0).copied()
    }

    /// The song's check has not been asked yet and none is in flight.
    pub fn needs_profile_audio_check(&self, file_id: FileId) -> bool {
        file_id.0 > 0
            && self.my_user_id.is_some()
            && !self.media.profile_audio_ids.contains_key(&file_id.0)
            && !self
                .requests
                .has_purpose(RequestPurpose::Media(MediaPurpose::IsProfileAudio {
                    file_id: file_id.0,
                }))
    }

    /// The current user's own playlist, when it is loaded.
    fn my_playlist_mut(&mut self) -> Option<&mut Playlist> {
        let me = self.my_user_id?;
        self.media.playlists.get_mut(&me)
    }

    /// `save`: the track goes to the front of the list, unless it is there.
    pub fn local_save_profile_audio(&mut self, audio: ProfileAudio) {
        self.media.profile_audio_ids.insert(audio.file_id.0, true);
        let Some(playlist) = self.my_playlist_mut() else {
            return;
        };
        if playlist.tracks.iter().any(|t| t.file_id == audio.file_id) {
            return;
        }
        playlist.tracks.insert(0, audio);
        playlist.total_count = playlist.total_count.max(0) + 1;
    }

    /// `remove`: the track leaves the list. Returns whether it was there.
    pub fn local_remove_profile_audio(&mut self, file_id: FileId) -> bool {
        self.media.profile_audio_ids.insert(file_id.0, false);
        let Some(playlist) = self.my_playlist_mut() else {
            return false;
        };
        let before = playlist.tracks.len();
        playlist.tracks.retain(|t| t.file_id != file_id);
        let removed = before - playlist.tracks.len();
        if removed > 0 {
            playlist.total_count = (playlist.total_count - removed as i32).max(0);
        }
        removed > 0
    }

    /// `reorder`: moves the track at `from` to `to` and returns the file id
    /// to send as `after_file_id` (0 for the front). `None` for positions
    /// that are out of range or equal, as in tdesktop.
    pub fn local_move_profile_audio(&mut self, from: usize, to: usize) -> Option<(i32, i32)> {
        let playlist = self.my_playlist_mut()?;
        let len = playlist.tracks.len();
        if from >= len || to >= len || from == to {
            return None;
        }
        let track = playlist.tracks.remove(from);
        let file_id = track.file_id.0;
        playlist.tracks.insert(to, track);
        let after = if to > 0 {
            playlist.tracks[to - 1].file_id.0
        } else {
            0
        };
        Some((file_id, after))
    }

    /// The answer to `isProfileAudio`; the other saved-music requests have
    /// nothing to apply on `Ok`.
    pub(crate) fn apply_profile_music_ok(&mut self, pending: Option<&PendingRequest>) {
        // The toasts for adding and removing show when the user acts, as in
        // tdesktop; only the check has a result to keep.
        if let Some(RequestPurpose::Media(MediaPurpose::IsProfileAudio { file_id })) =
            pending.map(|p| p.purpose)
        {
            self.media.profile_audio_ids.insert(file_id, true);
        }
    }

    /// The failures of the saved-music requests. `isProfileAudio` answers
    /// 404 for a song that is not saved; every other failure of a change
    /// makes the playlist load again.
    pub(crate) fn apply_profile_music_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) {
        match pending.map(|p| p.purpose) {
            Some(RequestPurpose::Media(MediaPurpose::IsProfileAudio { file_id })) => {
                if err.code == 404 {
                    self.media.profile_audio_ids.insert(file_id, false);
                }
            }
            Some(RequestPurpose::Media(MediaPurpose::AddProfileAudio { file_id })) => {
                self.local_remove_profile_audio(FileId(file_id));
                self.media.profile_audio_ids.remove(&file_id);
                self.mark_my_playlist_stale();
                self.messages.message_action_note = Some(format!(
                    "could not save to your profile: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::Media(
                MediaPurpose::RemoveProfileAudio { file_id }
                | MediaPurpose::SetProfileAudioPosition { file_id },
            )) => {
                self.media.profile_audio_ids.remove(&file_id);
                self.mark_my_playlist_stale();
                self.messages.message_action_note = Some(format!(
                    "could not change your playlist: {}",
                    error_reason(err)
                ));
            }
            _ => {}
        }
    }

    fn mark_my_playlist_stale(&mut self) {
        if let Some(playlist) = self.my_playlist_mut() {
            playlist.stale = true;
        }
    }

    /// The playlist ids the driver should load again after a failed change.
    pub fn take_stale_playlists(&mut self) -> Vec<i64> {
        self.media
            .playlists
            .iter_mut()
            .filter(|(_, p)| p.stale && !p.loading)
            .map(|(user, p)| {
                p.stale = false;
                p.loaded = false;
                *user
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i32) -> ProfileAudio {
        ProfileAudio {
            file_id: FileId(id),
            title: format!("t{id}"),
            performer: String::new(),
            file_name: String::new(),
            duration: 10,
        }
    }

    fn session_with(ids: &[i32]) -> Session {
        let mut session = Session::new(
            AccountKey::primary(),
            std::sync::Arc::new(crate::diagnostics::MemorySink::new()),
        );
        session.my_user_id = Some(7);
        let playlist = session.media.playlists.entry(7).or_default();
        playlist.tracks = ids.iter().map(|id| track(*id)).collect();
        playlist.total_count = ids.len() as i32;
        playlist.loaded = true;
        session
    }

    fn order(session: &Session) -> Vec<i32> {
        session.media.playlists[&7]
            .tracks
            .iter()
            .map(|t| t.file_id.0)
            .collect()
    }

    #[test]
    fn saving_puts_the_track_first_once() {
        let mut session = session_with(&[1, 2]);
        session.local_save_profile_audio(track(3));
        session.local_save_profile_audio(track(3));
        assert_eq!(order(&session), [3, 1, 2]);
        assert_eq!(session.media.playlists[&7].total_count, 3);
        assert_eq!(session.profile_audio_known(FileId(3)), Some(true));
    }

    #[test]
    fn removing_drops_the_track_and_the_count() {
        let mut session = session_with(&[1, 2, 3]);
        assert!(session.local_remove_profile_audio(FileId(2)));
        assert!(!session.local_remove_profile_audio(FileId(2)));
        assert_eq!(order(&session), [1, 3]);
        assert_eq!(session.media.playlists[&7].total_count, 2);
        assert_eq!(session.profile_audio_known(FileId(2)), Some(false));
    }

    #[test]
    fn moving_reports_the_track_before_it() {
        let mut session = session_with(&[1, 2, 3, 4]);
        // Down: 1 goes after 3.
        assert_eq!(session.local_move_profile_audio(0, 2), Some((1, 3)));
        assert_eq!(order(&session), [2, 3, 1, 4]);
        // To the front: after_file_id is 0.
        assert_eq!(session.local_move_profile_audio(3, 0), Some((4, 0)));
        assert_eq!(order(&session), [4, 2, 3, 1]);
        // Same or missing positions do nothing.
        assert_eq!(session.local_move_profile_audio(1, 1), None);
        assert_eq!(session.local_move_profile_audio(1, 9), None);
    }

    #[test]
    fn failed_changes_reload_the_playlist() {
        let mut session = session_with(&[1, 2]);
        session.local_remove_profile_audio(FileId(1));
        session.mark_my_playlist_stale();
        assert_eq!(session.take_stale_playlists(), vec![7]);
        assert!(!session.media.playlists[&7].loaded);
        assert!(session.take_stale_playlists().is_empty());
    }
}

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
        match self.media.profile_audio_ids.get(&file_id.0)? {
            ProfileAudioMark::Saved => Some(true),
            ProfileAudioMark::NotSaved => Some(false),
            ProfileAudioMark::Checking { .. } => None,
        }
    }

    /// The song's check has not been asked yet, or its ask is gone with no
    /// answer (a lost request is asked again).
    pub fn needs_profile_audio_check(&self, file_id: FileId) -> bool {
        file_id.0 > 0
            && self.my_user_id.is_some()
            && match self.media.profile_audio_ids.get(&file_id.0) {
                None => true,
                Some(ProfileAudioMark::Checking { request }) => {
                    self.requests.get(*request).is_none()
                }
                Some(ProfileAudioMark::Saved | ProfileAudioMark::NotSaved) => false,
            }
    }

    /// `isProfileAudio` for `file_id` went out as `request`.
    pub fn begin_profile_audio_check(&mut self, file_id: FileId, request: RequestId) {
        self.media
            .profile_audio_ids
            .insert(file_id.0, ProfileAudioMark::Checking { request });
    }

    /// The answer of `request` settles the song, if its check is still the
    /// one waiting.
    fn settle_profile_audio_check(&mut self, file_id: i32, request: RequestId, saved: bool) {
        if let Some(mark) = self.media.profile_audio_ids.get_mut(&file_id)
            && *mark == (ProfileAudioMark::Checking { request })
        {
            *mark = if saved {
                ProfileAudioMark::Saved
            } else {
                ProfileAudioMark::NotSaved
            };
        }
    }

    /// The current user's own playlist, when it is loaded.
    /// Where track `file_id` sits in your own playlist now.
    pub fn my_playlist_position(&self, file_id: FileId) -> Option<usize> {
        let me = self.my_user_id?;
        self.media
            .playlists
            .get(&me)?
            .tracks()
            .iter()
            .position(|track| track.file_id == file_id)
    }

    fn my_playlist_mut(&mut self) -> Option<&mut Playlist> {
        let me = self.my_user_id?;
        self.media.playlists.get_mut(&me)
    }

    /// `save`: the track goes to the front of the list, unless it is there.
    pub fn local_save_profile_audio(&mut self, audio: ProfileAudio) {
        self.media
            .profile_audio_ids
            .insert(audio.file_id.0, ProfileAudioMark::Saved);
        let Some(playlist) = self.my_playlist_mut() else {
            return;
        };
        let Some(data) = playlist.list.data_mut() else {
            return;
        };
        if data.tracks.iter().any(|t| t.file_id == audio.file_id) {
            return;
        }
        data.tracks.insert(0, audio);
        data.total_count = data.total_count.max(0) + 1;
    }

    /// `remove`: the track leaves the list. Returns whether it was there.
    pub fn local_remove_profile_audio(&mut self, file_id: FileId) -> bool {
        self.media
            .profile_audio_ids
            .insert(file_id.0, ProfileAudioMark::NotSaved);
        let Some(playlist) = self.my_playlist_mut() else {
            return false;
        };
        let Some(data) = playlist.list.data_mut() else {
            return false;
        };
        let before = data.tracks.len();
        data.tracks.retain(|t| t.file_id != file_id);
        let removed = before - data.tracks.len();
        if removed > 0 {
            data.total_count = (data.total_count - removed as i32).max(0);
        }
        removed > 0
    }

    /// `reorder`: moves the track at `from` to `to` and returns the file id
    /// to send as `after_file_id` (0 for the front). `None` for positions
    /// that are out of range or equal, as in tdesktop.
    pub fn local_move_profile_audio(&mut self, from: usize, to: usize) -> Option<(i32, i32)> {
        let data = self.my_playlist_mut()?.list.data_mut()?;
        let len = data.tracks.len();
        if from >= len || to >= len || from == to {
            return None;
        }
        let track = data.tracks.remove(from);
        let file_id = track.file_id.0;
        data.tracks.insert(to, track);
        let after = to
            .checked_sub(1)
            .and_then(|prev| data.tracks.get(prev))
            .map_or(0, |t| t.file_id.0);
        Some((file_id, after))
    }

    /// The answer to `isProfileAudio`; the other saved-music requests have
    /// nothing to apply on `Ok`.
    pub(crate) fn apply_profile_music_ok(&mut self, pending: Option<&PendingRequest>) {
        // The toasts for adding and removing show when the user acts, as in
        // tdesktop; only the check has a result to keep.
        if let Some((request, RequestPurpose::Media(MediaPurpose::IsProfileAudio { file_id }))) =
            pending.map(|p| (p.id, p.purpose))
        {
            // A local save or remove made after the check went out replaced
            // its mark and is newer than this answer.
            self.settle_profile_audio_check(file_id, request, true);
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
                let request = pending.map(|p| p.id);
                if let Some(request) = request {
                    if err.code == 404 {
                        self.settle_profile_audio_check(file_id, request, false);
                    } else if self.media.profile_audio_ids.get(&file_id)
                        == Some(&ProfileAudioMark::Checking { request })
                    {
                        // The check failed for another reason: unknown, and
                        // asked again.
                        self.media.profile_audio_ids.remove(&file_id);
                    }
                }
            }
            Some(RequestPurpose::Media(MediaPurpose::AddProfileAudio { file_id })) => {
                self.local_remove_profile_audio(FileId(file_id));
                self.playlist_change_unsent(file_id);
                self.messages.message_action_note.push(format!(
                    "could not save to your profile: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::Media(
                MediaPurpose::RemoveProfileAudio { file_id }
                | MediaPurpose::SetProfileAudioPosition { file_id },
            )) => {
                self.media.profile_audio_ids.remove(&file_id);
                self.playlist_change_unsent(file_id);
                self.messages.message_action_note.push(format!(
                    "could not change your playlist: {}",
                    error_reason(err)
                ));
            }
            _ => {}
        }
    }

    /// A remove or reorder of `file_id` did not happen (refused by the
    /// server, or never sent): the local change is undone by loading the
    /// list again.
    pub fn playlist_change_unsent(&mut self, file_id: i32) {
        self.media.profile_audio_ids.remove(&file_id);
        if let Some(playlist) = self.my_playlist_mut() {
            playlist.list.outdate();
        }
    }

    /// The playlist ids the driver should load again after a failed change.
    pub fn take_stale_playlists(&mut self) -> Vec<i64> {
        self.media
            .playlists
            .iter()
            .filter(|(_, p)| p.list.is_outdated())
            .map(|(user, _)| *user)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RequestId;

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
        playlist.set_ready(ids.iter().map(|id| track(*id)).collect());
        session
    }

    fn order(session: &Session) -> Vec<i32> {
        session.media.playlists[&7]
            .tracks()
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
        assert_eq!(session.media.playlists[&7].total_count(), 3);
        assert_eq!(session.profile_audio_known(FileId(3)), Some(true));
    }

    #[test]
    fn removing_drops_the_track_and_the_count() {
        let mut session = session_with(&[1, 2, 3]);
        assert!(session.local_remove_profile_audio(FileId(2)));
        assert!(!session.local_remove_profile_audio(FileId(2)));
        assert_eq!(order(&session), [1, 3]);
        assert_eq!(session.media.playlists[&7].total_count(), 2);
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
        session.playlist_change_unsent(1);
        assert_eq!(session.take_stale_playlists(), vec![7]);
        assert!(session.media.playlists[&7].list.is_outdated());
        // The reload going out leaves the outdated stage.
        session
            .media
            .playlists
            .get_mut(&7)
            .unwrap()
            .list
            .begin(RequestId(1), false);
        assert!(session.take_stale_playlists().is_empty());
    }

    #[test]
    fn a_late_check_answer_does_not_undo_a_local_save_or_remove() {
        let mut session = session_with(&[1]);
        let check = |session: &mut Session, file_id: i32| {
            let id = session.request(
                RequestPurpose::Media(MediaPurpose::IsProfileAudio { file_id }),
                None,
            );
            session.requests.take(id)
        };
        let saved = check(&mut session, 3);
        session.local_save_profile_audio(track(3));
        let not_found = TdError::from_code(404);
        session.apply_profile_music_error(&not_found, saved.as_ref());
        assert_eq!(session.profile_audio_known(FileId(3)), Some(true));

        let removed = check(&mut session, 1);
        session.local_remove_profile_audio(FileId(1));
        session.apply_profile_music_ok(removed.as_ref());
        assert_eq!(session.profile_audio_known(FileId(1)), Some(false));
    }

    #[test]
    fn a_check_is_settled_by_its_own_request_and_asked_again_when_it_fails_or_is_lost() {
        let mut session = session_with(&[1]);
        session.my_user_id = Some(7);
        let ask = |session: &mut Session| {
            let id = session.request(
                RequestPurpose::Media(MediaPurpose::IsProfileAudio { file_id: 5 }),
                None,
            );
            session.begin_profile_audio_check(FileId(5), id);
            id
        };
        let first = ask(&mut session);
        assert!(!session.needs_profile_audio_check(FileId(5)), "one is out");
        // A failure other than 404 leaves the song unknown and asks again.
        let pending = session.requests.take(first);
        session.apply_profile_music_error(&TdError::from_code(500), pending.as_ref());
        assert_eq!(session.profile_audio_known(FileId(5)), None);
        assert!(session.needs_profile_audio_check(FileId(5)));
        // An answer of an older request does not settle the newer check.
        let old = ask(&mut session);
        let new = ask(&mut session);
        let old = session.requests.take(old);
        session.apply_profile_music_ok(old.as_ref());
        assert_eq!(session.profile_audio_known(FileId(5)), None);
        let new = session.requests.take(new);
        session.apply_profile_music_ok(new.as_ref());
        assert_eq!(session.profile_audio_known(FileId(5)), Some(true));
        // A request lost with no answer is asked again.
        let lost = ask(&mut session);
        session.media.profile_audio_ids.remove(&6);
        session.requests.take(lost);
        assert!(session.needs_profile_audio_check(FileId(5)));
    }
}

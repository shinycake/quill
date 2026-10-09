//! B14 story viewer extras, kept GPUI-free so they are unit-testable:
//! video-story progress, press-and-hold / Space pause, the close-friends
//! editor selection, public story links and the share / save gates.
//!
//! tdesktop reference (`media/stories/media_stories_controller.cpp`,
//! `media_stories_view.cpp`): a press on the story pauses it until release
//! (`Controller::ignoreWindowMove` / `PauseState::Inactive`), Space toggles
//! pause, the mute button toggles `Controller::muted` for video sound, and
//! `Data::Story::url()` builds `https://t.me/<username>/s/<id>`.

use crate::telegram::envelope::ParsedStory;
use std::collections::BTreeSet;

/// Fraction of a video story shown in the progress segment: the player's
/// own position over the clip length.
pub fn video_progress(position_secs: f64, duration_secs: f64) -> f32 {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return 0.0;
    }
    (position_secs / duration_secs).clamp(0.0, 1.0) as f32
}

/// Whether a video story reached its end and should auto-advance: the
/// position is within a frame of the end, or the player stopped by itself
/// after having moved (a clip shorter than its declared duration). A paused
/// story never finishes.
pub fn video_finished(position_secs: f64, duration_secs: f64, playing: bool, paused: bool) -> bool {
    if paused || !duration_secs.is_finite() || duration_secs <= 0.0 {
        return false;
    }
    position_secs >= duration_secs - 0.05 || (!playing && position_secs > 0.0)
}

/// The viewer's user-driven pause: press-and-hold on the media and the
/// Space toggle are independent, either one pauses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StoryUserPause {
    held: bool,
    toggled: bool,
}

impl StoryUserPause {
    pub fn press(&mut self) {
        self.held = true;
    }

    pub fn release(&mut self) {
        self.held = false;
    }

    pub fn toggle(&mut self) {
        self.toggled = !self.toggled;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn is_paused(&self) -> bool {
        self.held || self.toggled
    }
}

/// Close-friends editor selection (tdesktop `EditCloseFriendsBox`): the
/// list loaded from `getCloseFriends` plus the user's toggles. Saving sends
/// the whole list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloseFriendsEdit {
    original: BTreeSet<i64>,
    selected: BTreeSet<i64>,
}

impl CloseFriendsEdit {
    pub fn new(current: &[i64]) -> Self {
        let set: BTreeSet<i64> = current.iter().copied().collect();
        Self {
            original: set.clone(),
            selected: set,
        }
    }

    pub fn toggle(&mut self, user_id: i64) {
        if !self.selected.remove(&user_id) {
            self.selected.insert(user_id);
        }
    }

    pub fn contains(&self, user_id: i64) -> bool {
        self.selected.contains(&user_id)
    }

    pub fn count(&self) -> usize {
        self.selected.len()
    }

    pub fn changed(&self) -> bool {
        self.original != self.selected
    }

    /// The ids to send, ascending.
    pub fn ids(&self) -> Vec<i64> {
        self.selected.iter().copied().collect()
    }
}

/// Public link of a story: `https://t.me/<username>/s/<id>`
/// (tdesktop `Data::Story::url`). `None` without a public username.
pub fn story_link(username: &str, story_id: i32) -> Option<String> {
    let username = username.trim().trim_start_matches('@');
    (!username.is_empty() && story_id > 0).then(|| format!("https://t.me/{username}/s/{story_id}"))
}

/// Share / repost-as-message is offered only for stories TDLib marks
/// forwardable (protected content and privacy-limited stories are not).
pub fn can_share_story(story: &ParsedStory) -> bool {
    story.can_be_forwarded
}

/// Saving the media to disk follows the same rule: tdesktop gates "Save
/// as…" on `canDownloadIfPremium` / protected content; TDLib folds both
/// into `can_be_forwarded`.
pub fn can_save_story(story: &ParsedStory) -> bool {
    story.can_be_forwarded
}

/// Label of the hide / unhide action (tdesktop "Hide stories" /
/// "Show stories").
pub fn hide_label(archived: bool) -> &'static str {
    if archived {
        "Unhide stories"
    } else {
        "Hide stories"
    }
}

/// Label of the profile toggle on own stories.
pub fn profile_label(posted: bool) -> &'static str {
    if posted {
        "Remove from profile"
    } else {
        "Post to profile"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_follows_position() {
        assert_eq!(video_progress(0.0, 10.0), 0.0);
        assert!((video_progress(2.5, 10.0) - 0.25).abs() < 1e-6);
        assert_eq!(video_progress(12.0, 10.0), 1.0);
        assert_eq!(video_progress(1.0, 0.0), 0.0);
        assert_eq!(video_progress(1.0, f64::NAN), 0.0);
    }

    #[test]
    fn finished_at_end_or_when_player_stops() {
        assert!(video_finished(9.97, 10.0, true, false));
        assert!(!video_finished(5.0, 10.0, true, false));
        // The player stopped early (short file).
        assert!(video_finished(4.0, 10.0, false, false));
        // Not started yet: not finished.
        assert!(!video_finished(0.0, 10.0, false, false));
        // Paused never finishes.
        assert!(!video_finished(10.0, 10.0, false, true));
    }

    #[test]
    fn hold_and_space_pause_independently() {
        let mut pause = StoryUserPause::default();
        assert!(!pause.is_paused());
        pause.press();
        assert!(pause.is_paused());
        pause.release();
        assert!(!pause.is_paused());
        pause.toggle();
        assert!(pause.is_paused());
        pause.press();
        pause.release();
        assert!(pause.is_paused(), "space pause survives a hold");
        pause.toggle();
        assert!(!pause.is_paused());
        pause.press();
        pause.reset();
        assert!(!pause.is_paused());
    }

    #[test]
    fn close_friends_edit_tracks_changes() {
        let mut edit = CloseFriendsEdit::new(&[3, 1]);
        assert!(!edit.changed());
        assert_eq!(edit.ids(), vec![1, 3]);
        edit.toggle(2);
        edit.toggle(1);
        assert_eq!(edit.ids(), vec![2, 3]);
        assert!(edit.changed());
        assert!(edit.contains(2) && !edit.contains(1));
        edit.toggle(2);
        edit.toggle(1);
        assert!(!edit.changed());
        assert_eq!(edit.count(), 2);
    }

    #[test]
    fn story_link_needs_a_username() {
        assert_eq!(
            story_link("@quill", 7).as_deref(),
            Some("https://t.me/quill/s/7")
        );
        assert_eq!(story_link(" ", 7), None);
        assert_eq!(story_link("quill", 0), None);
    }

    #[test]
    fn labels_flip() {
        assert_eq!(hide_label(false), "Hide stories");
        assert_eq!(hide_label(true), "Unhide stories");
        assert_eq!(profile_label(false), "Post to profile");
        assert_eq!(profile_label(true), "Remove from profile");
    }
}

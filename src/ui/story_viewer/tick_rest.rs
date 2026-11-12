//! One frame of the story viewer's clock, and when the clock rests.
//!
//! The viewer runs on the frame clock (`QuillApp::request_app_tick`), not on
//! a timer: a frame only comes while the window is shown. Each frame's
//! upkeep moves the story on; the upkeep asks for the next frame only while
//! something moves. A story that is held (Space, a panel over it, the window
//! behind another app) moves nothing, so it asks for nothing unless a load
//! or a download it waits on is really out. Then the clock stops; what
//! resumes the story asks again.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::state::CloseFriendsList;
use quill::story_extras::CloseFriendsStage;

/// The viewer's frames a second (`kPhotoProgressInterval`: 100 ms).
pub(in crate::ui) const STORY_TICK_FPS: u32 = 10;

impl QuillApp {
    /// One frame of the viewer, from the main window's upkeep: update the
    /// pause state, advance the story when its time is over, and ask for the
    /// next frame while something moves.
    pub(in crate::ui) fn story_viewer_upkeep(&mut self, cx: &mut Context<Self>) {
        if !self.stories.viewer.is_open() {
            self.stories.playback.stop();
            self.stories.emoji_warm = None;
            return;
        }
        let now = Instant::now();
        // B14: the native player drives video stories;
        // the duration clock stays frozen meanwhile.
        self.sync_story_video(cx);
        let user_paused = self.story_playback_paused();
        let video_driven = self.story_video_driven();
        self.stories
            .playback
            .set_paused(user_paused || video_driven, now);
        self.apply_story_native_pause(user_paused);
        let mut changed = self.tick_close_friends(cx);
        self.live_story_tick(cx);
        if self.story_segment_finished(now) {
            self.step_story_viewer(1, cx);
            changed = true;
        }
        // Phase 9.2+: keep custom-emoji reaction stickers
        // warm while the viewer is open (picker options +
        // the chosen-reaction badge), once per story and
        // per change of what they need.
        self.warm_story_custom_emoji();
        // A stealth toast is cleared when its time is over, which redraws.
        if let Some(open) = self.stories.viewer_ui_mut()
            && open
                .stealth_toast
                .as_ref()
                .is_some_and(|shown| shown.until <= now)
        {
            open.stealth_toast = None;
            changed = true;
        }
        if !self.stories.viewer.is_open() {
            self.stories.playback.stop();
            self.stories.emoji_warm = None;
            return;
        }
        // Only the overlay shows what a frame advances, and a paused one
        // (Space, a panel over it, the window behind another app) shows
        // nothing new: what pauses or resumes it redraws it itself. A live
        // story on screen keeps moving under a pause, and a toast on screen
        // is waiting for its end.
        let still = user_paused
            && self.live_story_shown().is_none()
            && self.stories.viewer_ui().stealth_toast.is_none();
        if changed || !still {
            self.notify_story_slice(cx);
        }
        if !self.story_tick_rests() {
            self.request_app_tick(STORY_TICK_FPS, cx);
        }
    }

    /// Whether the clock has nothing to do: the story is held and nothing it
    /// follows is really out.
    pub(super) fn story_tick_rests(&self) -> bool {
        self.story_playback_paused()
            && !self.story_live_moves()
            && !self.story_close_friends_out()
            && self.stories.viewer_ui().stealth_toast.is_none()
            && !self.story_custom_emoji_warming()
    }

    /// A live story is watched, or one is due to be joined (not one that was
    /// refused: only its "Join live" button asks again).
    fn story_live_moves(&self) -> bool {
        if self.stories.live.watching().is_some() {
            return true;
        }
        self.live_story_target()
            .is_some_and(|target| self.stories.live.refused() != Some(target))
    }

    /// The close-friends panel waits on a save, or on a load that is out.
    fn story_close_friends_out(&self) -> bool {
        match self.stories.viewer_ui().close_friends() {
            CloseFriendsStage::Saving { .. } => true,
            CloseFriendsStage::Awaiting => self.session().is_some_and(|session| {
                matches!(
                    session.stories.close_friends,
                    CloseFriendsList::Loading { .. }
                )
            }),
            _ => false,
        }
    }

    /// Whether a custom emoji the reaction picker or badge shows has its
    /// file on the way. A sticker the server does not know, or a file whose
    /// download is not running, is not waited on: the next redraw retries
    /// them (`warm_story_custom_emoji` keeps the backoff).
    fn story_custom_emoji_warming(&self) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        self.story_custom_emoji_fetch_ids().iter().any(|id| {
            session
                .stories
                .custom_emoji_stickers
                .get(id)
                .and_then(|sticker| sticker.display_file_id())
                .filter(|file| file.0 != 0)
                .is_some_and(|file| {
                    session.media.downloads.is_requested(file.0)
                        || session
                            .media
                            .files
                            .get(&file.0)
                            .is_some_and(|file| file.local.is_downloading_active())
                })
        })
    }
}

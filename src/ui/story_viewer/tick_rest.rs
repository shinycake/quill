//! One wake of the story viewer's 100 ms tick, and when the tick rests.
//!
//! A story that is held (Space, a panel over it, the window behind another
//! app) moves nothing, so its tick has nothing to do unless something it
//! polls for is still out. Then it ends instead of waking every 100 ms; what
//! resumes the story starts it again.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::story_extras::CloseFriendsStage;

/// What a wake of the tick found.
pub(super) enum Wake {
    /// The viewer is closed: the tick is over.
    Closed,
    /// The story is held and nothing is waited on: the tick ends.
    Rests,
    /// Wake again in 100 ms.
    Goes,
}

impl QuillApp {
    pub(super) fn story_tick_wake(&mut self, cx: &mut Context<Self>) -> Wake {
        if !self.stories.viewer.is_open() {
            return Wake::Closed;
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
        // Only the overlay shows what a tick advances, and a paused one
        // (Space, a panel over it, the window behind another app) shows
        // nothing new: what pauses or resumes it redraws it itself. A live
        // story keeps moving under a pause, and a toast on screen is
        // waiting for its end.
        let still = user_paused
            && self.stories.live.watching().is_none()
            && self.stories.viewer_ui().stealth_toast.is_none();
        if changed || !still {
            self.notify_story(cx);
        }
        if !self.stories.viewer.is_open() {
            Wake::Closed
        } else if self.story_tick_rests() {
            Wake::Rests
        } else {
            Wake::Goes
        }
    }

    /// Whether the tick has nothing to do: the story is held and nothing the
    /// tick polls for is out.
    pub(super) fn story_tick_rests(&self) -> bool {
        self.story_playback_paused()
            && self.stories.live.watching().is_none()
            && self.live_story_target().is_none()
            && !matches!(
                self.stories.viewer_ui().close_friends(),
                CloseFriendsStage::Awaiting | CloseFriendsStage::Saving { .. }
            )
            && self.stories.viewer_ui().stealth_toast.is_none()
            && !self.story_custom_emoji_warming()
    }

    /// A redraw of a held story whose tick has ended found something to
    /// wait on (the window became active, a panel closed): start it again.
    pub(in crate::ui) fn wake_story_tick_if_due(&mut self, cx: &mut Context<Self>) {
        if !self.stories.tick.is_running() && !self.story_tick_rests() {
            self.ensure_story_tick(cx);
        }
    }

    /// Whether a custom emoji the reaction picker or badge shows is still to
    /// be fetched or downloaded.
    fn story_custom_emoji_warming(&self) -> bool {
        let ids = self.story_custom_emoji_fetch_ids();
        let Some(session) = self.session() else {
            return false;
        };
        ids.iter().any(|id| {
            let file = session
                .stories
                .custom_emoji_stickers
                .get(id)
                .and_then(|sticker| sticker.display_file_id())
                .filter(|file| file.0 != 0);
            match file {
                None => true,
                Some(file) => session
                    .media
                    .files
                    .get(&file.0)
                    .is_none_or(|file| file.usable_path().is_none()),
            }
        })
    }
}

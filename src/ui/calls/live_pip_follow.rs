//! The live PiP follows its picture's feed (`video_player::live_follow`).
//! A rejoin or a restart gives a stream a new feed under the same key, and
//! a live story's presenter switch gives it a new one too. The stream
//! window and the story viewer already build a new player when the feed
//! changes. The PiP swaps its player the same way, inside the window it
//! has, so a rejoin no longer closes it. Design:
//! `docs/decisions/live-pip-feed-follow.md`.
#![deny(clippy::indexing_slicing)]

use super::live_pip::LivePipSource;
use crate::ui::app::QuillApp;
use crate::ui::video_player::{follow_live_feed, shows_feed};
use gpui_kit::*;
use quill::calls::live_video::LiveSink;

impl QuillApp {
    /// The feed the PiP's source plays now: the stream of the call's
    /// chat, or the watched live story's (its stream, or the presenter's
    /// camera or screen). `None` between a feed's end and the next.
    fn live_pip_sink(&self, source: LivePipSource) -> Option<LiveSink> {
        match source {
            LivePipSource::Stream { call_id } => {
                let call = self
                    .session()?
                    .calls
                    .active_group_call
                    .as_ref()
                    .filter(|call| call.id == call_id)?;
                let chat_id = self.group_call_chat(call)?.id.0;
                self.stream_sink(chat_id)
            }
            LivePipSource::Story { chat_id, story_id } => {
                if self.stories.live.watching != Some((chat_id, story_id)) {
                    return None;
                }
                self.live_story_sink()
            }
        }
    }

    /// Each sync: when the source's feed is not the one the PiP's player
    /// shows, a player on the new feed goes into the same window, the old
    /// one is released, and the new one is kept where enlarge hands it
    /// back from (the stream window's or the story viewer's slot), so the
    /// host shows that same player. With no feed yet (a rejoin's leave
    /// came before its connect) the window keeps the old picture and
    /// shows the connecting state.
    pub(super) fn follow_live_pip_feed(&mut self, cx: &mut App) {
        let Some(link) = self.group_call.live_pip.as_ref() else {
            return;
        };
        // Not made yet: it opens on the slot's player.
        let Some(window) = link.window.clone() else {
            return;
        };
        let source = link.source;
        let Some(sink) = self.live_pip_sink(source) else {
            return;
        };
        let Some(current) = window
            .view
            .upgrade()
            .and_then(|view| view.read(cx).player().cloned())
        else {
            // Closing: the check after this one drops the link.
            return;
        };
        if shows_feed(&current, &sink) {
            return;
        }
        let slot = match source {
            LivePipSource::Stream { .. } => &mut self.group_call.stream.player,
            LivePipSource::Story { .. } => &mut self.stories.live.player,
        };
        // The slot may follow the new feed already.
        let next = match follow_live_feed(slot.as_ref(), &sink, cx) {
            Some(player) => player,
            None => {
                let Some(player) = slot.clone() else {
                    return;
                };
                player
            }
        };
        let Some(old) = window.replace_player(next.clone(), cx) else {
            // The window went meanwhile: nothing was swapped. A player
            // built here goes again; the slot stays as it was.
            if !slot.as_ref().is_some_and(|player| player.same_as(&next)) {
                next.release_live(cx);
            }
            return;
        };
        let previous = slot.replace(next.clone());
        old.release_live(cx);
        if let Some(previous) =
            previous.filter(|player| !player.same_as(&old) && !player.same_as(&next))
        {
            previous.release_live(cx);
        }
    }
}

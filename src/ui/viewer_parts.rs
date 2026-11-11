//! The media viewer's staged state: the clip it plays, a photo turn being
//! decoded, the controls' visibility and the photo editor slot. Each is one
//! value replaced whole when its stage changes, so closing or stepping the
//! viewer cannot leave half of one behind.
#![deny(clippy::indexing_slicing)]

use super::video_player::VideoPlayer;
use super::viewer_state::DeferredOpen;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::media_viewer::ViewerOrientation;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// The clip of the item the viewer shows.
#[derive(Default)]
pub(super) enum ViewerClip {
    #[default]
    None,
    /// The clip of `item` plays (or is paused) in the viewer.
    Playing {
        item: MessageId,
        player: VideoPlayer,
        /// The media-session tick (media keys and the OS Now Playing
        /// widget, 250 ms) lives as long as the clip does.
        tick: Option<Task<()>>,
        /// The player failed after it opened: the line its controls show.
        error: Option<String>,
    },
    /// A `.gif` file the player can't decode (AVPlayer does not read GIF
    /// and the FFmpeg shim is missing): it animates as an image instead.
    Gif { item: MessageId, path: PathBuf },
    /// The clip could not be opened: the line its controls show. Each
    /// window's viewer has its own, so one clip's error is never drawn over
    /// another window's working clip.
    Failed { error: String },
}

impl ViewerClip {
    pub(super) fn player(&self) -> Option<&VideoPlayer> {
        match self {
            Self::Playing { player, .. } => Some(player),
            _ => None,
        }
    }

    /// The item whose clip plays or animates.
    pub(super) fn video(&self) -> Option<MessageId> {
        match self {
            Self::Playing { item, .. } | Self::Gif { item, .. } => Some(*item),
            Self::None | Self::Failed { .. } => None,
        }
    }

    pub(super) fn gif(&self) -> Option<&PathBuf> {
        match self {
            Self::Gif { path, .. } => Some(path),
            _ => None,
        }
    }

    pub(super) fn error(&self) -> Option<&String> {
        match self {
            Self::Playing { error, .. } => error.as_ref(),
            Self::Failed { error } => Some(error),
            Self::None | Self::Gif { .. } => None,
        }
    }

    /// The clip of `item` plays on `player`.
    pub(super) fn play(&mut self, item: MessageId, player: VideoPlayer) {
        *self = Self::Playing {
            item,
            player,
            tick: None,
            error: None,
        };
    }

    pub(super) fn animate(&mut self, item: MessageId, path: PathBuf) {
        *self = Self::Gif { item, path };
    }

    pub(super) fn fail(&mut self, error: String) {
        *self = Self::Failed { error };
    }

    /// The player failed while it played.
    pub(super) fn player_failed(&mut self, line: String) {
        if let Self::Playing { error, .. } = self {
            *error = Some(line);
        }
    }

    /// The error line goes (the item changed, something else plays).
    pub(super) fn clear_error(&mut self) {
        match self {
            Self::Playing { error, .. } => *error = None,
            Self::Failed { .. } => *self = Self::None,
            Self::None | Self::Gif { .. } => {}
        }
    }

    /// Full stop: the player and the tick go.
    pub(super) fn stop(&mut self) {
        *self = Self::None;
    }

    /// Hand the player out; the clip is over.
    pub(super) fn take_player(&mut self) -> Option<VideoPlayer> {
        match std::mem::take(self) {
            Self::Playing { player, .. } => Some(player),
            other => {
                *self = other;
                None
            }
        }
    }

    pub(super) fn has_tick(&self) -> bool {
        matches!(self, Self::Playing { tick: Some(_), .. })
    }

    pub(super) fn set_tick(&mut self, task: Task<()>) {
        if let Self::Playing { tick, .. } = self {
            *tick = Some(task);
        }
    }
}

/// A rotated or flipped decode of the viewer photo: its path and the
/// pixels, made for the orientation of the stage that holds it.
pub(super) type Rotated = (PathBuf, super::owned_image::OwnedImage);

/// The next rotate/flip decode token.
fn next_token() -> u64 {
    static TOKENS: AtomicU64 = AtomicU64::new(0);
    TOKENS.fetch_add(1, Ordering::Relaxed) + 1
}

/// The turn and flips of the viewer item, and the decode of the next one:
/// the orientation shown (with nothing decoded, as a video's turn is), a
/// decode of `target` running (the previous picture stays until it
/// lands), or the decoded picture. The shown orientation lives in every
/// stage, so it and its pixels are never compared by hand. Replaced whole
/// when the viewer moves on, so a late decode finds no matching token.
pub(super) enum Reorient {
    Idle(ViewerOrientation),
    Decoding {
        orientation: ViewerOrientation,
        target: ViewerOrientation,
        token: u64,
        shown: Option<Rotated>,
    },
    Ready {
        orientation: ViewerOrientation,
        rotated: Rotated,
    },
}

impl Default for Reorient {
    fn default() -> Self {
        Self::Idle(ViewerOrientation::default())
    }
}

impl Reorient {
    /// The orientation on screen.
    pub(super) fn orientation(&self) -> ViewerOrientation {
        match self {
            Self::Idle(orientation)
            | Self::Decoding { orientation, .. }
            | Self::Ready { orientation, .. } => *orientation,
        }
    }

    /// The picture to draw: the decoded one (or the one before it while a
    /// new turn decodes), made for `orientation()`.
    pub(super) fn shown(&self) -> Option<&Rotated> {
        match self {
            Self::Idle(_) => None,
            Self::Decoding { shown, .. } => shown.as_ref(),
            Self::Ready { rotated, .. } => Some(rotated),
        }
    }

    /// What queued presses build on: the running decode's target, else the
    /// orientation on screen.
    pub(super) fn target(&self) -> ViewerOrientation {
        match self {
            Self::Decoding { target, .. } => *target,
            Self::Idle(orientation) | Self::Ready { orientation, .. } => *orientation,
        }
    }

    /// Start decoding `target`; returns the token its answer must carry.
    pub(super) fn begin(&mut self, target: ViewerOrientation) -> u64 {
        let token = next_token();
        let orientation = self.orientation();
        let shown = std::mem::take(self).into_shown();
        *self = Self::Decoding {
            orientation,
            target,
            token,
            shown,
        };
        token
    }

    pub(super) fn is_decoding(&self, token: u64) -> bool {
        matches!(self, Self::Decoding { token: now, .. } if *now == token)
    }

    /// Drop a running decode, keeping the picture it would replace.
    pub(super) fn cancel(&mut self) {
        let orientation = self.orientation();
        *self = match std::mem::take(self).into_shown() {
            Some(rotated) => Self::Ready {
                orientation,
                rotated,
            },
            None => Self::Idle(orientation),
        };
    }

    /// The decode landed: `orientation` and its picture show together.
    pub(super) fn finish(&mut self, orientation: ViewerOrientation, rotated: Rotated) {
        *self = Self::Ready {
            orientation,
            rotated,
        };
    }

    /// Show `orientation` with nothing decoded and nothing decoding.
    pub(super) fn reset(&mut self, orientation: ViewerOrientation) {
        *self = Self::Idle(orientation);
    }

    fn into_shown(self) -> Option<Rotated> {
        match self {
            Self::Idle(_) => None,
            Self::Decoding { shown, .. } => shown,
            Self::Ready { rotated, .. } => Some(rotated),
        }
    }
}

/// Where the pointer is in the viewer: nowhere that matters, on a control
/// (they stay up), or holding the zoomed picture to pan it. One value, so
/// a step, a zoom reset or a close ends a pan without a hand reset of the
/// controls flag.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(super) enum Pointer {
    #[default]
    Away,
    OverControls,
    /// The last mouse position in px while the left button is held.
    Panning((f32, f32)),
}

impl Pointer {
    pub(super) fn anchor(self) -> Option<(f32, f32)> {
        match self {
            Self::Panning(at) => Some(at),
            Self::Away | Self::OverControls => None,
        }
    }

    pub(super) fn over_controls(self) -> bool {
        matches!(self, Self::OverControls)
    }

    /// The pointer moved onto a control or off them; a pan goes on (it
    /// ends with the button).
    pub(super) fn moved(&mut self, over_controls: bool) {
        if !matches!(self, Self::Panning(_)) {
            *self = if over_controls {
                Self::OverControls
            } else {
                Self::Away
            };
        }
    }

    /// The button went up, or the item changed: a pan ends, a pointer on a
    /// control stays there.
    pub(super) fn end_pan(&mut self) {
        if matches!(self, Self::Panning(_)) {
            *self = Self::Away;
        }
    }
}

/// The toolbar, arrows and caption: up (maybe with the idle timer
/// waiting), or faded out. `flips` counts the shows and hides and keys the
/// 150 ms fade.
pub(super) enum ControlsVisibility {
    Shown {
        flips: u64,
        idle_task: Option<Task<()>>,
    },
    Hidden {
        flips: u64,
        at: Instant,
    },
}

impl Default for ControlsVisibility {
    fn default() -> Self {
        Self::Shown {
            flips: 0,
            idle_task: None,
        }
    }
}

impl ControlsVisibility {
    pub(super) fn is_hidden(&self) -> bool {
        matches!(self, Self::Hidden { .. })
    }

    pub(super) fn flips(&self) -> u64 {
        match self {
            Self::Shown { flips, .. } | Self::Hidden { flips, .. } => *flips,
        }
    }

    /// When they faded out, if they are hidden.
    pub(super) fn hidden_at(&self) -> Option<Instant> {
        match self {
            Self::Hidden { at, .. } => Some(*at),
            Self::Shown { .. } => None,
        }
    }

    /// Show them; `true` when they were hidden.
    pub(super) fn show(&mut self) -> bool {
        match self {
            Self::Hidden { flips, .. } => {
                *self = Self::Shown {
                    flips: flips.wrapping_add(1),
                    idle_task: None,
                };
                true
            }
            Self::Shown { .. } => false,
        }
    }

    /// Fade them out (this drops the idle timer).
    pub(super) fn hide(&mut self, now: Instant) {
        let flips = self.flips().wrapping_add(1);
        *self = Self::Hidden { flips, at: now };
    }

    pub(super) fn has_timer(&self) -> bool {
        matches!(
            self,
            Self::Shown {
                idle_task: Some(_),
                ..
            }
        )
    }

    pub(super) fn set_timer(&mut self, task: Task<()>) {
        if let Self::Shown { idle_task, .. } = self {
            *idle_task = Some(task);
        }
    }

    /// The idle timer ended without hiding them.
    pub(super) fn timer_done(&mut self) {
        if let Self::Shown { idle_task, .. } = self {
            *idle_task = None;
        }
    }
}

/// The photo editor slot: closed, an open asked for while its photo still
/// renders, or the editor.
#[derive(Default)]
pub(super) enum PhotoEditorSlot {
    #[default]
    Closed,
    /// The editor opens on the photo of this render once it lands, unless
    /// the user left the chat view it was asked for in.
    Deferred(DeferredOpen),
    Open(Box<super::photo_editor::PhotoEditor>),
}

impl PhotoEditorSlot {
    pub(super) fn as_ref(&self) -> Option<&super::photo_editor::PhotoEditor> {
        match self {
            Self::Open(editor) => Some(editor),
            _ => None,
        }
    }

    pub(super) fn as_mut(&mut self) -> Option<&mut super::photo_editor::PhotoEditor> {
        match self {
            Self::Open(editor) => Some(editor),
            _ => None,
        }
    }

    pub(super) fn is_some(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    pub(super) fn is_none(&self) -> bool {
        !self.is_some()
    }

    /// Open `editor` (a deferred open it answers is over).
    pub(super) fn open(&mut self, editor: super::photo_editor::PhotoEditor) {
        *self = Self::Open(Box::new(editor));
    }

    /// Take the open editor out; a deferred open stays.
    pub(super) fn take(&mut self) -> Option<super::photo_editor::PhotoEditor> {
        match std::mem::take(self) {
            Self::Open(editor) => Some(*editor),
            other => {
                *self = other;
                None
            }
        }
    }

    /// Close the editor and drop any deferred open.
    pub(super) fn close(&mut self) {
        *self = Self::Closed;
    }

    /// Ask for the editor once its photo's render lands. An open editor
    /// stays (it is modal).
    pub(super) fn defer(&mut self, open: DeferredOpen) {
        if !self.is_some() {
            *self = Self::Deferred(open);
        }
    }

    #[cfg(test)]
    pub(super) fn deferred(&self) -> Option<DeferredOpen> {
        match self {
            Self::Deferred(open) => Some(*open),
            _ => None,
        }
    }

    /// Drop a deferred open (the editor stays).
    pub(super) fn clear_deferred(&mut self) {
        if matches!(self, Self::Deferred(_)) {
            *self = Self::Closed;
        }
    }

    /// Take the deferred open when it is for `render`.
    pub(super) fn take_deferred_for(&mut self, render: u64) -> Option<DeferredOpen> {
        match *self {
            Self::Deferred(open) if open.render == render => {
                *self = Self::Closed;
                Some(open)
            }
            _ => None,
        }
    }
}

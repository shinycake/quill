//! Play when ready: one press on media whose file is not local downloads
//! it and parks what to play; the poll loop starts it once the file lands.
//! A press while the file downloads cancels the download, as tdesktop's
//! `_cancell` does (the disc shows a cross then). One slot, since one thing
//! plays at a time: a newer press replaces the parked one.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::inline_players::Demand;
use gpui_kit::Context;
use quill::ids::{ChatId, FileId, MessageId};

/// What starts once the file is local.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum PlayTarget {
    Voice {
        chat: ChatId,
        message: MessageId,
        listened: bool,
        duration: f64,
    },
    Audio {
        chat: ChatId,
        message: MessageId,
        duration: f64,
    },
    /// An `image/gif` file, decoded in process.
    Gif { message: MessageId },
    /// A round video or MP4 GIF on the inline player, asked for in
    /// `window` (`None`: the main window).
    Inline {
        chat: ChatId,
        message: MessageId,
        demand: Demand,
        window: Option<ChatId>,
    },
    /// The media viewer's current clip.
    Viewer { message: MessageId },
}

impl PlayTarget {
    pub(super) const fn message(&self) -> MessageId {
        match self {
            Self::Voice { message, .. }
            | Self::Audio { message, .. }
            | Self::Gif { message }
            | Self::Inline { message, .. }
            | Self::Viewer { message } => *message,
        }
    }

    pub(super) const fn kind(&self) -> PlayKind {
        match self {
            Self::Voice { .. } => PlayKind::Voice,
            Self::Audio { .. } => PlayKind::Audio,
            Self::Gif { .. } => PlayKind::Gif,
            Self::Inline { .. } => PlayKind::Inline,
            Self::Viewer { .. } => PlayKind::Viewer,
        }
    }
}

/// The player a target starts: stopping a player drops its parked target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlayKind {
    Voice,
    Audio,
    Gif,
    Inline,
    Viewer,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PlayIntent {
    pub(super) file: FileId,
    pub(super) target: PlayTarget,
}

impl QuillApp {
    /// `file` is downloaded and readable.
    pub(super) fn file_is_local(&self, file: FileId) -> bool {
        self.session()
            .and_then(|session| session.file(file))
            .is_some_and(|file| file.usable_path().is_some())
    }

    /// `file` is downloading: what the media discs draw a cross for.
    fn file_is_fetching(&self, file: FileId) -> bool {
        self.session().is_some_and(|session| {
            super::message_media::file_is_downloading(
                file,
                &session.media.files,
                &session.media.downloads,
            )
        })
    }

    /// Download `file` (unless it already downloads) and play `target` once
    /// it lands.
    pub(super) fn play_when_ready(
        &mut self,
        file: FileId,
        target: PlayTarget,
        cx: &mut Context<Self>,
    ) {
        self.playback.intent = Some(PlayIntent { file, target });
        if !self.file_is_fetching(file) {
            self.request_media_download(file, None, cx);
        }
        cx.notify();
    }

    /// A press on a media control that cancels while `file` downloads (the
    /// disc shows a cross then) and otherwise runs `play`.
    pub(super) fn press_media(
        &mut self,
        file: FileId,
        cx: &mut Context<Self>,
        play: impl FnOnce(&mut Self, &mut Context<Self>),
    ) {
        if self.file_is_fetching(file) {
            self.cancel_media_download(file, cx);
            self.drop_play_intent_for(file);
            cx.notify();
            return;
        }
        play(self, cx);
    }

    /// Stopping a player drops the target parked for it.
    pub(super) fn drop_play_intent(&mut self, kind: PlayKind) {
        self.playback
            .intent
            .take_if(|intent| intent.target.kind() == kind);
    }

    /// Drop a target parked for `message` (its track stopped).
    pub(super) fn drop_play_intent_for_message(&mut self, message: MessageId) {
        self.playback
            .intent
            .take_if(|intent| intent.target.message() == message);
    }

    /// Drop the target waiting on `file` (its download failed or stopped).
    pub(super) fn drop_play_intent_for(&mut self, file: FileId) {
        self.playback.intent.take_if(|intent| intent.file == file);
    }

    /// Start the parked target once its file is local (the poll loop).
    pub(super) fn resume_play_intent(&mut self, cx: &mut Context<Self>) {
        let Some(intent) = self.playback.intent.clone() else {
            return;
        };
        if !self.file_is_local(intent.file) {
            return;
        }
        self.playback.intent = None;
        let file = intent.file;
        match intent.target {
            PlayTarget::Voice {
                chat,
                message,
                listened,
                duration,
            } => self.toggle_voice_playback(chat, message, file, listened, duration, cx),
            PlayTarget::Audio {
                chat,
                message,
                duration,
            } => self.toggle_audio_playback(chat, message, file, duration, cx),
            PlayTarget::Gif { message } => {
                self.toggle_animation_playback(message, file, "image/gif".into(), cx);
            }
            PlayTarget::Inline {
                chat,
                message,
                demand,
                window,
            } => self.play_inline_on_demand(window, chat, message, file, demand, cx),
            PlayTarget::Viewer { message } => {
                let current = self
                    .viewer
                    .state
                    .current()
                    .is_some_and(|item| item.message_id == message && item.plays_clip());
                if current {
                    self.maybe_autoplay_viewer_video(cx);
                }
            }
        }
    }
}

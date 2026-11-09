//! Play when ready: one press on media whose file is not local downloads
//! it and parks what to play; the poll loop starts it once the file lands.
//! A press while the file downloads cancels the download, as tdesktop's
//! `_cancell` does (the disc shows a cross then). One slot, since one thing
//! plays at a time: a newer press replaces the parked one.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_ui_state::ChatVisit;
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
    /// An `image/gif` file, decoded in process, asked for in `window`
    /// (`None`: the main window).
    Gif {
        message: MessageId,
        window: Option<ChatId>,
    },
    /// A round video or MP4 GIF on the inline player, asked for in
    /// `window` (`None`: the main window) during `visit` of its chat: it
    /// starts only while that window still shows the same visit.
    Inline {
        chat: ChatId,
        message: MessageId,
        demand: Demand,
        window: Option<ChatId>,
        visit: ChatVisit,
    },
    /// The media viewer's current clip, in `window`'s viewer (`None`: the
    /// main window's).
    Viewer {
        chat: ChatId,
        message: MessageId,
        window: Option<ChatId>,
    },
}

impl PlayTarget {
    pub(super) const fn message(&self) -> MessageId {
        match self {
            Self::Voice { message, .. }
            | Self::Audio { message, .. }
            | Self::Gif { message, .. }
            | Self::Inline { message, .. }
            | Self::Viewer { message, .. } => *message,
        }
    }

    /// The chat window the target was asked for in (`None`: the main
    /// window), for the targets that play in a window's own state.
    /// The chat of a parked voice note or song: the player bar's chat
    /// while the next track downloads, so its history stays whole and the
    /// playlist can go on (`sync_history_holds`).
    pub(super) const fn player_chat(&self) -> Option<ChatId> {
        match self {
            Self::Voice { chat, .. } | Self::Audio { chat, .. } => Some(*chat),
            Self::Gif { .. } | Self::Inline { .. } | Self::Viewer { .. } => None,
        }
    }

    pub(super) const fn window(&self) -> Option<ChatId> {
        match self {
            Self::Gif { window, .. }
            | Self::Inline { window, .. }
            | Self::Viewer { window, .. } => *window,
            Self::Voice { .. } | Self::Audio { .. } => None,
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

    /// The viewer of the window in place stopped: drop its parked clip
    /// only. Another window's viewer waits on its own download.
    pub(super) fn drop_viewer_play_intent_here(&mut self) {
        let here = self.windows.scope;
        self.playback.intent.take_if(|intent| {
            intent.target.kind() == PlayKind::Viewer && intent.target.window() == here
        });
    }

    /// The viewer of the window in place starts a clip now: drop what
    /// this window parked and a parked voice note or song (the clip
    /// stops those players). A clip parked in another window waits on
    /// its own download.
    pub(super) fn drop_play_intents_for_viewer_here(&mut self) {
        let here = self.windows.scope;
        self.playback.intent.take_if(|intent| {
            matches!(intent.target.kind(), PlayKind::Voice | PlayKind::Audio)
                || intent.target.window() == here
        });
    }

    /// Drop a target parked for `message` (its track stopped).
    pub(super) fn drop_play_intent_for_message(&mut self, message: MessageId) {
        self.playback
            .intent
            .take_if(|intent| intent.target.message() == message);
    }

    /// Drop a click to play parked in `chat`'s window, which closed: the
    /// request it would leave once its file lands names a window gone, so
    /// nothing would drop it, and the clip would start with sound wherever
    /// the chat shows next.
    pub(super) fn drop_play_intent_in_window(&mut self, chat: ChatId) {
        self.playback
            .intent
            .take_if(|intent| intent.target.window() == Some(chat));
    }

    /// A view's chat value was replaced: drop the round video or GIF
    /// parked during that visit (`ChatUi::replace_chat_ui`).
    pub(super) fn drop_play_intent_of_visit(&mut self, ended: ChatVisit) {
        self.playback.intent.take_if(
            |intent| matches!(intent.target, PlayTarget::Inline { visit, .. } if visit == ended),
        );
    }

    /// At the lock: a clip parked to play (a round video, a GIF, the
    /// viewer's clip) goes, as the lock stops every window's clips and
    /// closes the viewers. A voice note or song stays: tdesktop plays one
    /// from the click (it streams) and plays on while locked, and here it
    /// starts when its file lands, locked or not (`resume_play_intent`).
    pub(super) fn drop_play_intents_for_lock(&mut self) {
        self.playback
            .intent
            .take_if(|intent| intent.target.player_chat().is_none());
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
        // Only a voice note or song plays under the lock; the lock dropped
        // the clips (`drop_play_intents_for_lock`).
        if self.lock_waits() && intent.target.player_chat().is_none() {
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
            // A window closed meanwhile drops its target (`in_window_scope`).
            PlayTarget::Gif { message, window } => {
                self.in_window_scope(window, cx, |app, cx| {
                    app.toggle_animation_playback(message, file, "image/gif".into(), cx);
                });
            }
            PlayTarget::Inline {
                chat,
                message,
                demand,
                window,
                visit,
            } => {
                // Its window left the chat meanwhile (another chat, or Esc
                // and back), or closed. tdesktop drops the play request
                // with the chat's history widget.
                let same_visit = self
                    .in_window_scope(window, cx, |app, _| app.chat_ui.visit() == visit)
                    .unwrap_or(false);
                if same_visit {
                    self.play_inline_on_demand(window, chat, message, file, demand, cx);
                }
            }
            PlayTarget::Viewer {
                chat,
                message,
                window,
            } => {
                self.in_window_scope(window, cx, |app, cx| {
                    let current = app.viewer.state.current().is_some_and(|item| {
                        item.chat_id == chat && item.message_id == message && item.plays_clip()
                    });
                    if current {
                        app.maybe_autoplay_viewer_video(cx);
                    }
                });
            }
        }
    }
}

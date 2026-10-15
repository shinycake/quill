//! The viewer's side of Picture-in-Picture (tdesktop
//! `OverlayWidget::switchToPip` and the `closeAndContinue` it hands the
//! window): the viewer's player moves into the window and the viewer
//! closes; "back to the viewer" reopens the viewer on the same message
//! with the same player, where it was.
//!
//! The window goes when the clip's message is deleted, the app locks or
//! the account changes (tdesktop's `itemRemoved`, `passcodeLockChanges`,
//! `sessionChanges`), and when another clip with sound starts.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::video_player::VideoPlayer;
use crate::ui::video_player::controls::pip::{
    PipHooks, PipOpen, PipSource, PipWindow, open_pip_window,
};
use quill::ids::AccountKey;
use quill::lang::{keys, tr};
use std::rc::Rc;

/// An open Picture-in-Picture window.
pub(in crate::ui) struct PipLink {
    pub(in crate::ui) window: PipWindow,
    /// The account it was opened in.
    account: Option<AccountKey>,
    source: PipSource,
    /// The window the viewer was in, brought to the front on the way back.
    return_to: Option<AnyWindowHandle>,
}

impl QuillApp {
    /// The viewer's Picture-in-Picture button (and the Touch Bar's): hand
    /// the playing clip to a floating window and close the viewer.
    pub(in crate::ui) fn open_video_pip(&mut self, cx: &mut Context<Self>) {
        // The window takes the pointer and keys: a Space hold's release
        // never comes.
        self.cancel_viewer_boost(cx);
        // Only clips with the controller go (tdesktop `_streamed->controls`).
        if self.viewer.gif.is_some() || self.viewer_loops() {
            return;
        }
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        if self.viewer.video != Some(item.message_id) {
            return;
        }
        let Some(player) = self.viewer.player.take() else {
            return;
        };
        let source = PipSource {
            chat_id: item.chat_id,
            message_id: item.message_id,
            dimensions: item
                .natural_size
                .and_then(|(w, h)| Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?))),
        };
        self.close_video_pip(cx);
        self.close_media_viewer(cx);
        let last_volume = if self.playback.volume > 0.0 {
            self.playback.volume
        } else {
            self.playback.unmuted_volume
        };
        let account = self.session().map(|session| session.account.clone());
        let weak = cx.entity().downgrade();
        let hooks = pip_hooks(weak.clone());
        // Window creation draws at once: let this update finish first.
        cx.defer(move |cx| {
            let return_to = cx.active_window();
            let display = return_to.and_then(|handle| {
                handle
                    .update(cx, |_, window, cx| window.display(cx).map(|d| d.id()))
                    .ok()
                    .flatten()
            });
            let opened = open_pip_window(
                PipOpen {
                    player,
                    source,
                    hooks,
                    last_volume,
                    display,
                },
                cx,
            );
            let _ = weak.update(cx, |app, cx| {
                match opened {
                    Some(window) => {
                        app.viewer.pip = Some(PipLink {
                            window,
                            account,
                            source,
                            return_to,
                        })
                    }
                    None => {
                        app.connection.status_note = tr(keys::Q_MEDIA_PIP_FAILED).into();
                    }
                }
                cx.notify();
            });
        });
        cx.notify();
    }

    /// Close Picture-in-Picture: its player is dropped and the sound
    /// stops.
    pub(in crate::ui) fn close_video_pip(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.viewer.pip.take() else {
            return;
        };
        // Not from inside this update: the window's view may be the one
        // that asked.
        cx.defer(move |cx| {
            let view = link.window.view.clone();
            let _ = link.window.window.update(cx, |_, window, cx| {
                let _ = view.update(cx, |pip, cx| pip.close(window, cx));
            });
        });
    }

    /// Root render: the window closed itself, or its message, account or
    /// unlocked state is gone.
    pub(in crate::ui) fn check_video_pip(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.viewer.pip.as_ref() else {
            return;
        };
        // Closed: released, or let go of its player on the way out.
        let closed = link
            .window
            .view
            .upgrade()
            .is_none_or(|view| view.read(cx).player().is_none());
        if closed {
            self.viewer.pip = None;
            return;
        }
        let source = link.source;
        let account = self.session().map(|session| session.account.clone());
        let deleted = self
            .session()
            .and_then(|session| session.histories.get(&source.chat_id.0))
            .is_some_and(|history| !history.messages.contains_key(&source.message_id.0));
        if self.account.passcode.locked || account != link.account || deleted {
            self.close_video_pip(cx);
        }
    }

    /// The player Picture-in-Picture handed back for `message_id`, if the
    /// viewer is opening that clip (`continueStreaming`).
    pub(super) fn take_pip_handoff(&mut self, message_id: MessageId) -> Option<VideoPlayer> {
        self.viewer
            .handoff
            .take_if(|(id, _)| *id == message_id)
            .map(|(_, player)| player)
    }

    /// The viewer plays `player` (from Picture-in-Picture) as it is: no
    /// reopen, no seek; its play state, position, volume and rate stay.
    pub(super) fn adopt_viewer_player(
        &mut self,
        item: &MediaViewerItem,
        path: &std::path::Path,
        player: VideoPlayer,
        cx: &mut Context<Self>,
    ) {
        self.viewer.gif = None;
        self.viewer.video = Some(item.message_id);
        self.viewer.video_path = Some(path.to_path_buf());
        self.playback.error = None;
        let total = player.transport().total();
        let duration = if total > 0.0 {
            total
        } else {
            f64::from(item.duration_secs.unwrap_or(0).max(0))
        };
        player.clear_watchers();
        self.viewer.player = Some(player);
        self.create_viewer_sliders(duration, cx);
        self.spawn_viewer_session_tick(cx);
        cx.notify();
    }

    /// "Back to the viewer": the viewer opens on the clip's message with
    /// the window's player.
    fn pip_enlarged(&mut self, source: PipSource, player: VideoPlayer, cx: &mut Context<Self>) {
        let return_to = self.viewer.pip.take().and_then(|link| link.return_to);
        self.viewer.handoff = Some((source.message_id, player));
        self.open_media_viewer(source.chat_id, source.message_id, cx);
        // Not taken (the message is gone): the player stops here.
        self.viewer.handoff = None;
        cx.defer(move |cx| {
            cx.activate(true);
            if let Some(handle) = return_to {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            }
        });
        cx.notify();
    }

    /// The window's volume changed: it is the app's video volume too.
    fn pip_volume(&mut self, volume: f32) {
        if volume > 0.0 {
            self.playback.unmuted_volume = volume;
        }
        self.playback.volume = volume;
        self.playback.audio.set_volume(volume);
    }
}

/// What the window tells the app (each deferred by the window).
fn pip_hooks(app: WeakEntity<QuillApp>) -> PipHooks {
    let closed = app.clone();
    let enlarge = app.clone();
    PipHooks {
        closed: Rc::new(move |cx| {
            let _ = closed.update(cx, |app, cx| {
                // A link whose view is gone; a newer window keeps its own.
                app.check_video_pip(cx);
            });
        }),
        enlarge: Rc::new(move |source, player, cx| {
            let _ = enlarge.update(cx, |app, cx| app.pip_enlarged(source, player, cx));
        }),
        volume: Rc::new(move |volume, cx| {
            let _ = app.update(cx, |app, _| app.pip_volume(volume));
        }),
    }
}

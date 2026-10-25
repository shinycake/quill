//! The viewer's side of Picture-in-Picture (tdesktop
//! `OverlayWidget::switchToPip` and the `closeAndContinue` it hands the
//! window): the viewer's player moves into the window and the viewer
//! closes; "back to the viewer" reopens the viewer on the same clip with
//! the same player, where it was. Opening that clip again from the chat
//! does the same (tdesktop `continueStreaming`).
//!
//! The window goes when the clip's message is deleted, the app locks, the
//! account it belongs to logs out (tdesktop's `itemRemoved`,
//! `passcodeLockChanges`, `sessionChanges`), and when another sound
//! starts ([`QuillApp::sound_starts`]). Switching to another account
//! keeps it playing, as tdesktop keeps the other account's session.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::video_player::VideoPlayer;
use crate::ui::video_player::controls::pip::{PipHooks, PipOpen, PipWindow, open_pip_window};
use quill::ids::AccountKey;
use quill::lang::{keys, tr};
use quill::media_viewer::pip::reopen::{ClipRef, reopen_index};
use std::rc::Rc;

/// An open Picture-in-Picture window.
pub(in crate::ui) struct PipLink {
    pub(in crate::ui) window: PipWindow,
    /// The account it was opened in.
    account: Option<AccountKey>,
    /// The clip, and the viewer item it was opened from (the viewer
    /// reopens on that item alone when its message is no longer loaded).
    clip: ClipRef,
    item: MediaViewerItem,
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
        let clip = ClipRef {
            message_id: item.message_id,
            offset: self.viewer.state.current_offset(),
            file_id: item.play_file_id,
        };
        let dimensions = item
            .natural_size
            .and_then(|(w, h)| Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?)));
        self.close_video_pip(cx);
        // One sound at a time: a live PiP goes too.
        self.close_live_pip(cx);
        self.close_media_viewer(cx);
        let last_volume = if self.playback.volume > 0.0 {
            self.playback.volume
        } else {
            self.playback.unmuted_volume
        };
        let account = self.session().map(|session| session.account.clone());
        let weak = cx.entity().downgrade();
        let hooks = pip_hooks(weak.clone());
        // The link belongs to the viewer of the window that opened it.
        let scope = self.windows.scope;
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
                    dimensions,
                    hooks,
                    last_volume,
                    display,
                    live: None,
                },
                cx,
            );
            let _ = weak.update(cx, |app, cx| {
                let orphan = match opened {
                    Some(window) => {
                        let link = PipLink {
                            window,
                            account,
                            clip,
                            item,
                            return_to,
                        };
                        let mut link = Some(link);
                        app.in_window_scope(scope, cx, |app, _| app.viewer.pip = link.take());
                        link
                    }
                    None => {
                        app.connection
                            .set_note(StatusNote::error(tr(keys::Q_MEDIA_PIP_FAILED)));
                        None
                    }
                };
                // The window that opened it closed meanwhile: so does the
                // Picture-in-Picture window.
                if let Some(link) = orphan {
                    cx.defer(move |cx| {
                        let view = link.window.view.clone();
                        let _ = link.window.window.update(cx, |_, window, cx| {
                            let _ = view.update(cx, |pip, cx| pip.close(window, cx));
                        });
                    });
                }
                cx.notify();
            });
        });
        cx.notify();
    }

    /// Close Picture-in-Picture: its player is dropped and the sound
    /// stops. Runs without the main window drawing (it may be minimized
    /// or hidden to the tray).
    pub(in crate::ui) fn close_video_pip(&mut self, cx: &mut App) {
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

    /// Another sound starts (a viewer clip, a story, a voice message or
    /// music, a recording preview, a round video with sound): one sound
    /// plays at a time, so Picture-in-Picture closes (tdesktop
    /// `startStreamingPlayer` drops `_pip`).
    pub(in crate::ui) fn sound_starts(&mut self, cx: &mut App) {
        self.close_video_pip(cx);
        self.close_live_pip(cx);
    }

    /// A round video was clicked: when it now plays with sound, that is
    /// a sound starting.
    pub(in crate::ui) fn round_sound_started(&mut self, key: (i64, i64), cx: &mut App) {
        if self.playback.inline_videos.is_sounding(key) {
            self.sound_starts(cx);
        }
    }

    /// The account `account` logged out or was removed: its
    /// Picture-in-Picture goes with it (tdesktop `sessionChanges`).
    pub(in crate::ui) fn video_pip_account_ended(
        &mut self,
        account: &AccountKey,
        cx: &mut Context<Self>,
    ) {
        if self
            .viewer
            .pip
            .as_ref()
            .is_some_and(|link| link.account.as_ref() == Some(account))
        {
            self.close_video_pip(cx);
        }
        self.live_pip_account_ended(account, cx);
    }

    /// After updates land and at each root render: the window closed
    /// itself, its message was deleted or the app is locked. Only a real
    /// deletion counts (a tombstone), not the message leaving the loaded
    /// part of the history.
    pub(in crate::ui) fn check_video_pip(&mut self, cx: &mut Context<Self>) {
        // The live PiP's checks (`calls::live_pip`) run with the file's.
        self.check_live_pip(cx);
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
        let (chat, message) = (link.item.chat_id, link.clip.message_id);
        let deleted = self
            .session()
            .filter(|session| Some(&session.account) == link.account.as_ref())
            .and_then(|session| session.histories.get(&chat.0))
            .is_some_and(|history| history.is_tombstone(message));
        if self.account.passcode.locked || deleted {
            self.close_video_pip(cx);
        }
    }

    /// The viewer is opening `item`: when Picture-in-Picture plays that
    /// very clip, its player comes back to the viewer (no restart) and the
    /// window closes.
    pub(super) fn take_pip_player_for(
        &mut self,
        item: &MediaViewerItem,
        cx: &mut Context<Self>,
    ) -> Option<VideoPlayer> {
        let link = self.viewer.pip.as_ref()?;
        let account = self.session().map(|session| session.account.clone());
        if !link.clip.is(item) || link.item.chat_id != item.chat_id || account != link.account {
            return None;
        }
        let view = link.window.view.upgrade()?;
        let player = view.read(cx).player().cloned()?;
        let link = self.viewer.pip.take()?;
        cx.defer(move |cx| {
            let _ = link.window.window.update(cx, |_, window, cx| {
                view.update(cx, |pip, cx| pip.detach(window, cx));
            });
        });
        Some(player)
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

    /// "Back to the viewer": reopen the viewer on the window's clip, which
    /// takes the window's player (`take_pip_player_for`). The clip is found
    /// by its album position and file in the chat's loaded media; a
    /// message no longer loaded opens alone. When the viewer can't take the
    /// player (another account is active, the file is gone), the window
    /// stays.
    fn pip_enlarge_requested(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.viewer.pip.as_ref() else {
            return;
        };
        let account = self.session().map(|session| session.account.clone());
        if account != link.account {
            return;
        }
        let Some(player) = link
            .window
            .view
            .upgrade()
            .and_then(|view| view.read(cx).player().cloned())
        else {
            return;
        };
        let (clip, item, return_to) = (link.clip, link.item.clone(), link.return_to);
        let items = self
            .session()
            .and_then(|session| session.histories.get(&item.chat_id.0))
            .map(|history| {
                collect_media_items(&history.ordered().into_iter().cloned().collect::<Vec<_>>())
            })
            .unwrap_or_default();
        self.viewer.state = match reopen_index(&items, &clip) {
            Some(index) => MediaViewer::open(items, index),
            None => MediaViewer::open(vec![item], 0),
        };
        self.viewer.open_gen += 1;
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        let taken = self
            .viewer
            .player
            .as_ref()
            .is_some_and(|viewer| viewer.same_as(&player));
        if !taken {
            // Nothing reopened the clip: the window keeps playing it.
            self.close_media_viewer(cx);
            cx.notify();
            return;
        }
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
        enlarge: Rc::new(move |cx| {
            let _ = enlarge.update(cx, |app, cx| app.pip_enlarge_requested(cx));
        }),
        volume: Rc::new(move |volume, cx| {
            let _ = app.update(cx, |app, _| app.pip_volume(volume));
        }),
    }
}

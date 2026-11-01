//! The voice chat window's video area: one tile per camera or screen
//! stream, a pinned tile shown large, and a "paused" state when the
//! sender pauses (`groupCallParticipantVideoInfo.is_paused`, or your own
//! shared window hidden): the blurred last frame under the pause icon.
//! A stream with no frame yet shows the avatar, with the spinner after
//! 500 ms. Both states come from the live controller's overlay
//! (`video_player::controls::PictureOverlay`); the tiles keep their own
//! chrome (name, pin, full screen). The full-screen stage adds the
//! participant's volume (`group_call_stage_volume.rs`).
//!
//! Each stream is a live player over the driver's feed for
//! `(call, user, screen)`; a new frame redraws the window by itself, and
//! a tile scrolled out of sight stops asking.
//!
//! tdesktop: `calls/group/calls_group_viewport.cpp` (pin) and
//! `calls_group_viewport_raster.cpp` (paused tile).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::nested_click::SwallowPress;
use super::video_player::controls::{LiveInput, LiveOverlayParts, PictureOverlay, PictureState};
use super::video_player::{Purpose, VideoPlayer, video_frame};
use crate::ui::app_theme::call_chrome;
use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::live_video::{LiveKey, LiveState};
use quill::calls::tile_pin::{TileKey, stage_tile};
use quill::lang::{keys, tr, tr_with};
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};

struct Tile {
    key: TileKey,
    name: String,
    picture: AnyElement,
    /// The live overlay over the picture (paused, opening).
    overlay: Option<PictureState>,
    /// Your own tile (no volume on the stage).
    own: bool,
}

/// A tile's id for the overlay's elements.
fn tile_row(key: TileKey) -> u64 {
    let id = match key.participant {
        MessageSender::User { user_id } => user_id,
        MessageSender::Chat { chat_id } => chat_id,
    };
    id.unsigned_abs()
        .wrapping_mul(2)
        .wrapping_add(u64::from(key.screen))
}

impl QuillApp {
    /// The live player of one feed, made on first use; a new feed (a
    /// rejoin) gets a new player. `None` without a feed.
    pub(super) fn group_tile_player(&mut self, key: LiveKey, cx: &mut App) -> Option<VideoPlayer> {
        let sink = match self.live.as_ref() {
            Some(live) => live.driver.live_sink(key),
            None => self
                .demo_ui
                .group_feeds
                .get(&key)
                .map(|feed| feed.sink.clone()),
        }?;
        if let Some(player) = self.group_call.players.get(&key)
            && player.live_sink().is_some_and(|own| own.same_as(&sink))
        {
            return Some(player.clone());
        }
        let player = VideoPlayer::live(sink, Purpose::Live { tile: true }, cx);
        // Your camera reads as a mirror, as tdesktop's self tile does
        // (`VideoTile::mirror`); screens and others' cameras never flip.
        if matches!(key, LiveKey::Local { .. }) {
            player.set_live_mirrored(true);
        }
        if let Some(old) = self.group_call.players.insert(key, player.clone()) {
            old.release_live(cx);
        }
        Some(player)
    }

    /// Every stream of the call as a tile. `contain` fits video without
    /// cropping (the full-screen stage).
    fn collect_group_tiles(
        &mut self,
        call: &ActiveGroupCall,
        contain: bool,
        cx: &mut Context<Self>,
    ) -> Vec<Tile> {
        // Players of another call, or of streams that are gone, go.
        self.group_call.players.retain(|key, player| {
            let keep = key.call_id() == Some(call.id) && call_streams(call, *key);
            if !keep {
                player.release_live(cx);
            }
            keep
        });
        let players = &self.group_call.players;
        self.group_call
            .tile_overlays
            .retain(|key, _| players.contains_key(key));
        let mut built: Vec<Tile> = Vec::new();
        for participant in call.participants.iter() {
            let own_camera = participant.is_current_user && call.is_my_video_enabled;
            if !own_camera && !participant.video_enabled && !participant.screen_sharing_enabled {
                continue;
            }
            let name = self.group_call_participant_name(&participant.participant_id);
            if own_camera {
                let picture = self.group_self_tile_content(call, participant, &name, cx);
                built.push(Tile {
                    key: TileKey {
                        participant: participant.participant_id,
                        screen: false,
                    },
                    name: name.clone(),
                    picture,
                    overlay: call
                        .is_my_video_paused
                        .then_some(PictureState::LivePaused { wide: contain }),
                    own: true,
                });
            } else if participant.video_enabled {
                let paused = participant
                    .video_info
                    .as_ref()
                    .is_some_and(|info| info.is_paused);
                if let Some((picture, overlay)) =
                    self.group_stream_tile(call, participant, false, &name, paused, contain, cx)
                {
                    built.push(Tile {
                        key: TileKey {
                            participant: participant.participant_id,
                            screen: false,
                        },
                        name: name.clone(),
                        picture,
                        overlay,
                        own: participant.is_current_user,
                    });
                }
            }
            if participant.screen_sharing_enabled {
                // Your own hidden window shows paused at once, before the
                // participant update.
                let paused = participant
                    .screen_sharing_video_info
                    .as_ref()
                    .is_some_and(|info| info.is_paused)
                    || (participant.is_current_user && call.screen_share_paused);
                if let Some((picture, overlay)) =
                    self.group_stream_tile(call, participant, true, &name, paused, contain, cx)
                {
                    built.push(Tile {
                        key: TileKey {
                            participant: participant.participant_id,
                            screen: true,
                        },
                        name: tr_with(keys::Q_AV_SCREEN, &[("name", &name.to_string())]),
                        picture,
                        overlay,
                        own: participant.is_current_user,
                    });
                }
            }
        }
        built
    }

    /// The video area: the pinned tile large, the rest in a wrapped strip.
    fn retain_group_call_pin(&mut self, keys: &[TileKey]) {
        if self.group_call.pin.retain_available(keys) {
            self.sync_group_call_large_tile();
        }
    }

    pub(super) fn group_call_tiles(
        &mut self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let built = self.collect_group_tiles(call, false, cx);
        let keys: Vec<TileKey> = built.iter().map(|t| t.key).collect();
        // The pin lasts only as long as its stream does.
        self.retain_group_call_pin(&keys);
        if built.is_empty() {
            return None;
        }
        let layout = self.group_call.pin.layout(&keys);
        let mut main: Option<AnyElement> = None;
        let mut rest: Vec<AnyElement> = Vec::new();
        for tile in built {
            if layout.main == Some(tile.key) {
                main = Some(tile_view(tile, true, cx));
            } else {
                rest.push(tile_view(tile, false, cx));
            }
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .children(main)
                .when(!rest.is_empty(), |this| {
                    this.child(div().flex().flex_wrap().gap(px(6.)).children(rest))
                })
                .into_any_element(),
        )
    }

    /// The pinned stream across the whole window (full screen), or `None`
    /// when nothing is pinned or the stream is gone.
    pub(super) fn group_call_stage(
        &mut self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let built = self.collect_group_tiles(call, true, cx);
        let keys: Vec<TileKey> = built.iter().map(|t| t.key).collect();
        self.retain_group_call_pin(&keys);
        let stage = stage_tile(&self.group_call.pin, &keys)?;
        let tile = built.into_iter().find(|t| t.key == stage)?;
        let volume = (!tile.own)
            .then(|| self.stage_volume_slider(call, stage.participant, cx))
            .flatten();
        if volume.is_none() {
            self.group_call.stage_volume = None;
        }
        Some(stage_view(tile, volume, cx))
    }

    /// One stream's tile content: the live player once it has a frame
    /// (the last frame small and blurred while paused), else the avatar;
    /// and the shared live overlay over it (tdesktop's paused shadow, or
    /// the opening spinner after 500 ms). `None` without a feed unless
    /// paused (the avatar stands in, as before).
    #[allow(clippy::too_many_arguments)]
    fn group_stream_tile(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        screen: bool,
        name: &str,
        paused: bool,
        contain: bool,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, Option<PictureState>)> {
        let MessageSender::User { user_id } = participant.participant_id else {
            return None;
        };
        let key = LiveKey::Video {
            call_id: call.id,
            user_id,
            screen,
        };
        let player = self.group_tile_player(key, cx);
        // The pause comes with the participant update, not the engine: no
        // engine signal sets a track's state (only a stream's). One that
        // did would be masked here, as this sets it on every render.
        if let Some(sink) = player.as_ref().and_then(VideoPlayer::live_sink) {
            sink.set_state(if paused {
                LiveState::Paused
            } else {
                LiveState::Active
            });
        }
        let picture = match player.as_ref().filter(|player| player.has_live_frame()) {
            Some(player) => {
                let fit = if contain && !paused {
                    ObjectFit::Contain
                } else {
                    ObjectFit::Cover
                };
                let radius = px(if contain { 0. } else { 10. });
                video_frame(player)
                    .object_fit(fit)
                    .paused_overlay(true)
                    .corner_radii(Corners::all(radius), black())
                    .into_any_element()
            }
            None => initials_avatar(name, 56.).into_any_element(),
        };
        if paused {
            self.group_call.tile_overlays.remove(&key);
            return Some((picture, Some(PictureState::LivePaused { wide: contain })));
        }
        let player = player?;
        let input = LiveInput {
            state: Some(player.state()),
            idle: false,
            failed: false,
        };
        let now = cx.background_executor().now();
        let (overlay, due) = self
            .group_call
            .tile_overlays
            .entry(key)
            .or_default()
            .observe(input, now);
        if let Some(due) = due {
            let wait = due.saturating_duration_since(now);
            let timer = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            });
            if let Some(clock) = self.group_call.tile_overlays.get_mut(&key) {
                clock.arm(due, timer);
            }
        }
        // Tiles keep their own chrome and take only the opening and
        // paused states: a stalled tile keeps its last frame undimmed, as
        // tdesktop's viewport does.
        let state = PictureState::live(overlay, LiveOverlayParts::default())
            .filter(|state| matches!(state, PictureState::LiveOpening { .. }));
        Some((picture, state))
    }
}

/// Whether `key` is one of the call's streams: your camera, or a
/// participant's camera or screen that is on.
fn call_streams(call: &ActiveGroupCall, key: LiveKey) -> bool {
    match key {
        LiveKey::Local { .. } => call.is_my_video_enabled,
        LiveKey::Stream { .. } | LiveKey::Call { .. } => false,
        LiveKey::Video {
            user_id, screen, ..
        } => call.participants.iter().any(|participant| {
            participant.participant_id == MessageSender::User { user_id }
                && if screen {
                    participant.screen_sharing_enabled
                } else {
                    participant.video_enabled
                }
        }),
    }
}

fn tile_view(tile: Tile, large: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    let key = tile.key;
    let pinned = large;
    let label = SharedString::from(format!(
        "{} {}",
        if pinned {
            tr(keys::Q_AV_UNPIN)
        } else {
            tr(keys::CONTEXT_PIN_MSG)
        },
        tile.name
    ));
    let id = SharedString::from(format!("group-tile-{:?}-{}", key.participant, key.screen));
    div()
        .id(id)
        .relative()
        .when(large, |this| this.w_full().h(px(220.)))
        .when(!large, |this| this.w(relative(0.49)).h(px(120.)))
        .overflow_hidden()
        .rounded(px(10.))
        .bg(black())
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(label)
        .on_click(cx.listener(move |this, _, _, cx| this.toggle_group_call_tile_pin(key, cx)))
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(tile.picture),
        )
        .children(
            tile.overlay
                .map(|state| PictureOverlay::new(tile_row(key), state)),
        )
        .child(
            div()
                .absolute()
                .left(px(8.))
                .bottom(px(6.))
                .max_w(relative(0.8))
                .truncate()
                .text_size(px(12.))
                .text_color(call_chrome::caption())
                .child(tile.name),
        )
        .when(pinned, |this| {
            this.child(
                div()
                    .id("group-tile-fullscreen")
                    .absolute()
                    .right(px(36.))
                    .top(px(6.))
                    .size(px(24.))
                    .rounded_full()
                    .bg(call_chrome::chip())
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(white())
                    .role(Role::Button)
                    .aria_label(tr(keys::Q_AV_FULL_SCREEN))
                    .tab_index(0)
                    .cursor_pointer()
                    .tooltip(|window, cx| {
                        Tooltip::new(tr(keys::Q_AV_FULL_SCREEN)).build(window, cx)
                    })
                    .swallow_press()
                    .on_click(
                        cx.listener(|this, _, window, cx| this.enter_group_call_stage(window, cx)),
                    )
                    .child(Icon::new(IconName::Maximize).with_size(px(14.))),
            )
        })
        .child(
            div()
                .absolute()
                .right(px(6.))
                .top(px(6.))
                .size(px(24.))
                .rounded_full()
                .bg(call_chrome::chip())
                .flex()
                .items_center()
                .justify_center()
                .text_color(white())
                .child(
                    Icon::new(if pinned {
                        IconName::PinOff
                    } else {
                        IconName::Pin
                    })
                    .with_size(px(14.)),
                ),
        )
        .into_any_element()
}

/// One round overlay button on the stage.
fn stage_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    cx: &mut Context<QuillApp>,
    on_click: impl Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(34.))
        .rounded_full()
        .bg(call_chrome::chip())
        .hover(|style| style.bg(call_chrome::chip_over()))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(white())
        .role(Role::Button)
        .aria_label(label)
        .tab_index(0)
        .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
        .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
        .child(Icon::new(icon).with_size(px(18.)))
}

/// The full-screen stage: the pinned video on black, name bottom-left,
/// unpin and leave-full-screen top-right (Esc also leaves).
fn stage_view(tile: Tile, volume: Option<AnyElement>, cx: &mut Context<QuillApp>) -> AnyElement {
    let key = tile.key;
    div()
        .id("group-call-stage")
        .debug_selector(|| "group-call-stage".into())
        .relative()
        .size_full()
        .bg(black())
        .overflow_hidden()
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(tile.picture),
        )
        .children(
            tile.overlay
                .map(|state| PictureOverlay::new(tile_row(key), state)),
        )
        .child(
            div()
                .absolute()
                .left(px(16.))
                .bottom(px(14.))
                .max_w(relative(0.7))
                .truncate()
                .text_size(px(14.))
                .text_color(call_chrome::caption())
                .child(tile.name),
        )
        .child(
            div()
                .absolute()
                .right(px(14.))
                .top(px(14.))
                .flex()
                .items_center()
                .gap(px(8.))
                .children(volume)
                .child(stage_button(
                    "group-stage-unpin",
                    tr(keys::Q_AV_UNPIN),
                    IconName::PinOff,
                    cx,
                    move |this, _, cx| this.toggle_group_call_tile_pin(key, cx),
                ))
                .child(stage_button(
                    "group-stage-exit",
                    tr(keys::Q_AV_EXIT_FULL_SCREEN),
                    IconName::Minimize,
                    cx,
                    |this, window, cx| this.leave_group_call_stage(window, cx),
                )),
        )
        .into_any_element()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_call_tiles_tests.rs"]
mod tests;

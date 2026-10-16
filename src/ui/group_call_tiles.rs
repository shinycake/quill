//! The voice chat window's video area: one tile per camera or screen
//! stream, a pinned tile shown large, and a "paused" state when the
//! sender pauses (`groupCallParticipantVideoInfo.is_paused`, or your own
//! shared window hidden): the blurred last frame under the pause icon
//! (`group_call_paused.rs`).
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
use super::group_call_paused::paused_overlay;
use super::nested_click::SwallowPress;
use super::video_player::{Purpose, VideoPlayer, video_frame};
use gpui_kit::assets::IconName;
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
    paused: bool,
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
        self.group_call.players.insert(key, player.clone());
        Some(player)
    }

    /// One stream's picture: the live player once it has a frame (the
    /// last frame small and blurred while paused), or `None`.
    fn group_stream_picture(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        screen: bool,
        paused: bool,
        contain: bool,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let MessageSender::User { user_id } = participant.participant_id else {
            return None;
        };
        let key = LiveKey::Video {
            call_id: call.id,
            user_id,
            screen,
        };
        let player = self.group_tile_player(key, cx)?;
        // The pause comes with the participant update, not the engine.
        if let Some(sink) = player.live_sink() {
            sink.set_state(if paused {
                LiveState::Paused
            } else {
                LiveState::Active
            });
        }
        if !player.has_live_frame() {
            return None;
        }
        let fit = if contain && !paused {
            ObjectFit::Contain
        } else {
            ObjectFit::Cover
        };
        let radius = px(if contain { 0. } else { 10. });
        Some(
            video_frame(&player)
                .object_fit(fit)
                .paused_overlay(true)
                .corner_radii(Corners::all(radius), black())
                .into_any_element(),
        )
    }

    /// Every stream of the call as a tile. `contain` fits video without
    /// cropping (the full-screen stage).
    fn collect_group_tiles(
        &mut self,
        call: &ActiveGroupCall,
        contain: bool,
        cx: &mut App,
    ) -> Vec<Tile> {
        // Players of another call, or of streams that are gone, go.
        self.group_call
            .players
            .retain(|key, _| key.call_id() == Some(call.id) && call_streams(call, *key));
        let mut built: Vec<Tile> = Vec::new();
        for participant in &call.participants {
            let name = self.group_call_participant_name(&participant.participant_id);
            if participant.is_current_user && call.is_my_video_enabled {
                let picture = self.group_self_tile_content(call, participant, &name, cx);
                built.push(Tile {
                    key: TileKey {
                        participant: participant.participant_id,
                        screen: false,
                    },
                    name: name.clone(),
                    picture,
                    paused: call.is_my_video_paused,
                });
            } else if participant.video_enabled {
                let paused = participant
                    .video_info
                    .as_ref()
                    .is_some_and(|info| info.is_paused);
                if let Some(picture) =
                    self.stream_or_placeholder(call, participant, false, &name, paused, contain, cx)
                {
                    built.push(Tile {
                        key: TileKey {
                            participant: participant.participant_id,
                            screen: false,
                        },
                        name: name.clone(),
                        picture,
                        paused,
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
                if let Some(picture) =
                    self.stream_or_placeholder(call, participant, true, &name, paused, contain, cx)
                {
                    built.push(Tile {
                        key: TileKey {
                            participant: participant.participant_id,
                            screen: true,
                        },
                        name: tr_with(keys::Q_AV_SCREEN, &[("name", &name.to_string())]),
                        picture,
                        paused,
                    });
                }
            }
        }
        built
    }

    /// The video area: the pinned tile large, the rest in a wrapped strip.
    pub(super) fn group_call_tiles(
        &mut self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let built = self.collect_group_tiles(call, false, cx);
        let keys: Vec<TileKey> = built.iter().map(|t| t.key).collect();
        // The pin lasts only as long as its stream does.
        self.group_call.pin.retain_available(&keys);
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
        self.group_call.pin.retain_available(&keys);
        let stage = stage_tile(&self.group_call.pin, &keys)?;
        let tile = built.into_iter().find(|t| t.key == stage)?;
        Some(stage_view(tile, cx))
    }

    /// The live picture, or (only while paused) the avatar standing in.
    #[allow(clippy::too_many_arguments)]
    fn stream_or_placeholder(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        screen: bool,
        name: &str,
        paused: bool,
        contain: bool,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.group_stream_picture(call, participant, screen, paused, contain, cx)
            .or_else(|| paused.then(|| initials_avatar(name, 56.).into_any_element()))
    }
}

/// Whether `key` is one of the call's streams: your camera, or a
/// participant's camera or screen that is on.
fn call_streams(call: &ActiveGroupCall, key: LiveKey) -> bool {
    match key {
        LiveKey::Local { .. } => call.is_my_video_enabled,
        LiveKey::Stream { .. } => false,
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
        .bg(rgb(0x000000))
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
        .when(tile.paused, |this| this.child(paused_overlay(false)))
        .child(
            div()
                .absolute()
                .left(px(8.))
                .bottom(px(6.))
                .max_w(relative(0.8))
                .truncate()
                .text_size(px(12.))
                .text_color(rgba(0xffffffe0))
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
                    .bg(rgba(0x00000099))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(white())
                    .role(Role::Button)
                    .aria_label(tr(keys::Q_AV_FULL_SCREEN))
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
                .bg(rgba(0x00000099))
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
        .bg(rgba(0x00000099))
        .hover(|style| style.bg(rgba(0x000000cc)))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(white())
        .role(Role::Button)
        .aria_label(label)
        .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
        .child(Icon::new(icon).with_size(px(18.)))
}

/// The full-screen stage: the pinned video on black, name bottom-left,
/// unpin and leave-full-screen top-right (Esc also leaves).
fn stage_view(tile: Tile, cx: &mut Context<QuillApp>) -> AnyElement {
    let key = tile.key;
    div()
        .id("group-call-stage")
        .relative()
        .size_full()
        .bg(rgb(0x000000))
        .overflow_hidden()
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(tile.picture),
        )
        .when(tile.paused, |this| this.child(paused_overlay(true)))
        .child(
            div()
                .absolute()
                .left(px(16.))
                .bottom(px(14.))
                .max_w(relative(0.7))
                .truncate()
                .text_size(px(14.))
                .text_color(rgba(0xffffffe0))
                .child(tile.name),
        )
        .child(
            div()
                .absolute()
                .right(px(14.))
                .top(px(14.))
                .flex()
                .gap(px(8.))
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

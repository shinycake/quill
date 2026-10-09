//! The voice chat window's video area: one tile per camera or screen
//! stream, a pinned tile shown large, and a "paused" state when the
//! sender pauses (`groupCallParticipantVideoInfo.is_paused`).
//!
//! tdesktop: `calls/group/calls_group_viewport.cpp` (pin) and
//! `calls_group_viewport_tile.cpp` (paused overlay).

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::tile_pin::TileKey;
use quill::state::ActiveGroupCall;
use quill::telegram::envelope::{MessageSender, ParsedGroupCallParticipant};

struct Tile {
    key: TileKey,
    name: String,
    picture: AnyElement,
    paused: bool,
}

impl QuillApp {
    /// One stream's picture: the latest decoded frame, or `None`.
    fn group_stream_picture(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        screen: bool,
    ) -> Option<AnyElement> {
        let MessageSender::User { user_id } = participant.participant_id else {
            return None;
        };
        let frame = if let Some(live) = self.live.as_ref() {
            live.driver
                .latest_group_video_frame(call.id, user_id, screen)
        } else {
            self.demo_group_frames.get(&(user_id, screen)).cloned()
        }?;
        let image = self.cached_group_video_image(call.id, user_id, screen, &frame)?;
        Some(
            img(ImageSource::Render(image))
                .size_full()
                .rounded(px(10.))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
        )
    }

    /// The video area: the pinned tile large, the rest in a wrapped strip.
    pub(super) fn group_call_tiles(
        &mut self,
        call: &ActiveGroupCall,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut built: Vec<Tile> = Vec::new();
        for participant in &call.participants {
            let name = self.group_call_participant_name(&participant.participant_id);
            if participant.is_current_user && call.is_my_video_enabled {
                let picture = self.group_self_tile_content(call, participant, &name);
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
                    self.stream_or_placeholder(call, participant, false, &name, paused)
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
                let paused = participant
                    .screen_sharing_video_info
                    .as_ref()
                    .is_some_and(|info| info.is_paused);
                if let Some(picture) =
                    self.stream_or_placeholder(call, participant, true, &name, paused)
                {
                    built.push(Tile {
                        key: TileKey {
                            participant: participant.participant_id,
                            screen: true,
                        },
                        name: format!("{name} · screen"),
                        picture,
                        paused,
                    });
                }
            }
        }
        let keys: Vec<TileKey> = built.iter().map(|t| t.key).collect();
        // The pin lasts only as long as its stream does.
        self.group_call_pin.retain_available(&keys);
        if built.is_empty() {
            return None;
        }
        let layout = self.group_call_pin.layout(&keys);
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

    /// The decoded frame, or (only while paused) the avatar standing in.
    fn stream_or_placeholder(
        &mut self,
        call: &ActiveGroupCall,
        participant: &ParsedGroupCallParticipant,
        screen: bool,
        name: &str,
        paused: bool,
    ) -> Option<AnyElement> {
        self.group_stream_picture(call, participant, screen)
            .or_else(|| paused.then(|| initials_avatar(name, 56.).into_any_element()))
    }
}

fn tile_view(tile: Tile, large: bool, cx: &mut Context<QuillApp>) -> AnyElement {
    let key = tile.key;
    let pinned = large;
    let label = SharedString::from(format!(
        "{} {}",
        if pinned { "Unpin" } else { "Pin" },
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
        .when(tile.paused, |this| {
            this.child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(rgba(0x000000a0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(4.))
                    .text_color(white())
                    .child(Icon::new(IconName::Pause).with_size(px(22.)))
                    .child(div().text_size(px(13.)).child(if key.screen {
                        "Screen sharing paused"
                    } else {
                        "Camera paused"
                    })),
            )
        })
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

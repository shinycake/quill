//! The "now playing" bar and the playlist behind it.
//!
//! Telegram Desktop shows a bar above the chat while a voice note or a music
//! file plays (`media/player/media_player_widget.cpp`): play/pause,
//! previous/next, title and performer, progress, speed, volume, repeat and
//! order, close. It stays when the user opens another chat, a click on the
//! title jumps to the message, and the playlist
//! (`media_player_instance.cpp`) continues on its own: a voice note goes on
//! to the next unread voice note, music walks the chat's audio files by the
//! repeat / order settings. The OS media keys and the Now Playing widget
//! drive the same controls (`quill::media_session`).
//!
//! The bar keeps to this module and is added to the conversation column by
//! one call, above the call bar and the chat's own bars.

use super::app::QuillApp;
use super::playback::PlaybackKind;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::media_session::{self, Command, NowPlaying};
use quill::playback::PlaybackClock;
use quill::playlist::{self, Move, OrderMode, RepeatMode, ShuffleState, Step, VoiceEntry};
use quill::telegram::envelope::MessageContent;
use quill::voice::format_voice_duration;

/// What the bar shows about the active track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TrackMeta {
    pub(super) title: String,
    pub(super) subtitle: String,
}

/// Player-bar state that outlives a chat switch.
#[derive(Default)]
pub(super) struct PlayerBarState {
    /// Chat the active track belongs to (may differ from the open chat).
    pub(super) chat: Option<ChatId>,
    pub(super) meta: Option<TrackMeta>,
    /// The bar's own progress slider (the row has another one).
    pub(super) slider: Option<Entity<SliderState>>,
    pub(super) repeat: RepeatMode,
    pub(super) order: OrderMode,
    pub(super) shuffle: ShuffleState,
}

/// Cheap pseudo-random index for shuffle (no `rand` dependency): the clock's
/// sub-second part, which is plenty for picking the next song.
fn random_below(n: usize) -> usize {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize);
    nanos.wrapping_mul(2_654_435_761) % n.max(1)
}

impl QuillApp {
    /// Title and subtitle for a message, from the chat history.
    fn track_meta_for(&self, chat: ChatId, id: MessageId, kind: PlaybackKind) -> TrackMeta {
        let message = self.session().and_then(|session| {
            session
                .histories
                .get(&chat.0)
                .and_then(|h| h.ordered().into_iter().find(|m| m.id == id))
                .map(|m| (session, m))
        });
        match (kind, message) {
            (PlaybackKind::Audio, Some((_, m))) => {
                if let MessageContent::Audio(audio) = &m.content {
                    let title = [&audio.title, &audio.file_name]
                        .into_iter()
                        .find(|t| !t.trim().is_empty())
                        .cloned()
                        .unwrap_or_else(|| "Audio".into());
                    return TrackMeta {
                        title,
                        subtitle: audio.performer.clone(),
                    };
                }
                TrackMeta {
                    title: "Audio".into(),
                    subtitle: String::new(),
                }
            }
            (PlaybackKind::Voice, Some((session, m))) => TrackMeta {
                title: session
                    .sender_name_and_accent(m.sender.clone())
                    .0
                    .unwrap_or_else(|| "Voice message".into()),
                subtitle: "Voice message".into(),
            },
            (PlaybackKind::Voice, None) => TrackMeta {
                title: "Voice message".into(),
                subtitle: String::new(),
            },
            (PlaybackKind::Audio, None) => TrackMeta {
                title: "Audio".into(),
                subtitle: String::new(),
            },
        }
    }

    /// Remember which chat and message the bar is about; called when a track
    /// starts.
    pub(super) fn note_track_started(
        &mut self,
        kind: PlaybackKind,
        chat: ChatId,
        id: MessageId,
        cx: &mut Context<Self>,
    ) {
        self.player.chat = Some(chat);
        self.player.meta = Some(self.track_meta_for(chat, id, kind));
        let duration = self
            .playback_clock
            .as_ref()
            .map_or(0.1, |c| c.duration_secs().max(0.1));
        let offset = self
            .playback_clock
            .as_ref()
            .map_or(0.0, |c| c.elapsed_secs());
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(duration as f32)
                .step(0.1)
                .default_value(offset.clamp(0.0, duration) as f32)
        });
        cx.subscribe(&slider, |this, _entity, event: &SliderEvent, cx| {
            this.on_seek_event(event, cx);
        })
        .detach();
        self.player.slider = Some(slider);
    }

    /// The chat's playable messages, oldest first.
    fn playlist_audio_ids(&self, chat: ChatId) -> Vec<i64> {
        self.session()
            .and_then(|s| s.histories.get(&chat.0))
            .map(|h| {
                h.ordered()
                    .into_iter()
                    .filter(|m| matches!(m.content, MessageContent::Audio(_)))
                    .map(|m| m.id.0)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn playlist_voice_entries(&self, chat: ChatId) -> Vec<VoiceEntry> {
        self.session()
            .and_then(|s| s.histories.get(&chat.0))
            .map(|h| {
                h.ordered()
                    .into_iter()
                    .filter_map(|m| match &m.content {
                        MessageContent::VoiceNote(v) => Some(VoiceEntry {
                            id: m.id.0,
                            listened: v.is_listened,
                            outgoing: m.is_outgoing,
                        }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the bar offers previous / next for the active track.
    fn playlist_can_move(&self) -> (bool, bool) {
        if self.active_playback_kind() != PlaybackKind::Audio {
            return (false, false);
        }
        let Some(chat) = self.player.chat else {
            return (false, false);
        };
        let len = self.playlist_audio_ids(chat).len();
        (len > 1, len > 1)
    }

    /// The message to play after the active one, per the playlist rules.
    fn playlist_neighbor(&mut self, step: Step, auto: bool) -> Option<(ChatId, MessageId)> {
        let chat = self.player.chat?;
        let current = self.active_playback_id()?;
        let next = match self.active_playback_kind() {
            PlaybackKind::Voice => {
                if step != Step::Next || !auto {
                    return None;
                }
                playlist::next_voice(&self.playlist_voice_entries(chat), current.0)
            }
            PlaybackKind::Audio => {
                let ids = self.playlist_audio_ids(chat);
                let mv = Move {
                    current: current.0,
                    step,
                    auto,
                    repeat: self.player.repeat,
                    order: self.player.order,
                };
                playlist::next_track(&ids, mv, &mut self.player.shuffle, random_below)
            }
        };
        next.map(|id| (chat, MessageId(id)))
    }

    /// Start a voice note or music file by message id, as a row's play
    /// button would.
    pub(super) fn play_message(&mut self, chat: ChatId, id: MessageId, cx: &mut Context<Self>) {
        let content = self.session().and_then(|s| {
            s.histories
                .get(&chat.0)
                .and_then(|h| h.ordered().into_iter().find(|m| m.id == id))
                .map(|m| m.content.clone())
        });
        match content {
            Some(MessageContent::VoiceNote(v)) => self.toggle_voice_playback(
                chat,
                id,
                v.file_id,
                v.is_listened,
                f64::from(v.duration),
                cx,
            ),
            Some(MessageContent::Audio(a)) => {
                self.toggle_audio_playback(chat, id, a.file_id, f64::from(a.duration), cx)
            }
            _ => {}
        }
    }

    /// Previous / next from the bar or the media keys. Does nothing at the
    /// end of the list, as Telegram Desktop.
    pub(super) fn player_step(&mut self, step: Step, cx: &mut Context<Self>) {
        if let Some((chat, id)) = self.playlist_neighbor(step, false) {
            self.play_message(chat, id, cx);
        }
    }

    /// What follows a track that played to its end (chosen before the stop
    /// clears the active id).
    pub(super) fn next_after_finish(&mut self) -> Option<(ChatId, MessageId)> {
        self.playlist_neighbor(Step::Next, true)
    }

    /// Pause when playing, resume when paused.
    pub(super) fn toggle_active_playback(&mut self, cx: &mut Context<Self>) {
        let Some(clock) = self.playback_clock.as_ref() else {
            return;
        };
        if clock.is_playing() {
            self.pause_active_playback();
        } else {
            self.resume_active_playback();
        }
        cx.notify();
    }

    /// The bar's close button: stop and forget the track.
    pub(super) fn close_player(&mut self, cx: &mut Context<Self>) {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.player.shuffle.clear();
        media_session::publish(None);
        cx.notify();
    }

    /// Apply what the OS asked for (media keys, the Now Playing widget).
    pub(super) fn drain_media_commands(&mut self, cx: &mut Context<Self>) {
        for command in media_session::take_commands() {
            let playing = self
                .playback_clock
                .as_ref()
                .is_some_and(PlaybackClock::is_playing);
            match command {
                Command::Play if !playing => self.toggle_active_playback(cx),
                Command::Pause if playing => self.toggle_active_playback(cx),
                Command::Toggle => self.toggle_active_playback(cx),
                Command::Stop => self.close_player(cx),
                Command::Next => self.player_step(Step::Next, cx),
                Command::Previous => self.player_step(Step::Previous, cx),
                Command::SeekTo(secs) => self.seek_active_to(secs, cx),
                Command::Play | Command::Pause => {}
            }
        }
    }

    /// Tell the OS what is playing (cheap: deduplicated downstream).
    pub(super) fn publish_now_playing(&mut self) {
        let (Some(id), Some(clock)) = (self.active_playback_id(), self.playback_clock.as_ref())
        else {
            media_session::publish(None);
            return;
        };
        let meta = match (&self.player.meta, self.player.chat) {
            (Some(meta), _) => meta.clone(),
            (None, Some(chat)) => self.track_meta_for(chat, id, self.active_playback_kind()),
            (None, None) => TrackMeta {
                title: "Audio".into(),
                subtitle: String::new(),
            },
        };
        let (can_previous, can_next) = self.playlist_can_move();
        let info = NowPlaying {
            title: meta.title,
            artist: meta.subtitle,
            duration_secs: clock.duration_secs(),
            position_secs: clock.elapsed_secs(),
            playing: clock.is_playing(),
            rate: self.playback_speed,
            can_next,
            can_previous,
        };
        media_session::publish(Some(&info));
    }

    /// Open the track's chat if needed and scroll to the message.
    fn jump_to_playing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(chat), Some(id)) = (self.player.chat, self.active_playback_id()) else {
            return;
        };
        if self.open_chat_id() != Some(chat) {
            self.select_listed_chat(chat, window, cx);
        }
        self.jump_to_pinned_message(id, cx);
    }

    /// The bar, when a voice note or music file is active.
    pub(super) fn player_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.active_playback_id()?;
        let clock = self.playback_clock.as_ref()?;
        let kind = self.active_playback_kind();
        let meta = match (&self.player.meta, self.player.chat) {
            (Some(meta), _) => meta.clone(),
            (None, Some(chat)) => self.track_meta_for(chat, id, kind),
            (None, None) => TrackMeta {
                title: if kind == PlaybackKind::Audio {
                    "Audio".into()
                } else {
                    "Voice message".into()
                },
                subtitle: String::new(),
            },
        };
        let playing = clock.is_playing();
        let elapsed = self
            .seek_preview_secs
            .unwrap_or_else(|| clock.elapsed_secs());
        let time = format!(
            "{} / {}",
            format_voice_duration(elapsed as i32),
            format_voice_duration(clock.duration_secs() as i32)
        );
        let music = kind == PlaybackKind::Audio;
        let (can_previous, can_next) = self.playlist_can_move();
        let (repeat, order) = (self.player.repeat, self.player.order);
        let muted = self.playback_volume < 0.01;
        let speed = Self::speed_label(self.playback_speed);

        let icon_button = |id: &'static str, icon: IconName, label: &'static str| {
            Button::new(id)
                .icon(icon)
                .ghost()
                .small()
                .tooltip(label)
                .accessibility_label(label)
        };

        let title_block = div()
            .id("player-bar-jump")
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .role(Role::Button)
            .aria_label("Go to message")
            .tab_index(0)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| this.jump_to_playing(window, cx)))
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .truncate()
                    .text_color(text_primary())
                    .child(meta.title),
            )
            .when(!meta.subtitle.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(text_muted())
                        .child(meta.subtitle),
                )
            });

        Some(
            div()
                .id("player-bar")
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .h(px(48.))
                .px_3()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .role(Role::Group)
                .aria_label("Now playing")
                .when(music, |this| {
                    this.child(
                        icon_button("player-bar-prev", IconName::SkipBack, "Previous track")
                            .disabled(!can_previous)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.player_step(Step::Previous, cx);
                            })),
                    )
                })
                .child(
                    icon_button(
                        "player-bar-play",
                        if playing {
                            IconName::Pause
                        } else {
                            IconName::Play
                        },
                        if playing { "Pause" } else { "Play" },
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_active_playback(cx))),
                )
                .when(music, |this| {
                    this.child(
                        icon_button("player-bar-next", IconName::SkipForward, "Next track")
                            .disabled(!can_next)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.player_step(Step::Next, cx);
                            })),
                    )
                })
                .child(title_block)
                .when_some(self.player.slider.clone(), |this, slider| {
                    this.child(
                        div()
                            .id("player-bar-seek")
                            .flex_none()
                            .w(px(140.))
                            .role(Role::Group)
                            .aria_label("Playback position")
                            .child(Slider::new(&slider).bg(accent()).text_color(accent())),
                    )
                })
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(text_muted())
                        .child(time),
                )
                .child(
                    Button::new("player-bar-speed")
                        .label(speed)
                        .ghost()
                        .small()
                        .tooltip("Playback speed")
                        .accessibility_label("Playback speed")
                        .on_click(cx.listener(|this, _, _, cx| this.cycle_playback_speed(cx))),
                )
                .child(
                    icon_button(
                        "player-bar-volume",
                        if muted {
                            IconName::VolumeX
                        } else {
                            IconName::Volume2
                        },
                        if muted { "Unmute" } else { "Mute" },
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_playback_mute(cx))),
                )
                .when(music, |this| {
                    this.child(
                        icon_button(
                            "player-bar-repeat",
                            if repeat == RepeatMode::One {
                                IconName::Repeat1
                            } else {
                                IconName::Repeat
                            },
                            match repeat {
                                RepeatMode::Off => "Repeat: off",
                                RepeatMode::One => "Repeat: this track",
                                RepeatMode::All => "Repeat: all tracks",
                            },
                        )
                        .selected(repeat != RepeatMode::Off)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.player.repeat = this.player.repeat.cycled();
                            cx.notify();
                        })),
                    )
                    .child(
                        icon_button(
                            "player-bar-order",
                            if order == OrderMode::Shuffle {
                                IconName::Shuffle
                            } else {
                                IconName::ArrowDownUp
                            },
                            match order {
                                OrderMode::InOrder => "Order: in order",
                                OrderMode::Reverse => "Order: newest first",
                                OrderMode::Shuffle => "Order: shuffle",
                            },
                        )
                        .selected(order != OrderMode::InOrder)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.player.order = this.player.order.cycled();
                            this.player.shuffle.clear();
                            cx.notify();
                        })),
                    )
                })
                .child(
                    icon_button("player-bar-close", IconName::X, "Close player")
                        .on_click(cx.listener(|this, _, _, cx| this.close_player(cx))),
                )
                .into_any_element(),
        )
    }
}

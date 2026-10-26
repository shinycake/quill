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
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::{AudioScrubber, ScrubberFace};
use super::playback::PlaybackKind;
use super::playback_state::SeekSite;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::media_session::{self, Command, NowPlaying};
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
    /// The chat's playable ids, kept while its messages are unchanged.
    playlist: PlaylistCache,
}

/// A chat's playlist (`playlist_audio_ids`) for one state of its loaded
/// messages: the ids come from `HistoryMessages::revision`, which moves on
/// any change, so the answer is the one a fresh scan would give.
#[derive(Default)]
pub(super) struct PlaylistCache(std::cell::RefCell<Option<(i64, u64, std::rc::Rc<[i64]>)>>);

impl PlaylistCache {
    /// The ids for `chat` at `revision`: the kept ones, or `compute`d anew
    /// and kept.
    pub(super) fn ids(
        &self,
        chat: i64,
        revision: u64,
        compute: impl FnOnce() -> Vec<i64>,
    ) -> std::rc::Rc<[i64]> {
        if let Ok(kept) = self.0.try_borrow()
            && let Some((kept_chat, kept_revision, ids)) = kept.as_ref()
            && *kept_chat == chat
            && *kept_revision == revision
        {
            return ids.clone();
        }
        let ids: std::rc::Rc<[i64]> = compute().into();
        if let Ok(mut kept) = self.0.try_borrow_mut() {
            *kept = Some((chat, revision, ids.clone()));
        }
        ids
    }
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
        // A saved-music track has no message: its id is the negated file id.
        if id.0 < 0
            && let Some(track) = self.session().and_then(|session| {
                session
                    .media
                    .playlists
                    .values()
                    .flat_map(|playlist| &playlist.tracks)
                    .find(|track| quill::state::playlist_message_id(track.file_id) == id)
            })
        {
            return TrackMeta {
                title: [&track.title, &track.file_name]
                    .into_iter()
                    .find(|t| !t.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| tr(keys::Q_AV_AUDIO).into()),
                subtitle: track.performer.clone(),
            };
        }
        match (kind, message) {
            (PlaybackKind::Audio, Some((_, m))) => {
                if let MessageContent::Audio(audio) = &m.content {
                    let title = [&audio.title, &audio.file_name]
                        .into_iter()
                        .find(|t| !t.trim().is_empty())
                        .cloned()
                        .unwrap_or_else(|| tr(keys::Q_AV_AUDIO).into());
                    return TrackMeta {
                        title,
                        subtitle: audio.performer.clone(),
                    };
                }
                TrackMeta {
                    title: tr(keys::Q_AV_AUDIO).into(),
                    subtitle: String::new(),
                }
            }
            (PlaybackKind::Voice, Some((session, m))) => TrackMeta {
                title: session
                    .sender_name_and_accent(m.sender)
                    .0
                    .unwrap_or_else(|| tr(keys::Q_AV_VOICE_MESSAGE).into()),
                subtitle: tr(keys::Q_AV_VOICE_MESSAGE).into(),
            },
            (PlaybackKind::Voice, None) => TrackMeta {
                title: tr(keys::Q_AV_VOICE_MESSAGE).into(),
                subtitle: String::new(),
            },
            (PlaybackKind::Audio, None) => TrackMeta {
                title: tr(keys::Q_AV_AUDIO).into(),
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
        self.playback.player.chat = Some(chat);
        self.playback.player.meta = Some(self.track_meta_for(chat, id, kind));
        let duration = self.playback.track_duration().map_or(0.1, |d| d.max(0.1));
        let offset = self.playback.active_position_secs().unwrap_or(0.0);
        let slider = cx.new(|_| {
            SliderState::new()
                .label(tr(keys::Q_AV_PLAYBACK_POSITION))
                .min(0.0)
                .max(duration as f32)
                .step(0.1)
                .default_value(offset.clamp(0.0, duration) as f32)
        });
        cx.subscribe(&slider, |this, _entity, event: &SliderEvent, cx| {
            this.on_seek_event(event, cx);
        })
        .detach();
        self.playback.player.slider = Some(slider);
    }

    /// The chat's playable messages, oldest first.
    fn playlist_audio_ids(&self, chat: ChatId) -> std::rc::Rc<[i64]> {
        let history = self.session().and_then(|s| s.histories.get(&chat.0));
        self.playback.player.playlist.ids(
            chat.0,
            history.map_or(0, |h| h.messages.revision()),
            || {
                history
                    .map(|h| {
                        h.ordered()
                            .into_iter()
                            .filter(|m| matches!(m.content, MessageContent::Audio(_)))
                            .map(|m| m.id.0)
                            .collect()
                    })
                    .unwrap_or_default()
            },
        )
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
    pub(super) fn playlist_can_move(&self) -> (bool, bool) {
        if self.active_playback_kind() != PlaybackKind::Audio {
            return (false, false);
        }
        let Some(chat) = self.playback.player.chat else {
            return (false, false);
        };
        let len = self.playlist_audio_ids(chat).len();
        (len > 1, len > 1)
    }

    /// The message to play after the active one, per the playlist rules.
    fn playlist_neighbor(&mut self, step: Step, auto: bool) -> Option<(ChatId, MessageId)> {
        let chat = self.playback.player.chat?;
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
                    repeat: self.playback.player.repeat,
                    order: self.playback.player.order,
                };
                playlist::next_track(&ids, mv, &mut self.playback.player.shuffle, random_below)
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
        if self.playback.track_duration().is_none() {
            return;
        }
        if self.playback.is_playing() {
            self.pause_active_playback();
        } else {
            self.resume_active_playback(cx);
        }
        cx.notify();
    }

    /// The bar's close button: stop and forget the track.
    pub(super) fn close_player(&mut self, cx: &mut Context<Self>) {
        self.stop_voice_playback();
        self.stop_audio_playback();
        self.playback.player.shuffle.clear();
        media_session::publish(None);
        cx.notify();
    }

    /// Apply what the OS asked for (media keys, the Now Playing widget).
    pub(super) fn drain_media_commands(&mut self, cx: &mut Context<Self>) {
        for command in media_session::take_commands() {
            let playing = self.playback.is_playing();
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
        let (Some(id), Some(duration), Some(position)) = (
            self.active_playback_id(),
            self.playback.track_duration(),
            self.playback.active_position_secs(),
        ) else {
            media_session::publish(None);
            return;
        };
        let meta = match (&self.playback.player.meta, self.playback.player.chat) {
            (Some(meta), _) => meta.clone(),
            (None, Some(chat)) => self.track_meta_for(chat, id, self.active_playback_kind()),
            (None, None) => TrackMeta {
                title: tr(keys::Q_AV_AUDIO).into(),
                subtitle: String::new(),
            },
        };
        let (can_previous, can_next) = self.playlist_can_move();
        let info = NowPlaying {
            title: meta.title,
            artist: meta.subtitle,
            duration_secs: duration,
            position_secs: position,
            playing: self.playback.is_playing(),
            rate: self.playback.speed,
            can_next,
            can_previous,
        };
        media_session::publish(Some(&info));
    }

    /// Open the track's chat if needed and scroll to the message.
    fn jump_to_playing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(chat), Some(id)) = (self.playback.player.chat, self.active_playback_id()) else {
            return;
        };
        if self.open_chat_id() != Some(chat) {
            self.select_listed_chat(chat, window, cx);
        }
        self.jump_to_pinned_message(id, cx);
    }

    /// The bar, when a voice note or music file is active.
    pub(super) fn player_bar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.active_playback_id()?;
        self.playback.track_duration()?;
        self.playback.active_position_secs()?;
        self.player_bar_body(cx)
    }

    /// `player_bar` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn player_bar_body(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.active_playback_id()?;
        let duration = self.playback.track_duration()?;
        let position = self.playback.active_position_secs()?;
        let kind = self.active_playback_kind();
        let meta = match (&self.playback.player.meta, self.playback.player.chat) {
            (Some(meta), _) => meta.clone(),
            (None, Some(chat)) => self.track_meta_for(chat, id, kind),
            (None, None) => TrackMeta {
                title: if kind == PlaybackKind::Audio {
                    tr(keys::Q_AV_AUDIO).into()
                } else {
                    tr(keys::Q_AV_VOICE_MESSAGE).into()
                },
                subtitle: String::new(),
            },
        };
        let playing = self.playback.is_playing();
        let elapsed = self.playback.seek_preview_secs.unwrap_or(position);
        let time = format!(
            "{} / {}",
            format_voice_duration(elapsed as i32),
            format_voice_duration(duration as i32)
        );
        let progress = if duration > 0.0 {
            (elapsed / duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let hover = self
            .playback
            .seek_hover
            .filter(|hover| hover.site == SeekSite::PlayerBar)
            .map(|hover| hover.at);
        let dragging = self.playback.seek_preview_secs.is_some();
        let music = kind == PlaybackKind::Audio;
        let (can_previous, can_next) = self.playlist_can_move();
        let (repeat, order) = (self.playback.player.repeat, self.playback.player.order);
        let muted = self.playback.volume < 0.01;
        // The shared trigger is white for the video overlay; the bar sits on
        // the theme surface.
        let speed_trigger = self
            .speed_label_trigger("player-bar-speed")
            .text_color(text_primary());
        let speed_dial = self.speed_dial("player-bar-speed", speed_trigger, cx);

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
            .aria_label(tr(keys::Q_AV_GO_TO_MESSAGE))
            .tab_index(0)
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| this.jump_to_playing(window, cx)))
            .child(
                div()
                    .text_sm()
                    .font_medium()
                    .truncate()
                    .text_color(text_primary())
                    .child(super::bidi_line::one_line_plain(meta.title)),
            )
            .when(!meta.subtitle.is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(text_muted())
                        .child(super::bidi_line::one_line_plain(meta.subtitle)),
                )
            });

        // The bar names the track's chat: tagged with it, so the UI driver
        // prunes it for a chat without a grant.
        let chat_tag = self
            .playback
            .player
            .chat
            .map(|chat| quill::agent::ui_driver::redact::chat_tag(chat.0));
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
                .when_some(chat_tag, |this, tag| this.a11y_tag(tag))
                .aria_label(tr(keys::Q_AV_NOW_PLAYING))
                .when(music, |this| {
                    this.child(
                        icon_button(
                            "player-bar-prev",
                            IconName::SkipBack,
                            tr(keys::Q_AV_PREVIOUS_TRACK),
                        )
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
                        if playing {
                            tr(keys::MAC_MENU_PLAYER_PAUSE)
                        } else {
                            tr(keys::Q_AV_PLAY)
                        },
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_active_playback(cx))),
                )
                .when(music, |this| {
                    this.child(
                        icon_button(
                            "player-bar-next",
                            IconName::SkipForward,
                            tr(keys::Q_AV_NEXT_TRACK),
                        )
                        .disabled(!can_next)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.player_step(Step::Next, cx);
                        })),
                    )
                })
                .child(title_block)
                .when_some(self.playback.player.slider.clone(), |this, slider| {
                    this.child(
                        div().flex_none().w(px(140.)).child(
                            AudioScrubber::new(
                                "player-bar-seek",
                                &slider,
                                ScrubberFace::Line,
                                accent(),
                                QuillApp::seek_hover_handler(SeekSite::PlayerBar, cx),
                            )
                            .progress(progress)
                            .hover(hover)
                            .dragging(dragging),
                        ),
                    )
                })
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(text_muted())
                        .child(time),
                )
                .child(speed_dial)
                .child(
                    icon_button(
                        "player-bar-volume",
                        if muted {
                            IconName::VolumeX
                        } else {
                            IconName::Volume2
                        },
                        if muted {
                            tr(keys::Q_AV_UNMUTE)
                        } else {
                            tr(keys::GROUP_CALL_CONTEXT_MUTE)
                        },
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
                                RepeatMode::Off => tr(keys::Q_AV_REPEAT_OFF),
                                RepeatMode::One => tr(keys::Q_AV_REPEAT_THIS_TRACK),
                                RepeatMode::All => tr(keys::Q_AV_REPEAT_ALL_TRACKS),
                            },
                        )
                        .selected(repeat != RepeatMode::Off)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.playback.player.repeat = this.playback.player.repeat.cycled();
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
                                OrderMode::InOrder => tr(keys::Q_AV_ORDER_IN_ORDER),
                                OrderMode::Reverse => tr(keys::Q_AV_ORDER_NEWEST_FIRST),
                                OrderMode::Shuffle => tr(keys::Q_AV_ORDER_SHUFFLE),
                            },
                        )
                        .selected(order != OrderMode::InOrder)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.playback.player.order = this.playback.player.order.cycled();
                            this.playback.player.shuffle.clear();
                            cx.notify();
                        })),
                    )
                })
                .child(
                    icon_button("player-bar-close", IconName::X, tr(keys::Q_AV_CLOSE_PLAYER))
                        .on_click(cx.listener(|this, _, _, cx| this.close_player(cx))),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod cache_tests;

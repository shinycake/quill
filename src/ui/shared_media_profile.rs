//! The gallery's profile-style tabs (tdesktop `info/profile/tabs`): the
//! `profile-media-tabs-expanded` option, thumbnail grids for Stories and
//! Gifts, "load more", the profile's music button and the Playlist page.
#![deny(clippy::indexing_slicing)]

use super::accent;
use super::app::QuillApp;
use super::playlist_manage::PlaylistDrag;
use super::premium_ui::sticker_tile;
use super::pressable::{PressableDiv, action_row};
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::state::{
    SharedMediaAction, SharedMediaItem, SharedMediaTab, SharedMediaVisual, playlist_message_id,
    track_name,
};
use quill::telegram::envelope::{ChatKind, MessageSender};

/// Tile side in the Stories and Gifts grids (three per row in the panel).
const TILE: f32 = 96.;

impl QuillApp {
    /// Bring the open gallery (or the next one) in line with the option.
    pub(super) fn apply_profile_media_tabs_option(&mut self, cx: &mut Context<Self>) {
        let split = self.chat_prefs.profile_media_tabs_expanded;
        if let Some(live) = self.live.as_mut() {
            let gallery = &mut live.driver.session.media.shared_media;
            if gallery.open {
                if live.driver.set_shared_media_split(split).is_err() {
                    self.connection
                        .set_note(StatusNote::error(tr(keys::Q_OPS_COULD_NOT_LOAD_THAT_TAB)));
                }
            } else {
                gallery.media_split = split;
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let counts = session
                .media
                .shared_media
                .chat_id
                .and_then(|id| session.media.chat_media_counts.get(&id.0))
                .cloned();
            session
                .media
                .shared_media
                .set_media_split(split, counts.as_ref());
        }
        cx.notify();
    }

    /// The next page of a gallery tab (live only).
    pub(super) fn load_more_shared_media_ui(
        &mut self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && live.driver.fetch_more_shared_media(tab).is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_OPS_COULD_NOT_LOAD_MORE)));
        }
        cx.notify();
    }

    /// A click on a gallery row or tile. Messages open the viewer or jump
    /// to the message; stories open the story viewer over the loaded
    /// stories; gifts open the received-gifts list of the chat.
    pub(super) fn activate_shared_media_item(
        &mut self,
        action: SharedMediaAction,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        match action {
            SharedMediaAction::Message => {
                // Photos, videos and GIFs open the viewer over this list;
                // everything else jumps to the message.
                if !self.open_shared_media_viewer(message_id, cx) {
                    self.jump_to_shared_media_item_ui(message_id, cx);
                }
            }
            SharedMediaAction::Story(story_id) => self.open_shared_media_story(story_id, cx),
            SharedMediaAction::Gift => self.open_shared_media_gifts(cx),
        }
    }

    fn open_shared_media_story(&mut self, story_id: i32, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(chat_id) = session.media.shared_media.chat_id else {
            return;
        };
        let Some(stories_tab) = session.media.shared_media.tab_at(SharedMediaTab::Stories) else {
            return;
        };
        let keys: Vec<(i64, i32)> = stories_tab
            .items
            .iter()
            .filter_map(|item| match item.action {
                SharedMediaAction::Story(id) => Some((chat_id.0, id)),
                _ => None,
            })
            .collect();
        let items = quill::story_viewer::collect_found_story_items(&keys, &session.stories.stories);
        if let Some(index) = items.iter().position(|item| item.story_id == story_id) {
            self.begin_story_viewer(items, index, cx);
        }
    }

    fn open_shared_media_gifts(&mut self, cx: &mut Context<Self>) {
        let owner = self.session().and_then(|session| {
            let chat = session.chats.get(&session.media.shared_media.chat_id?.0)?;
            match chat.kind {
                ChatKind::Private { user_id } if user_id.0 > 0 => {
                    Some(MessageSender::User { user_id: user_id.0 })
                }
                ChatKind::Supergroup {
                    is_channel: true, ..
                } => Some(MessageSender::Chat { chat_id: chat.id.0 }),
                _ => None,
            }
        });
        if let Some(owner) = owner {
            self.open_gifts(owner, cx);
        }
    }

    /// The Stories or Gifts tab as a grid of thumbnails (tdesktop shows
    /// both as grids).
    pub(super) fn shared_media_grid(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let Some(state) = session.media.shared_media.tab_at(tab) else {
            return div().into_any_element();
        };
        let roots = self.media_display_roots();
        let mut grid = div().flex().flex_wrap().gap_1().px_2().py_2();
        for (index, item) in state.items.iter().enumerate() {
            let action = item.action;
            let tile = self
                .shared_media_tile(tab, index, item, &roots, cx)
                .id(("shared-media-tile", index as u64))
                .role(gpui_kit::Role::Button)
                .aria_label(item.label.clone())
                .tab_index(0)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.activate_shared_media_item(action, MessageId(0), cx);
                }));
            grid = grid.child(tile);
        }
        let mut body = div()
            .id(("shared-media-items", tab.index() as u64))
            .flex_1()
            .overflow_y_scroll()
            .child(grid);
        if state.can_load_more() {
            body = body.child(self.shared_media_load_more(tab, cx));
        }
        body.into_any_element()
    }

    fn shared_media_tile(
        &self,
        tab: SharedMediaTab,
        index: usize,
        item: &SharedMediaItem,
        roots: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) -> Div {
        let files = &self.session().map(|s| &s.media.files);
        let base = div()
            .relative()
            .size(px(TILE))
            .flex_none()
            .rounded_lg()
            .overflow_hidden();
        match &item.visual {
            SharedMediaVisual::Story {
                thumbs,
                width,
                height,
            } => {
                let path = thumbs.iter().find_map(|id| {
                    let file = files.and_then(|files| files.get(&id.0))?;
                    sandboxed_display_path(file.usable_path()?, roots)
                });
                let base = base.bg(cx.theme().secondary);
                match path {
                    Some(path) => base.child(
                        img(super::image_budget::sized_media(
                            &path,
                            (px(TILE), px(TILE)),
                            Some((*width, *height)),
                            super::image_budget::Fit::Cover,
                        ))
                        .size(px(TILE))
                        .object_fit(ObjectFit::Cover),
                    ),
                    None => base
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xl()
                        .child(tab.glyph()),
                }
            }
            SharedMediaVisual::Gift(gift) => {
                let backdrop = match gift.gift.backdrop {
                    Some((center, edge)) => base.bg(linear_gradient(
                        180.,
                        linear_color_stop(rgb(center), 0.),
                        linear_color_stop(rgb(edge), 1.),
                    )),
                    None => base.bg(cx.theme().secondary.opacity(0.6)),
                };
                let fg = if gift.gift.backdrop.is_some() {
                    gpui_kit::white()
                } else {
                    cx.theme().foreground
                };
                let empty = quill::state::FileMap::default();
                backdrop
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(sticker_tile(
                        ("shared-media-gift-sticker", index as u64),
                        gift.gift.sticker.as_ref(),
                        files.unwrap_or(&empty),
                        roots,
                        60.,
                        "\u{1F381}",
                    ))
                    .child(
                        div()
                            .px_1()
                            .text_center()
                            .text_xs()
                            .font_semibold()
                            .text_color(fg)
                            .child(gift.headline()),
                    )
            }
            SharedMediaVisual::None => base.child(item.label.clone()),
        }
    }

    pub(super) fn shared_media_load_more(
        &self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .justify_center()
            .py_2()
            .child(
                Button::new(("shared-media-load-more", tab.index() as u64))
                    .ghost()
                    .label(tr(keys::Q_OPS_LOAD_MORE))
                    .accessibility_label(tr(keys::Q_OPS_LOAD_MORE_SHARED_MEDIA))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.load_more_shared_media_ui(tab, cx);
                    })),
            )
            .into_any_element()
    }

    // ---- Playlist ------------------------------------------------------

    /// tdesktop's profile music button: the first saved track of the user,
    /// opening the Playlist page. `None` without saved music.
    pub(super) fn playlist_music_button(
        &self,
        user_id: i64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let playlist = self.session()?.media.playlists.get(&user_id)?;
        let first = playlist.tracks.first()?;
        Some(
            action_row(
                ("profile-music-button", user_id as u64),
                Some(IconName::AudioLines),
                track_name(first),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_playlist_ui(user_id, cx)))
            .into_any_element(),
        )
    }

    pub(super) fn open_playlist_ui(&mut self, user_id: i64, cx: &mut Context<Self>) {
        let Some(session) = self.session_mut_for_playlist() else {
            return;
        };
        session.media.shared_media.playlist_user = Some(user_id);
        cx.notify();
    }

    fn session_mut_for_playlist(&mut self) -> Option<&mut quill::state::Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// The Playlist page: a user's saved music, playable.
    pub(super) fn playlist_panel(&self, user_id: i64, cx: &mut Context<Self>) -> AnyElement {
        let session = self.session();
        let me = session.and_then(|s| s.my_user_id) == Some(user_id);
        let title = if me {
            tr(keys::MEDIA_SAVED_MUSIC_YOUR)
        } else {
            tr(keys::MEDIA_SAVED_MUSIC_TITLE)
        };
        let playlist = session.and_then(|s| s.media.playlists.get(&user_id));
        let chat = session
            .and_then(|s| s.open_chat)
            .unwrap_or(quill::ids::ChatId(0));
        let playing = self.playback.playing_audio;
        let is_playing = self.playback.is_playing();
        let mut list = div()
            .id("playlist-tracks")
            .flex_1()
            .overflow_y_scroll()
            .py_1();
        if let Some(playlist) = playlist {
            for (index, track) in playlist.tracks.iter().enumerate() {
                let message_id = playlist_message_id(track.file_id);
                let active = playing == Some(message_id);
                let (file_id, duration) = (track.file_id, f64::from(track.duration));
                // A press while the track downloads cancels it.
                let fetching = self.session().is_some_and(|session| {
                    super::message_media::file_is_downloading(
                        file_id,
                        &session.media.files,
                        &session.media.downloads,
                    )
                });
                let mut name = track_name(track);
                let drag_title = name.clone();
                if track.duration > 0 {
                    name = format!(
                        "{name} · {}:{:02}",
                        track.duration / 60,
                        track.duration % 60
                    );
                }
                list = list.child(
                    div()
                        .id(("playlist-track", index as u64))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .role(gpui_kit::Role::Button)
                        .aria_label(name.clone())
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .child(div().w(px(20.)).text_color(accent()).child(
                            if active && is_playing {
                                "❚❚"
                            } else if fetching {
                                "✕"
                            } else {
                                "▶"
                            },
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .text_color(if active {
                                    accent().into()
                                } else {
                                    cx.theme().foreground
                                })
                                .child(name),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                this.open_playlist_menu(file_id, event.position, cx);
                            }),
                        )
                        // Only your own playlist can be reordered
                        // (`setReorderDescriptor` is set for `isSelf`).
                        .when(me, |row| {
                            row.on_drag(
                                PlaylistDrag {
                                    index,
                                    title: drag_title.clone(),
                                },
                                |drag: &PlaylistDrag, _, _, cx| cx.new(|_| drag.clone()),
                            )
                            .on_drop(cx.listener(
                                move |this, drag: &PlaylistDrag, _, cx| {
                                    this.reorder_playlist_track(drag.index, index, cx);
                                },
                            ))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.press_media(file_id, cx, |this, cx| {
                                this.toggle_audio_playback(chat, message_id, file_id, duration, cx);
                            });
                        })),
                );
            }
            if playlist.has_more() {
                list = list.child(
                    div().flex().justify_center().py_2().child(
                        Button::new("playlist-load-more")
                            .ghost()
                            .label(tr(keys::Q_OPS_LOAD_MORE))
                            .accessibility_label(tr(keys::Q_OPS_LOAD_MORE_TRACKS))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(live) = this.live.as_mut() {
                                    let _ = live.driver.fetch_playlist(user_id, true);
                                }
                                cx.notify();
                            })),
                    ),
                );
            }
        }
        let empty = playlist.is_none_or(|p| p.loaded && p.tracks.is_empty());
        div()
            .id("playlist-panel")
            .flex_1()
            .min_w_0()
            .h_full()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().font_semibold().child(title))
                    .child(
                        div()
                            .id("playlist-close")
                            .role(gpui_kit::Role::Button)
                            .aria_label(tr(keys::Q_OPS_CLOSE_PLAYLIST))
                            .tab_index(0)
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_color(cx.theme().muted_foreground)
                            .child("✕")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_shared_media_ui(cx);
                            })),
                    ),
            )
            .child(list)
            .when(empty, |panel| {
                panel.child(
                    div()
                        .px_6()
                        .py_6()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if playlist.is_some() {
                            tr(keys::MEDIA_SONG_EMPTY)
                        } else {
                            tr(keys::PROFILE_LOADING)
                        }),
                )
            })
            .into_any_element()
    }
}

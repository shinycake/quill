//! B14 story viewer extras: video playback on the shared player core
//! (`video_player`, `Purpose::Story`: AVPlayer on macOS, the bundled FFmpeg
//! on Linux and Windows), mute,
//! press-and-hold / Space pause, the close-friends editor, hide / unhide a
//! peer's stories, share to a chat, copy link, save media and post to
//! profile. The pure parts live in `quill::story_extras`.
//!
//! tdesktop reference: `media/stories/media_stories_controller.cpp`
//! (pause on press, `muted`), `media_stories_view.cpp`,
//! `media_stories_share.cpp` (share box), `boxes/peers/edit_close_friends`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_media::file_is_downloading;
use super::story_video_state::{StoryKey, StoryVideo};
use super::video_player::{PlayState, Purpose, VideoPlayer, video_frame};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::local_path::sandboxed_display_path;
use quill::story_extras::{
    CloseFriendsEdit, can_save_story, story_link, video_finished, video_progress,
};
use quill::story_viewer::StoryViewerKind;
use quill::telegram::envelope::{ChatKind, StoryListView};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::chat_theme::danger;
use super::story_viewer::chrome;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// The viewer's result line under the story; dropped while it is closed.
    pub(super) fn set_story_notice(&mut self, notice: String) {
        self.stories
            .with_viewer_ui(|open| open.notice = Some(notice));
    }

    /// A text field of the viewer owns the keyboard: Space and the arrows
    /// belong to it, not to playback.
    pub(super) fn story_text_input_open(&self, window: &Window, cx: &App) -> bool {
        self.stories.viewer_ui().reply_open
            || self.stories.viewer_ui().report_open
            || self.stories.viewer_ui().share_open
            || self.stories.viewer_ui().close_friends_edit.is_some()
            || self.stories.viewer_ui().privacy_edit.is_some()
            || self.stories.viewer_ui().cover.is_some()
            // The live story's comment field, while it has the keyboard.
            || (self.live_story_shown().is_some()
                && self.stories.live.input.focus_handle(cx).is_focused(window))
    }

    /// The current video story's clip, once it is a local sandboxed file.
    pub(in crate::ui) fn story_video_path(&self) -> Option<PathBuf> {
        let item = self.stories.viewer.current()?;
        let file_id = item.video_file_id?;
        let roots = self.media_display_roots();
        self.session()?
            .media
            .files
            .get(&file_id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, &roots))
    }

    fn story_key(&self) -> Option<(i64, i32)> {
        self.stories
            .viewer
            .current()
            .map(|item| (item.chat_id.0, item.story_id))
    }

    /// Whether the current story is a video the viewer plays natively (or
    /// is waiting to), i.e. the player, not the duration clock, drives it.
    pub(super) fn story_video_driven(&self) -> bool {
        let Some(item) = self.stories.viewer.current() else {
            return false;
        };
        item.kind == StoryViewerKind::Video
            && item.video_file_id.is_some()
            && super::video_player::supported()
            && !self.stories.video.failed_for(self.story_key())
    }

    /// The clip is not local yet: the clock stays frozen, so the story does
    /// not run away while it downloads.
    fn story_video_waiting(&self) -> bool {
        self.story_video_driven() && self.stories.video.player().is_none()
    }

    /// Open, replace or drop the player to match the current story.
    pub(super) fn sync_story_video(&mut self, cx: &mut App) {
        let now = Instant::now();
        let key = self.story_key();
        if !self.story_video_driven() {
            // A failed story keeps its fallback; anything else has no clip.
            if !self.stories.video.failed_for(key) {
                self.stories.video = StoryVideo::Idle;
            }
            return;
        }
        let Some(key) = key else {
            self.stories.video = StoryVideo::Idle;
            return;
        };
        self.stories.video.follow(key, now);
        if let Some(player) = self.stories.video.player() {
            if player.take_error().is_some() || player.state() == PlayState::Failed {
                self.fall_back_to_clock(key, now);
            }
            return;
        }
        if let Some(path) = self.story_video_path() {
            let duration = self
                .stories
                .viewer
                .current()
                .and_then(|item| item.duration_secs)
                .map(f64::from);
            match VideoPlayer::open(&path, Purpose::Story, duration, cx) {
                Ok(player) => {
                    player.set_volume(self.story_volume());
                    let paused = self.story_playback_paused();
                    if !paused {
                        if self.story_volume() > 0.0 {
                            self.sound_starts(cx);
                        }
                        player.play();
                    }
                    self.stories.video = StoryVideo::Playing {
                        key,
                        player,
                        paused_by_us: paused,
                        play_at: now,
                    };
                }
                Err(_) => self.fall_back_to_clock(key, now),
            }
            return;
        }
        // Not local: wait while it downloads, then give up.
        let downloading = self.session().is_some_and(|session| {
            self.stories
                .viewer
                .current()
                .and_then(|item| item.video_file_id)
                .is_some_and(|id| {
                    file_is_downloading(id, &session.media.files, &session.media.downloads)
                })
        });
        if self.stories.video.give_up_if_stalled(downloading, now) {
            self.stories.playback.start(now);
        }
    }

    /// The clip of `key` cannot play: the duration clock drives the story.
    fn fall_back_to_clock(&mut self, key: StoryKey, now: Instant) {
        self.stories.video = StoryVideo::Fallback { key };
        self.stories.playback.start(now);
    }

    /// The story's sound level: silent while muted.
    fn story_volume(&self) -> f32 {
        if self.stories.muted {
            0.0
        } else {
            self.players.volume
        }
    }

    /// Pause or resume the player to match `paused`.
    pub(super) fn apply_story_native_pause(&mut self, paused: bool) {
        let Some(player) = self.stories.video.player() else {
            return;
        };
        if paused {
            // `wants_play`, not the state: a story paused while still
            // opening must not start once open. Runs every tick, so a
            // player already paused is left alone.
            if player.wants_play() {
                player.pause();
            }
            self.stories.video.set_paused_by_us(true);
        } else if self.stories.video.paused_by_us() {
            player.play();
            self.stories.video.note_play(Instant::now());
        }
    }

    /// Progress of the current segment: the player's position for video
    /// stories it drives, the duration clock otherwise.
    pub(super) fn story_segment_progress(&self, now: Instant) -> f32 {
        if let Some(player) = self.stories.video.player() {
            let transport = player.transport();
            let duration = transport
                .duration
                .or_else(|| {
                    self.stories
                        .viewer
                        .current()
                        .and_then(|item| item.duration_secs)
                        .map(f64::from)
                })
                .unwrap_or(0.0);
            return video_progress(transport.position, duration);
        }
        if self.story_video_waiting() {
            return 0.0;
        }
        self.stories
            .viewer
            .current()
            .map(|item| self.stories.playback.progress(item, now))
            .unwrap_or(0.0)
    }

    /// Whether the current story reached its end (player or clock).
    pub(super) fn story_segment_finished(&self, now: Instant) -> bool {
        if let Some(player) = self.stories.video.player() {
            let transport = player.transport();
            let duration = transport
                .duration
                .or_else(|| {
                    self.stories
                        .viewer
                        .current()
                        .and_then(|item| item.duration_secs)
                        .map(f64::from)
                })
                .unwrap_or(0.0);
            // A just-requested play() has not started yet.
            let playing = self
                .stories
                .video
                .play_requested_within(Duration::from_secs(1), Instant::now())
                || transport.is_playing()
                || transport.state == PlayState::Opening;
            return video_finished(
                transport.position,
                duration,
                playing,
                self.story_playback_paused(),
            );
        }
        if self.story_video_waiting() {
            return false;
        }
        self.stories
            .viewer
            .current()
            .is_some_and(|item| self.stories.playback.finished(item, now))
    }

    /// Mute / unmute the video story's sound.
    pub(super) fn toggle_story_mute(&mut self, cx: &mut Context<Self>) {
        self.stories.muted = !self.stories.muted;
        if self.stories.video.player().is_some() && self.story_volume() > 0.0 {
            self.sound_starts(cx);
        }
        if let Some(player) = self.stories.video.player() {
            player.set_volume(self.story_volume());
        }
        cx.notify();
    }

    /// Space: pause or resume the story.
    pub(super) fn toggle_story_pause(&mut self, cx: &mut Context<Self>) {
        self.stories.with_viewer_ui(|open| open.pause.toggle());
        self.refresh_story_pause(cx);
    }

    /// Press-and-hold on the story media pauses until release.
    pub(super) fn story_hold(&mut self, held: bool, cx: &mut Context<Self>) {
        if held {
            self.stories.with_viewer_ui(|open| open.pause.press());
        } else {
            self.stories.with_viewer_ui(|open| open.pause.release());
        }
        self.refresh_story_pause(cx);
    }

    fn refresh_story_pause(&mut self, cx: &mut Context<Self>) {
        let paused = self.story_playback_paused();
        self.stories.playback.set_paused(paused, Instant::now());
        self.apply_story_native_pause(paused);
        self.ensure_story_tick(cx);
        cx.notify();
    }

    /// The stories tray list of the current story's chat.
    pub(super) fn story_chat_hidden(&self, chat_id: i64) -> bool {
        self.session()
            .and_then(|s| s.stories.tray.get(&chat_id))
            .is_some_and(|tray| tray.list == Some(StoryListView::Archive))
    }

    /// Hide the peer's stories (archive list) or bring them back.
    pub(super) fn toggle_hide_story_peer(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let archive = !self.story_chat_hidden(item.chat_id.0);
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .set_chat_active_stories_list(item.chat_id, archive)
        });
        self.note_story_page_op(sent.as_ref());
        self.set_story_notice(match sent {
            Some(Ok(_)) if archive => tr(keys::Q_MEDIA_HIDING_STORIES).into(),
            Some(Ok(_)) => tr(keys::Q_MEDIA_SHOWING_STORIES).into(),
            Some(Err(_)) => tr(keys::Q_MEDIA_COULD_NOT_CHANGE_THE_STORIES).into(),
            None => tr(keys::Q_MEDIA_DEMO_SETCHATACTIVESTORIESLIST_RUNS_WITH_LIVE).into(),
        });
        cx.notify();
    }

    /// Post the own story to the profile or remove it from there.
    pub(super) fn toggle_story_profile(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let posted = self
            .session()
            .and_then(|s| s.stories.stories.get(&(item.chat_id.0, item.story_id)))
            .is_some_and(|story| story.is_posted_to_chat_page);
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .toggle_story_is_posted_to_chat_page(item.chat_id, item.story_id, !posted)
        });
        self.note_story_page_op(sent.as_ref());
        self.set_story_notice(match sent {
            Some(Ok(_)) if posted => tr(keys::Q_MEDIA_REMOVING_FROM_PROFILE).into(),
            Some(Ok(_)) => tr(keys::Q_MEDIA_POSTING_TO_PROFILE).into(),
            Some(Err(_)) => tr(keys::Q_MEDIA_COULD_NOT_CHANGE_THE_PROFILE).into(),
            None => tr(keys::Q_MEDIA_DEMO_TOGGLESTORYISPOSTEDTOCHATPAGE_RUNS_WITH_LIVE).into(),
        });
        cx.notify();
    }

    /// Copy the story's public link (`t.me/<username>/s/<id>`).
    pub(super) fn copy_story_link(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let username = self.session().and_then(|session| {
            let chat = session.chats.get(&item.chat_id.0)?;
            match chat.kind {
                ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => session
                    .user(user_id.0)
                    .map(|user| user.username.clone())
                    .filter(|name| !name.is_empty()),
                ChatKind::Supergroup { supergroup_id, .. } => session
                    .supergroup_username(supergroup_id)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string),
                _ => None,
            }
        });
        match username.and_then(|name| story_link(&name, item.story_id)) {
            Some(link) => {
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                self.set_story_notice(tr(keys::Q_MEDIA_STORY_LINK_COPIED).into());
            }
            None => self.set_story_notice(tr(keys::Q_MEDIA_THIS_STORY_HAS_NO_PUBLIC).into()),
        }
        cx.notify();
    }

    /// Save the story's photo / clip to the Downloads folder.
    pub(super) fn save_story_media(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let allowed = self
            .session()
            .and_then(|s| s.stories.stories.get(&(item.chat_id.0, item.story_id)))
            .is_some_and(can_save_story);
        if !allowed {
            self.set_story_notice(tr(keys::Q_MEDIA_SAVING_IS_NOT_ALLOWED_FOR).into());
            cx.notify();
            return;
        }
        let roots = self.media_display_roots();
        let source = {
            let session = self.session();
            let ids = item
                .video_file_id
                .into_iter()
                .chain(item.display_file_ids.iter().copied());
            session.and_then(|session| {
                ids.into_iter().find_map(|id| {
                    session
                        .media
                        .files
                        .get(&id.0)
                        .and_then(|file| file.usable_path())
                        .and_then(|path| sandboxed_display_path(path, &roots))
                })
            })
        };
        self.set_story_notice(match source {
            Some(path) => match quill::media_viewer::save_media_to_downloads(&path) {
                Ok(saved) => tr_with(
                    keys::Q_MEDIA_SAVED_TO,
                    &[("display", &(saved.display()).to_string())],
                ),
                Err(err) => tr_with(keys::Q_MEDIA_COULD_NOT_SAVE, &[("err", &err.to_string())]),
            },
            None => tr(keys::Q_MEDIA_THE_STORY_IS_NOT_DOWNLOADED).into(),
        });
        cx.notify();
    }

    /// Open / close the share panel.
    pub(super) fn toggle_story_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stories
            .with_viewer_ui(|open| open.share_open = !open.share_open);
        self.stories
            .with_viewer_ui(|open| open.close_friends_edit = None);
        if self.stories.viewer_ui().share_open {
            self.stories
                .more_search
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.refresh_story_pause(cx);
    }

    /// Send the current story to `dest` as an `inputMessageStory`.
    pub(super) fn share_story_to(&mut self, dest: quill::ids::ChatId, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let options = self.send_options_to(dest);
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .share_story_to_chat(dest, item.chat_id, item.story_id, &options)
        });
        if matches!(sent, Some(Ok(_))) {
            self.stories.with_viewer_ui(|open| open.share_open = false);
        }
        self.set_story_notice(match sent {
            Some(Ok(_)) => tr(keys::Q_MEDIA_STORY_SENT).into(),
            Some(Err(_)) => tr(keys::Q_MEDIA_COULD_NOT_SHARE_THE_STORY).into(),
            None => tr(keys::Q_MEDIA_DEMO_SHARING_RUNS_WITH_LIVE).into(),
        });
        self.refresh_story_pause(cx);
    }

    /// Open the close-friends editor on the loaded list.
    pub(super) fn open_close_friends_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let loading = self.live.as_mut().and_then(|live| {
            let _ = live.driver.fetch_contacts();
            live.driver.get_close_friends().ok().flatten()
        });
        if let Some(request) = loading {
            self.stories
                .with_viewer_ui(|open| open.page_op = Some(request));
        }
        let current = self
            .session()
            .and_then(|s| s.stories.close_friends.clone())
            .unwrap_or_default();
        self.stories
            .with_viewer_ui(|open| open.close_friends_edit = Some(CloseFriendsEdit::new(&current)));
        self.stories
            .with_viewer_ui(|open| open.close_friends_saving = false);
        self.stories.with_viewer_ui(|open| open.share_open = false);
        self.stories
            .more_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.refresh_story_pause(cx);
    }

    pub(super) fn toggle_close_friend(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(edit) = self
            .stories
            .viewer_ui_mut()
            .and_then(|open| open.close_friends_edit.as_mut())
        {
            edit.toggle(user_id);
        }
        cx.notify();
    }

    /// Send the edited list with `setCloseFriends`.
    pub(super) fn save_close_friends(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.stories.viewer_ui().close_friends_edit.as_ref() else {
            return;
        };
        if !edit.changed() {
            self.stories
                .with_viewer_ui(|open| open.close_friends_edit = None);
            self.refresh_story_pause(cx);
            return;
        }
        let ids = edit.ids();
        match self.live.as_mut() {
            Some(live) => match live.driver.set_close_friends(&ids) {
                Ok(request) => self.stories.with_viewer_ui(|open| {
                    open.close_friends_saving = true;
                    open.page_op = Some(request);
                }),
                Err(_) => {
                    self.set_story_notice(tr(keys::Q_MEDIA_COULD_NOT_SAVE_CLOSE_FRIENDS).into())
                }
            },
            None => {
                self.set_story_notice(tr(keys::Q_MEDIA_DEMO_SETCLOSEFRIENDS_RUNS_WITH_LIVE).into())
            }
        }
        cx.notify();
    }

    /// A story-page call went out from the viewer: its status line is the
    /// one shown under the story.
    fn note_story_page_op(
        &mut self,
        sent: Option<&Result<quill::ids::RequestId, quill::connect::ConnectSendError>>,
    ) {
        if let Some(Ok(request)) = sent {
            let request = *request;
            self.stories
                .with_viewer_ui(|open| open.page_op = Some(request));
        }
    }

    /// The story-page op the viewer sent for what it shows; any other
    /// (the story page's own, or one sent for a story left behind) stays
    /// off the viewer.
    pub(super) fn viewer_story_page_op(&self) -> Option<quill::story_page::StoryPageOp> {
        let request = self.stories.viewer_ui().page_op?;
        self.session()?.stories.page_ops.get(request).cloned()
    }

    /// Per-tick upkeep of the editor: adopt the loaded list while the user
    /// has not touched it, close after a confirmed save.
    pub(super) fn tick_close_friends(&mut self, cx: &mut Context<Self>) {
        let loaded = self.session().and_then(|s| s.stories.close_friends.clone());
        let failed = self
            .viewer_story_page_op()
            .is_some_and(|op| matches!(op.state, quill::story_page::StoryPageOpState::Failed(_)));
        let Some(loaded) = loaded else {
            return;
        };
        let Some(open) = self.stories.viewer_ui_mut() else {
            return;
        };
        let Some(edit) = open.close_friends_edit.as_mut() else {
            return;
        };
        if open.close_friends_saving {
            if CloseFriendsEdit::new(&loaded).ids() == edit.ids() {
                open.close_friends_saving = false;
                open.close_friends_edit = None;
                open.notice = Some(tr(keys::Q_MEDIA_CLOSE_FRIENDS_UPDATED).into());
                self.refresh_story_pause(cx);
            } else if failed {
                open.close_friends_saving = false;
            }
        } else if !edit.changed() && CloseFriendsEdit::new(&loaded) != *edit {
            *edit = CloseFriendsEdit::new(&loaded);
        }
    }

    /// What the ⋯ menu entries open under the story: the share panel, the
    /// close-friends editor, the notice and the story-page operation.
    pub(super) fn story_more_row(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.stories.viewer.current().is_none() {
            return div().into_any_element();
        }
        let mut column = div().flex().flex_col().gap_2().items_center();
        if self.stories.viewer_ui().share_open {
            column = column.child(self.story_share_panel(cx));
        }
        if self.stories.viewer_ui().close_friends_edit.is_some() {
            column = column.child(self.close_friends_panel(cx));
        }
        if let Some(notice) = self.stories.viewer_ui().notice.clone() {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(chrome::secondary())
                    .child(notice),
            );
        }
        if let Some(op) = self.viewer_story_page_op() {
            use quill::story_page::StoryPageOpState as S;
            match op.state {
                S::Sending | S::Checking => {
                    column = column.child(
                        div()
                            .text_xs()
                            .text_color(chrome::secondary())
                            .child(format!("{}…", op.label)),
                    );
                }
                S::Failed(reason) => {
                    column = column.child(
                        div()
                            .text_xs()
                            .text_color(danger())
                            .child(format!("{} failed: {reason}", op.label)),
                    );
                }
                S::Succeeded => {}
            }
        }
        column.into_any_element()
    }

    /// Chat picker for Share: search box and the first matches.
    fn story_share_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.stories.more_search.read(cx).value().to_string();
        let rows: Vec<(quill::ids::ChatId, String)> = self
            .session()
            .map(|s| {
                s.share_destinations(&query)
                    .into_iter()
                    .take(8)
                    .map(|chat| (chat.id, chat.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut list = div().flex().flex_col().gap_1().max_h(px(160.));
        if rows.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(chrome::secondary())
                    .child(tr(keys::Q_MEDIA_NO_CHATS_FOUND)),
            );
        }
        for (chat_id, title) in rows {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(chrome::fg())
                            .truncate()
                            .child(title),
                    )
                    .child(
                        Button::new(("story-share-send", chat_id.0 as u64))
                            .label(tr(keys::Q_MEDIA_SEND))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.share_story_to(chat_id, cx);
                            })),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w(px(360.))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(chrome::fg())
                    .child(tr(keys::Q_MEDIA_SHARE_STORY_TO)),
            )
            .child(chrome::search_field(
                &self.stories.more_search,
                tr(keys::Q_MEDIA_SEARCH_CHATS).into(),
                "story-share-placeholder",
                cx,
            ))
            .child(list)
            .into_any_element()
    }

    /// The close-friends editor: contact checkboxes, Save and Cancel.
    fn close_friends_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(edit) = self.stories.viewer_ui().close_friends_edit.as_ref() else {
            return div().into_any_element();
        };
        let query = self.stories.more_search.read(cx).value().to_string();
        let rows = self.g1_contact_rows(&query, cx);
        let mut list = div()
            .id("story-close-friends-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(160.))
            .overflow_y_scroll();
        if rows.is_empty() {
            list = list.child(
                div()
                    .text_sm()
                    .text_color(chrome::secondary())
                    .child(tr(keys::Q_MEDIA_NO_CONTACTS_FOUND)),
            );
        }
        for row in rows.iter().take(80) {
            let (user_id, selected) = (row.user_id, edit.contains(row.user_id));
            // The label as a child: the kit's own label takes the app
            // theme's foreground, and this sits on the dark chrome.
            list = list.child(
                Checkbox::new(("story-close-friends-toggle", user_id as u64))
                    .checked(selected)
                    .child(div().text_color(chrome::fg()).child(row.name.clone()))
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        if on != selected {
                            this.toggle_close_friend(user_id, cx);
                        }
                    })),
            );
        }
        let saving = self.stories.viewer_ui().close_friends_saving;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w(px(360.))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(chrome::fg())
                    .child(tr_with(
                        keys::Q_MEDIA_CLOSE_FRIENDS_SELECTED,
                        &[("count", &(edit.count()).to_string())],
                    )),
            )
            .child(chrome::search_field(
                &self.stories.more_search,
                tr(keys::Q_MEDIA_SEARCH_CONTACTS).into(),
                "story-close-friends-placeholder",
                cx,
            ))
            .child(list)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("story-close-friends-save")
                            .label(if saving {
                                tr(keys::Q_MEDIA_SAVING)
                            } else {
                                tr(keys::SETTINGS_SAVE)
                            })
                            .disabled(saving)
                            .on_click(cx.listener(|this, _, _, cx| this.save_close_friends(cx))),
                    )
                    .child(
                        Button::new("story-close-friends-cancel")
                            .label(tr(keys::CANCEL))
                            .custom(chrome::ghost(cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stories
                                    .with_viewer_ui(|open| open.close_friends_edit = None);
                                this.refresh_story_pause(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// The current story's player as an element, with `still` (the
    /// story's thumbnail) under it until the first picture arrives, or
    /// `None` when there is no player. `video_frame` asks the frame clock
    /// for its own frames while the clip plays, so no tick is needed.
    pub(super) fn story_video_element(&self, still: Option<PathBuf>) -> Option<AnyElement> {
        let player = self.stories.video.player()?;
        let (w, h) = (px(360.), px(640.));
        Some(
            div()
                .relative()
                .w(w)
                .h(h)
                .rounded_md()
                .overflow_hidden()
                .bg(chrome::frame_bg())
                .children(still.map(|path| {
                    img(path)
                        .absolute()
                        .inset_0()
                        .w(w)
                        .h(h)
                        .object_fit(ObjectFit::Contain)
                }))
                .child(video_frame(player).object_fit(ObjectFit::Contain))
                .into_any_element(),
        )
    }
}

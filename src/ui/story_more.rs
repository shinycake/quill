//! B14 story viewer extras: native video playback (AVPlayer on macOS, the
//! bundled FFmpeg on Linux and Windows, through `native_video`), mute,
//! press-and-hold / Space pause, the close-friends editor, hide / unhide a
//! peer's stories, share to a chat, copy link, save media and post to
//! profile. The pure parts live in `quill::story_extras`.
//!
//! tdesktop reference: `media/stories/media_stories_controller.cpp`
//! (pause on press, `muted`), `media_stories_view.cpp`,
//! `media_stories_share.cpp` (share box), `boxes/peers/edit_close_friends`.

use super::app::QuillApp;
use super::message_media::file_is_downloading;
use super::native_video::{NativeVideo, Purpose};
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::local_path::sandboxed_display_path;
use quill::story_extras::{
    CloseFriendsEdit, can_save_story, can_share_story, hide_label, profile_label, story_link,
    video_finished, video_progress,
};
use quill::story_viewer::StoryViewerKind;
use quill::telegram::envelope::{ChatKind, StoryListView};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::chat_theme::{danger, text_bright, text_muted};

/// How long a video story waits for its clip to start downloading before
/// the viewer falls back to the thumbnail and the duration clock.
const VIDEO_WAIT: Duration = Duration::from_secs(6);

impl QuillApp {
    /// A text field of the viewer owns the keyboard: Space and the arrows
    /// belong to it, not to playback.
    pub(super) fn story_text_input_open(&self) -> bool {
        self.stories.reply_open
            || self.stories.report_open
            || self.stories.share_open
            || self.stories.close_friends_edit.is_some()
            || self.stories.privacy_edit.is_some()
            || self.stories.cover_target.is_some()
    }

    /// The current video story's clip, once it is a local sandboxed file.
    fn story_video_path(&self) -> Option<PathBuf> {
        let item = self.stories.viewer.current()?;
        let file_id = item.video_file_id?;
        let roots = self.media_display_roots();
        self.session()?
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
            && super::native_video::supported()
            && self.stories.native_failed != self.story_key()
    }

    /// The clip is not local yet: the clock stays frozen, so the story does
    /// not run away while it downloads.
    fn story_video_waiting(&self) -> bool {
        self.story_video_driven() && self.stories.native.borrow().is_none()
    }

    /// Open, replace or drop the native player to match the current story.
    pub(super) fn sync_story_video(&mut self) {
        let key = self.story_key();
        if self.stories.native_key != key {
            *self.stories.native.borrow_mut() = None;
            self.stories.native_key = None;
            self.stories.video_wait_since = Some(Instant::now());
            self.stories.native_paused_by_us = false;
        }
        if !self.story_video_driven() {
            *self.stories.native.borrow_mut() = None;
            self.stories.native_key = None;
            return;
        }
        let failed_now = if let Some(video) = self.stories.native.borrow().as_ref() {
            video.error().is_some()
        } else {
            false
        };
        if failed_now {
            *self.stories.native.borrow_mut() = None;
            self.stories.native_key = None;
            self.stories.native_failed = key;
            self.stories.playback.start(Instant::now());
            return;
        }
        if self.stories.native.borrow().is_some() {
            return;
        }
        if let Some(path) = self.story_video_path() {
            match NativeVideo::open(&path, Purpose::Viewer) {
                Ok(mut video) => {
                    let volume = if self.stories.muted {
                        0.0
                    } else {
                        self.playback_volume
                    };
                    video.set_volume(volume);
                    if !self.story_playback_paused() {
                        video.play();
                        self.stories.native_play_at = Instant::now();
                    } else {
                        self.stories.native_paused_by_us = true;
                    }
                    *self.stories.native.borrow_mut() = Some(video);
                    self.stories.native_key = key;
                }
                Err(_) => {
                    self.stories.native_failed = key;
                    self.stories.playback.start(Instant::now());
                }
            }
        } else {
            // Not local: wait while it downloads, then give up.
            let downloading = self.session().is_some_and(|session| {
                self.stories
                    .viewer
                    .current()
                    .and_then(|item| item.video_file_id)
                    .is_some_and(|id| file_is_downloading(id, &session.files, &session.downloading))
            });
            let waited = self
                .stories
                .video_wait_since
                .is_some_and(|since| since.elapsed() > VIDEO_WAIT);
            if waited && !downloading {
                self.stories.native_failed = key;
                self.stories.playback.start(Instant::now());
            }
        }
    }

    /// Pause or resume the player to match `paused`.
    pub(super) fn apply_story_native_pause(&mut self, paused: bool) {
        let mut slot = self.stories.native.borrow_mut();
        let Some(video) = slot.as_mut() else {
            return;
        };
        if paused {
            if video.is_playing() {
                video.pause();
            }
            self.stories.native_paused_by_us = true;
        } else if self.stories.native_paused_by_us {
            video.play();
            self.stories.native_play_at = Instant::now();
            self.stories.native_paused_by_us = false;
        }
    }

    /// Progress of the current segment: the player's position for video
    /// stories it drives, the duration clock otherwise.
    pub(super) fn story_segment_progress(&self, now: Instant) -> f32 {
        if let Some(video) = self.stories.native.borrow().as_ref() {
            let duration = video
                .duration_secs()
                .or_else(|| {
                    self.stories
                        .viewer
                        .current()
                        .and_then(|item| item.duration_secs)
                        .map(f64::from)
                })
                .unwrap_or(0.0);
            return video_progress(video.position_secs(), duration);
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
        if let Some(video) = self.stories.native.borrow_mut().as_mut() {
            let duration = video
                .duration_secs()
                .or_else(|| {
                    self.stories
                        .viewer
                        .current()
                        .and_then(|item| item.duration_secs)
                        .map(f64::from)
                })
                .unwrap_or(0.0);
            // A just-requested play() has not started yet.
            let playing = self.stories.native_play_at.elapsed() < Duration::from_secs(1)
                || video.is_playing();
            return video_finished(
                video.position_secs(),
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
        let volume = if self.stories.muted {
            0.0
        } else {
            self.playback_volume
        };
        if let Some(video) = self.stories.native.borrow_mut().as_mut() {
            video.set_volume(volume);
        }
        cx.notify();
    }

    /// Space: pause or resume the story.
    pub(super) fn toggle_story_pause(&mut self, cx: &mut Context<Self>) {
        self.stories.pause.toggle();
        self.refresh_story_pause(cx);
    }

    /// Press-and-hold on the story media pauses until release.
    pub(super) fn story_hold(&mut self, held: bool, cx: &mut Context<Self>) {
        if held {
            self.stories.pause.press();
        } else {
            self.stories.pause.release();
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
    fn story_chat_hidden(&self, chat_id: i64) -> bool {
        self.session()
            .and_then(|s| s.story_tray.get(&chat_id))
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
        self.stories.notice = Some(match sent {
            Some(Ok(_)) if archive => "Hiding stories…".into(),
            Some(Ok(_)) => "Showing stories…".into(),
            Some(Err(_)) => "Could not change the stories list".into(),
            None => "demo — setChatActiveStoriesList runs with live TDLib".into(),
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
            .and_then(|s| s.stories.get(&(item.chat_id.0, item.story_id)))
            .is_some_and(|story| story.is_posted_to_chat_page);
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .toggle_story_is_posted_to_chat_page(item.chat_id, item.story_id, !posted)
        });
        self.stories.notice = Some(match sent {
            Some(Ok(_)) if posted => "Removing from profile…".into(),
            Some(Ok(_)) => "Posting to profile…".into(),
            Some(Err(_)) => "Could not change the profile post".into(),
            None => "demo — toggleStoryIsPostedToChatPage runs with live TDLib".into(),
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
                self.stories.notice = Some("Story link copied".into());
            }
            None => self.stories.notice = Some("This story has no public link".into()),
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
            .and_then(|s| s.stories.get(&(item.chat_id.0, item.story_id)))
            .is_some_and(can_save_story);
        if !allowed {
            self.stories.notice = Some("Saving is not allowed for this story".into());
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
                        .files
                        .get(&id.0)
                        .and_then(|file| file.usable_path())
                        .and_then(|path| sandboxed_display_path(path, &roots))
                })
            })
        };
        self.stories.notice = Some(match source {
            Some(path) => match quill::media_viewer::save_media_to_downloads(&path) {
                Ok(saved) => format!("Saved to {}", saved.display()),
                Err(err) => format!("Could not save: {err}"),
            },
            None => "The story is not downloaded yet".into(),
        });
        cx.notify();
    }

    /// Open / close the share panel.
    pub(super) fn toggle_story_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stories.share_open = !self.stories.share_open;
        self.stories.close_friends_edit = None;
        if self.stories.share_open {
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
        let options = self.composer_send_options();
        let sent = self.live.as_mut().map(|live| {
            live.driver
                .share_story_to_chat(dest, item.chat_id, item.story_id, &options)
        });
        self.stories.notice = Some(match sent {
            Some(Ok(_)) => {
                self.stories.share_open = false;
                "Story sent".into()
            }
            Some(Err(_)) => "Could not share the story here".into(),
            None => "demo — sharing runs with live TDLib".into(),
        });
        self.refresh_story_pause(cx);
    }

    /// Open the close-friends editor on the loaded list.
    pub(super) fn open_close_friends_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.get_close_friends();
            let _ = live.driver.fetch_contacts();
        }
        let current = self
            .session()
            .and_then(|s| s.close_friends.clone())
            .unwrap_or_default();
        self.stories.close_friends_edit = Some(CloseFriendsEdit::new(&current));
        self.stories.close_friends_saving = false;
        self.stories.share_open = false;
        self.stories
            .more_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.refresh_story_pause(cx);
    }

    pub(super) fn toggle_close_friend(&mut self, user_id: i64, cx: &mut Context<Self>) {
        if let Some(edit) = self.stories.close_friends_edit.as_mut() {
            edit.toggle(user_id);
        }
        cx.notify();
    }

    /// Send the edited list with `setCloseFriends`.
    pub(super) fn save_close_friends(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.stories.close_friends_edit.as_ref() else {
            return;
        };
        if !edit.changed() {
            self.stories.close_friends_edit = None;
            self.refresh_story_pause(cx);
            return;
        }
        let ids = edit.ids();
        match self.live.as_mut() {
            Some(live) => match live.driver.set_close_friends(&ids) {
                Ok(_) => self.stories.close_friends_saving = true,
                Err(_) => self.stories.notice = Some("Could not save close friends".into()),
            },
            None => {
                self.stories.notice = Some("demo — setCloseFriends runs with live TDLib".into())
            }
        }
        cx.notify();
    }

    /// Per-tick upkeep of the editor: adopt the loaded list while the user
    /// has not touched it, close after a confirmed save.
    pub(super) fn tick_close_friends(&mut self, cx: &mut Context<Self>) {
        let loaded = self.session().and_then(|s| s.close_friends.clone());
        let Some(edit) = self.stories.close_friends_edit.as_mut() else {
            return;
        };
        let Some(loaded) = loaded else {
            return;
        };
        if self.stories.close_friends_saving {
            if CloseFriendsEdit::new(&loaded).ids() == edit.ids() {
                self.stories.close_friends_saving = false;
                self.stories.close_friends_edit = None;
                self.stories.notice = Some("Close friends updated".into());
                self.refresh_story_pause(cx);
            } else if self
                .session()
                .and_then(|s| s.story_page_op.as_ref())
                .is_some_and(|op| {
                    matches!(op.state, quill::story_page::StoryPageOpState::Failed(_))
                })
            {
                self.stories.close_friends_saving = false;
            }
        } else if !edit.changed() && CloseFriendsEdit::new(&loaded) != *edit {
            *edit = CloseFriendsEdit::new(&loaded);
        }
    }

    /// The viewer's extra actions row: sound, share, link, save, hide,
    /// profile. Gated by what TDLib allows for the current story.
    pub(super) fn story_more_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self.stories.viewer.current() else {
            return div().into_any_element();
        };
        let story = self.current_story();
        let can_share = story.as_ref().is_some_and(can_share_story);
        let can_save = story.as_ref().is_some_and(can_save_story);
        let own = story.as_ref().is_some_and(|story| story.can_be_deleted);
        let can_toggle_profile = story
            .as_ref()
            .is_some_and(|story| story.can_toggle_is_posted_to_chat_page);
        let posted = story
            .as_ref()
            .is_some_and(|story| story.is_posted_to_chat_page);
        let hidden = self.story_chat_hidden(item.chat_id.0);
        let is_video = item.kind == StoryViewerKind::Video;
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .items_center()
            .justify_center();
        if is_video {
            let label = if self.stories.muted {
                "Sound off"
            } else {
                "Sound on"
            };
            row = row.child(
                Button::new("story-mute")
                    .label(label)
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_story_mute(cx))),
            );
        }
        row = row.child(
            Button::new("story-pause")
                .label(if self.stories.pause.is_paused() {
                    "Resume"
                } else {
                    "Pause"
                })
                .ghost()
                .text_color(text_bright())
                .on_click(cx.listener(|this, _, _, cx| this.toggle_story_pause(cx))),
        );
        if can_share {
            row = row.child(
                Button::new("story-share")
                    .label("Share")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_story_share(window, cx);
                    })),
            );
        }
        row = row.child(
            Button::new("story-copy-link")
                .label("Copy link")
                .ghost()
                .text_color(text_bright())
                .on_click(cx.listener(|this, _, _, cx| this.copy_story_link(cx))),
        );
        if can_save {
            row = row.child(
                Button::new("story-save-media")
                    .label("Save")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| this.save_story_media(cx))),
            );
        }
        if !own {
            row = row.child(
                Button::new("story-hide-peer")
                    .label(hide_label(hidden))
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_hide_story_peer(cx))),
            );
        }
        if own {
            row = row.child(
                Button::new("story-close-friends")
                    .label("Close friends…")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_close_friends_editor(window, cx);
                    })),
            );
        }
        if can_toggle_profile {
            row = row.child(
                Button::new("story-profile")
                    .label(profile_label(posted))
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_story_profile(cx))),
            );
        }
        let mut column = div().flex().flex_col().gap_2().items_center().child(row);
        if self.stories.share_open {
            column = column.child(self.story_share_panel(cx));
        }
        if self.stories.close_friends_edit.is_some() {
            column = column.child(self.close_friends_panel(cx));
        }
        if let Some(notice) = self.stories.notice.clone() {
            column = column.child(div().text_xs().text_color(text_muted()).child(notice));
        }
        if let Some(op) = self.session().and_then(|s| s.story_page_op.clone()) {
            use quill::story_page::StoryPageOpState as S;
            match op.state {
                S::Sending | S::Checking => {
                    column = column.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
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
                    .text_color(text_muted())
                    .child("No chats found"),
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
                            .text_color(text_bright())
                            .truncate()
                            .child(title),
                    )
                    .child(
                        Button::new(("story-share-send", chat_id.0 as u64))
                            .label("Send")
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
                    .text_color(text_bright())
                    .child("Share story to…"),
            )
            .child(
                Textarea::new(&self.stories.more_search)
                    .aria_label("Search chats")
                    .h(px(32.)),
            )
            .child(list)
            .into_any_element()
    }

    /// The close-friends editor: contact checkboxes, Save and Cancel.
    fn close_friends_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(edit) = self.stories.close_friends_edit.as_ref() else {
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
                    .text_color(text_muted())
                    .child("No contacts found"),
            );
        }
        for row in rows.iter().take(80) {
            list = list.child(self.g1_contact_checkbox(
                "story-close-friends".to_string(),
                row,
                edit.contains(row.user_id),
                row.user_id,
                cx,
            ));
        }
        let saving = self.stories.close_friends_saving;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .w(px(360.))
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(text_bright())
                    .child(format!("Close friends · {} selected", edit.count())),
            )
            .child(
                Textarea::new(&self.stories.more_search)
                    .aria_label("Search contacts")
                    .h(px(32.)),
            )
            .child(list)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("story-close-friends-save")
                            .label(if saving { "Saving…" } else { "Save" })
                            .disabled(saving)
                            .on_click(cx.listener(|this, _, _, cx| this.save_close_friends(cx))),
                    )
                    .child(
                        Button::new("story-close-friends-cancel")
                            .label("Cancel")
                            .ghost()
                            .text_color(text_bright())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stories.close_friends_edit = None;
                                this.refresh_story_pause(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// The live video frame for the current story, as an element, or
    /// `None` when no player has a picture yet (thumbnail shows instead).
    /// Asks the frame clock for the next frame while the clip plays.
    pub(super) fn story_video_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let picture = {
            let mut slot = self.stories.native.borrow_mut();
            slot.as_mut()?.frame()?
        };
        if !self.story_playback_paused() {
            self.request_media_tick(30, cx);
        }
        Some(
            div()
                .w(px(360.))
                .h(px(640.))
                .rounded_md()
                .overflow_hidden()
                .bg(super::chat_theme::bg_deep())
                .child(picture.element(px(360.), px(640.), ObjectFit::Contain, Corners::default()))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    fn video_wait_is_bounded() {
        // The fallback to the thumbnail clock must not wait forever on a
        // clip that never downloads.
        assert!(super::VIDEO_WAIT < Duration::from_secs(30));
    }
}

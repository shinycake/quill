//! Where video userpics play ([`super::userpic_videos`]): the userpic
//! beside a user's messages and a private chat's row in the chat list,
//! the places tdesktop calls `validateVideoUserpic` (history_inner_widget,
//! history_view_list_widget, dialogs_inner_widget). The chat header and
//! search results keep the still, as in tdesktop.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_list_keys::ListRow;
use super::history_row::{RowChrome, SenderAvatar};
use super::userpic_videos::{Surface, UserpicFrame};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use quill::state::UserpicVideo;
use quill::telegram::envelope::{ChatKind, MessageSender};

/// The userpic's side beside messages (`message_chrome`).
pub(super) const HISTORY_USERPIC: f32 = 32.;

impl QuillApp {
    /// `user_id`'s playing clip on `surface`, `size` pt, over `behind`;
    /// `None` keeps the still. A clip still unknown asks for the user's
    /// full info; one not downloaded yet downloads.
    pub(super) fn userpic_frame(
        &self,
        surface: Surface,
        user_id: i64,
        size: f32,
        behind: Hsla,
        cx: &mut Context<Self>,
    ) -> Option<UserpicFrame> {
        if !super::video_player::supported() {
            return None;
        }
        let session = self.session()?;
        let clip = match session.userpic_video(user_id) {
            UserpicVideo::Still => return None,
            UserpicVideo::Unknown => {
                self.defer_userpic_fetch(UserpicFetch::FullInfo(user_id), cx);
                return None;
            }
            UserpicVideo::Clip(clip) => clip,
        };
        let file = session.media.files.get(&clip.0)?;
        let Some(path) = file.usable_path() else {
            if session.should_download(clip) {
                self.defer_userpic_fetch(UserpicFetch::Clip(clip), cx);
            }
            return None;
        };
        let roots = self.media_display_roots();
        let videos = &self.playback.userpic_videos;
        let key = (surface, user_id);
        let player = videos.frame(
            key,
            clip,
            size,
            || quill::local_path::sandboxed_display_path(path, &roots),
            cx,
        )?;
        Some(UserpicFrame::new(videos.clone(), key, player, behind))
    }

    fn defer_userpic_fetch(&self, fetch: UserpicFetch, cx: &mut Context<Self>) {
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = app.update(cx, |this, _| {
                let Some(live) = this.live.as_mut() else {
                    return;
                };
                let _ = match fetch {
                    UserpicFetch::FullInfo(user_id) => live.driver.fetch_userpic_full_info(user_id),
                    UserpicFetch::Clip(file_id) => live.driver.download_file(file_id, 1),
                };
            });
        });
    }

    /// A history row's userpic: the chrome's, with the sender's clip.
    pub(super) fn history_sender_avatar(
        &self,
        chrome: &RowChrome,
        sender: Option<MessageSender>,
        cx: &mut Context<Self>,
    ) -> Option<SenderAvatar> {
        let (name, photo) = chrome.sender_avatar.clone()?;
        let video = match sender {
            Some(MessageSender::User { user_id }) if !name.is_empty() => {
                let behind = self.wallpaper_backdrop(cx);
                self.userpic_frame(Surface::History, user_id, HISTORY_USERPIC, behind, cx)
            }
            _ => None,
        };
        Some(SenderAvatar { name, photo, video })
    }

    /// A chat-list row's clip: a private chat's user, except Saved
    /// Messages. Its corners are cut in the row's fill as of this render
    /// (a hover redraws no row, so a hovered row keeps its resting fill).
    pub(super) fn chat_row_userpic(
        &self,
        chat: ChatId,
        kind: &ChatKind,
        selected: bool,
        saved: bool,
        size: f32,
        cx: &mut Context<Self>,
    ) -> Option<UserpicFrame> {
        let ChatKind::Private { user_id } = kind else {
            return None;
        };
        if saved {
            return None;
        }
        let lit = self.chat_list.keys.list_highlighted(ListRow::Chat(chat));
        let theme = cx.theme();
        let fill = match (selected, lit) {
            (true, true) => super::chat_row::open_row_hover_fill(theme),
            (true, false) => theme.primary.opacity(0.25),
            (false, true) => super::pressable::hover_fill(theme),
            (false, false) => transparent_black(),
        };
        let behind = theme.sidebar.blend(fill);
        self.userpic_frame(Surface::ChatList, user_id.0, size, behind, cx)
    }

    /// At the start of every main-window frame: the clips hold still while
    /// no Quill window is in front or something covers the chat, as
    /// tdesktop pauses them for any `GifPauseReason` (a layer, the
    /// emoji panel, inline results, a round video with sound).
    pub(super) fn sync_userpic_videos(&self, active: bool, scale: f32, history_drawn: bool) {
        let held = !active
            || self.viewer.state.is_open()
            || self.stories.viewer.is_open()
            || self.kit_dialog_open()
            || self.profile_modal_active()
            || self.media_panel_open()
            || self.composer_ui.inline_results_open
            || self.playback.inline_videos.any_sounding();
        self.playback
            .userpic_videos
            .frame_start(held, scale, history_drawn);
    }
}

#[derive(Clone, Copy)]
enum UserpicFetch {
    FullInfo(i64),
    Clip(FileId),
}

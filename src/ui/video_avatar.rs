//! Video profile photos on the info panel's cover, after tdesktop's
//! `VideoUserpicPlayer` (info_profile_top_bar.cpp `updateVideoUserpic`):
//! the shown photo's clip plays muted in a loop from its main frame over
//! the still, which stays until the first picture is decoded.
//!
//! One player at most, for the cover on screen ([`Session::
//! info_cover_animation`] and the panel being laid out). It is dropped,
//! with its decoder, as soon as the panel closes, is hidden by the layout
//! or the clip changes; it pauses while the window is in the background
//! or a layer covers the panel (the media viewer, a story, a dialog),
//! like tdesktop's `isGifPausedAtLeastFor(GifPauseReason::Layer)`. A clip
//! that fails keeps the still for good (`setVideoPlaybackFailed`).
//!
//! The picture is drawn by a view of its own ([`CoverVideo`]), so a new
//! frame redraws that view and the window around it while the slices
//! replay their last frame.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell_layout::PanelPane;
use super::video_player::{Purpose, VideoPlayer, WeakVideoPlayer, video_frame};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use quill::ids::FileId;
use quill::state::PhotoOwner;
use quill::telegram::envelope::ChatPhotoAnimation;
use std::cell::Cell;
use std::rc::Rc;

/// The cover avatar's side in points (user, group and channel panels).
pub(super) const COVER_SIZE: f32 = 96.;

/// The cover's player, if one plays (a `QuillApp` field).
#[derive(Default)]
pub(super) struct VideoAvatarUi {
    clip: Option<CoverClip>,
    /// The clip that failed to open or decode: its still stays.
    failed: Option<FileId>,
}

struct CoverClip {
    owner: PhotoOwner,
    file_id: FileId,
    player: VideoPlayer,
    view: Entity<CoverVideo>,
    /// The color the cover sits on, painted over a GPU surface's corners.
    behind: Rc<Cell<Hsla>>,
}

/// Draws the cover player's picture, cut to a circle.
pub(super) struct CoverVideo {
    player: WeakVideoPlayer,
    behind: Rc<Cell<Hsla>>,
}

impl Render for CoverVideo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let frame = self.player.upgrade().map(|player| {
            video_frame(&player)
                .object_fit(ObjectFit::Cover)
                .corner_radii(Corners::all(px(COVER_SIZE / 2.)), self.behind.get())
        });
        div().id("info-cover-video").size_full().children(frame)
    }
}

impl VideoAvatarUi {
    /// The cover player, while one is open (tests).
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn player(&self) -> Option<&VideoPlayer> {
        self.clip.as_ref().map(|clip| &clip.player)
    }
}

impl QuillApp {
    /// The clip the cover should play now: the open panel's, while the
    /// panel is on screen and clips play here.
    fn wanted_cover_clip(&self) -> Option<(PhotoOwner, ChatPhotoAnimation)> {
        let on_screen = self.profile_modal_active()
            || (self.info_panel_open() && self.shell_layout().panels != PanelPane::Hidden);
        if !on_screen || !super::video_player::supported() {
            return None;
        }
        self.session()?
            .info_cover_animation()
            .filter(|(_, animation)| self.video_avatar.failed != Some(animation.file_id))
    }

    /// Open, keep, pause or drop the cover player to match the state.
    /// Runs at the start of every main-window render; `active` is whether
    /// a Quill window is in front.
    pub(super) fn sync_video_avatar(&mut self, active: bool, scale: f32, cx: &mut Context<Self>) {
        if let Some(clip) = &self.video_avatar.clip
            && clip.player.take_error().is_some()
        {
            self.video_avatar.failed = Some(clip.file_id);
        }
        let Some((owner, animation)) = self.wanted_cover_clip() else {
            self.video_avatar.clip = None;
            return;
        };
        let current = self
            .video_avatar
            .clip
            .as_ref()
            .is_some_and(|clip| clip.owner == owner && clip.file_id == animation.file_id);
        if !current {
            self.video_avatar.clip = self.open_cover_clip(owner, animation, scale, cx);
        }
        let covered =
            self.viewer.state.is_open() || self.stories.viewer.is_open() || self.kit_dialog_open();
        if let Some(clip) = &self.video_avatar.clip {
            let play = active && !covered;
            if play != clip.player.wants_play() {
                if play {
                    clip.player.play();
                } else {
                    clip.player.pause();
                }
            }
        }
    }

    /// A player for `animation` once its file is local (it downloads with
    /// the open chat's thumbnails, `info_cover_file_ids_to_download`).
    fn open_cover_clip(
        &mut self,
        owner: PhotoOwner,
        animation: ChatPhotoAnimation,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> Option<CoverClip> {
        let roots = self.media_display_roots();
        let path = self
            .session()?
            .media
            .files
            .get(&animation.file_id.0)?
            .usable_path()
            .and_then(|path| quill::local_path::sandboxed_display_path(path, &roots))?;
        let max_edge = (COVER_SIZE * scale.max(1.)).ceil() as u32;
        let player = match VideoPlayer::open(&path, Purpose::Inline { max_edge }, None, cx) {
            Ok(player) => player,
            Err(_) => {
                self.video_avatar.failed = Some(animation.file_id);
                return None;
            }
        };
        player.set_volume(0.0);
        player.set_looping(true);
        if animation.start_ms > 0 {
            player.seek(animation.start_secs());
        }
        let behind = Rc::new(Cell::new(Hsla::default()));
        let view = cx.new(|_| CoverVideo {
            player: player.downgrade(),
            behind: behind.clone(),
        });
        Some(CoverClip {
            owner,
            file_id: animation.file_id,
            player,
            view,
            behind,
        })
    }

    /// What the cover sits on: the profile modal's card or the right
    /// column.
    pub(super) fn info_cover_behind(&self, cx: &App) -> Hsla {
        if self.profile_modal_active() {
            super::chat_theme::bg_canvas().into()
        } else {
            cx.theme().sidebar
        }
    }

    /// The cover avatar of `owner`: `still`, with the playing clip over
    /// it when there is one. `behind` is the panel's background.
    pub(super) fn info_cover_avatar(
        &self,
        owner: PhotoOwner,
        still: AnyElement,
        behind: Hsla,
    ) -> AnyElement {
        let video = self
            .video_avatar
            .clip
            .as_ref()
            .filter(|clip| clip.owner == owner)
            .map(|clip| {
                clip.behind.set(behind);
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .child(clip.view.clone())
            });
        div()
            .relative()
            .flex_none()
            .size(px(COVER_SIZE))
            .child(still)
            .children(video)
            .into_any_element()
    }
}

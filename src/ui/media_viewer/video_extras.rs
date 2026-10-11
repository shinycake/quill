//! Video extras of the viewer: rotate the picture, share a post at the
//! playback position, and pick a video quality (tdesktop's
//! `playbackControlsRotate`, `shareAtTime` and `playbackControlsQualities`).

use super::*;
use quill::media_viewer::{VideoQualityOption, current_quality};

/// A decoded frame turned to the viewer's orientation, cached for the
/// source frame it was made from.
pub(in crate::ui) struct OrientedFrame {
    source: usize,
    code: u8,
    image: Arc<RenderImage>,
}

/// Turn a BGRA/RGBA `RenderImage` frame to `orientation`. Channel order
/// does not matter: pixels move as four-byte units.
fn orient_render_image(
    frame: &RenderImage,
    orientation: ViewerOrientation,
) -> Option<Arc<RenderImage>> {
    let size = frame.size(0);
    let (width, height) = (size.width.0 as u32, size.height.0 as u32);
    let bytes = frame.as_bytes(0)?;
    let (pixels, width, height) = orient_rgba(bytes, width, height, orientation);
    let image = image::RgbaImage::from_raw(width, height, pixels)?;
    Some(Arc::new(RenderImage::new(SmallVec::from_buf([
        image::Frame::new(image),
    ]))))
}

/// Seconds shown and linked by "Share at": the whole seconds played.
pub(super) fn share_position_secs(elapsed: f64) -> i32 {
    elapsed.max(0.0) as i32
}

impl QuillApp {
    /// Rotating needs pictures this app draws itself: the decoded frames
    /// (every platform's fallback player, and GIFs). The macOS system
    /// player draws into its own surface and cannot be turned.
    pub(in crate::ui) fn viewer_video_rotatable(&self) -> bool {
        self.viewer.state.current().is_some_and(|item| {
            item.kind.is_playable()
                && self.viewer.native.is_none()
                && !self.viewer.video_frames.is_empty()
        })
    }

    /// "Rotate": a quarter turn of the playing video (tdesktop turns by
    /// 90 degrees each time and remembers it per file).
    pub(in crate::ui) fn rotate_viewer_video(&mut self, cx: &mut Context<Self>) {
        if !self.viewer_video_rotatable() {
            self.connection.status_note = "this video can't be rotated".into();
            cx.notify();
            return;
        }
        self.viewer.orientation.rotate_cw();
        self.viewer.extra.oriented_frame = None;
        cx.notify();
    }

    /// The frame on screen, turned to the viewer's orientation.
    pub(in crate::ui) fn viewer_oriented_frame(&mut self) -> Option<Arc<RenderImage>> {
        let frame = self.viewer_render_frame()?;
        let orientation = self.viewer.orientation;
        if orientation.is_identity() {
            return Some(frame);
        }
        let source = Arc::as_ptr(&frame) as usize;
        let code = orientation.code();
        if let Some(cached) = &self.viewer.extra.oriented_frame
            && cached.source == source
            && cached.code == code
        {
            return Some(cached.image.clone());
        }
        let image = orient_render_image(&frame, orientation)?;
        self.viewer.extra.oriented_frame = Some(OrientedFrame {
            source,
            code,
            image: image.clone(),
        });
        Some(image)
    }

    /// The playback position of the playing video, in whole seconds.
    pub(in crate::ui) fn viewer_share_position(&self) -> i32 {
        share_position_secs(
            self.viewer
                .clock
                .as_ref()
                .map_or(0.0, |clock| clock.elapsed_secs()),
        )
    }

    /// "Share at 1:23": tdesktop offers it on a video message that can be
    /// forwarded (not on a link-preview video or a profile photo).
    pub(in crate::ui) fn viewer_can_share_at_time(&self) -> bool {
        self.viewer.state.source() == ViewerSource::Chat
            && self
                .viewer
                .state
                .current()
                .is_some_and(|item| item.kind == MediaViewerKind::Video)
    }

    /// Pause the video, open the forward picker on this message and keep
    /// the position, so the picker's "Copy link" links to that second.
    pub(in crate::ui) fn share_viewer_video_at_time(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.viewer.state.current() else {
            return;
        };
        let (chat_id, message_id) = (item.chat_id, item.message_id);
        let secs = self.viewer_share_position();
        if self.refuse_protected_copy(chat_id, cx) {
            return;
        }
        if self
            .viewer
            .clock
            .as_ref()
            .is_some_and(|clock| clock.is_playing())
        {
            self.pause_viewer_video(cx);
        }
        self.close_media_viewer(cx);
        self.begin_forward_one(chat_id, message_id, false, window, cx);
        self.viewer.extra.share_at = Some((chat_id, message_id, secs));
    }

    /// The picker's "Copy link" for the message the viewer was shared at a
    /// position from: the link carries that position. Returns whether it
    /// handled the request (the plain link is used otherwise).
    pub(in crate::ui) fn share_viewer_link_at_time(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((_, _, secs)) = self
            .viewer
            .extra
            .share_at
            .take_if(|(chat, message, _)| *chat == chat_id && *message == message_id)
        else {
            return false;
        };
        self.connection.status_note = match self.live.as_mut() {
            Some(live) => match live.driver.get_message_link_at(chat_id, message_id, secs) {
                Ok(_) => "fetching message link…".into(),
                Err(_) => "could not get message link".into(),
            },
            None => "no live connection".into(),
        };
        cx.notify();
        true
    }

    /// The saved quality, applied to the video that just became current
    /// (tdesktop reads `Core::App().settings().videoQuality()` per video).
    pub(in crate::ui) fn apply_viewer_quality_pref(&mut self) {
        let preferred = self.saved_video_quality();
        self.viewer.state.apply_video_quality(preferred);
    }

    fn saved_video_quality(&self) -> u32 {
        self.session()
            .map_or(0, |session| session.settings.media_prefs.video_quality)
    }

    /// Choose a quality (0 is Auto), remember it, and restart the video
    /// from where it was when the file to play changes.
    pub(in crate::ui) fn set_viewer_video_quality(&mut self, height: u32, cx: &mut Context<Self>) {
        self.set_media_pref(|prefs| prefs.video_quality = height, cx);
        let elapsed = self.viewer.clock.as_ref().map(|clock| clock.elapsed_secs());
        if !self.viewer.state.apply_video_quality(height) {
            return;
        }
        if let (Some(item), Some(elapsed)) = (self.viewer.state.current(), elapsed) {
            self.viewer.pending_seek = Some((item.message_id, elapsed));
        }
        let orientation = self.viewer.orientation;
        self.reset_viewer_item_state(cx);
        self.viewer.orientation = orientation;
        cx.notify();
    }

    /// The quality button and its list, for a video with more than one
    /// rendition.
    pub(in crate::ui) fn viewer_quality_picker(
        &mut self,
        item: &MediaViewerItem,
        row_id: u64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if item.quality_options.len() < 2 {
            return None;
        }
        let saved = self.saved_video_quality();
        let current: Option<VideoQualityOption> = current_quality(item).cloned();
        let options = item.quality_options.clone();
        let button_label = current.as_ref().map_or_else(
            || "Auto".to_string(),
            |option| format!("{}p", option.height),
        );
        let view = cx.entity().downgrade();
        Some(
            Popover::new(("media-viewer-quality", row_id as usize))
                .anchor(Anchor::BottomRight)
                .trigger(
                    Button::new(("media-viewer-quality-button", row_id as usize))
                        .label(button_label)
                        .ghost()
                        .text_color(gpui_kit::white())
                        .tooltip("Video quality")
                        .accessibility_label("Video quality"),
                )
                .content(move |_state, _window, cx| {
                    let popover = cx.entity();
                    let mut rows: Vec<AnyElement> = Vec::new();
                    let mut push = |id: usize, label: String, chosen: bool, height: u32| {
                        let view = view.clone();
                        let popover = popover.clone();
                        rows.push(
                            Button::new(("media-viewer-quality-row", id))
                                .label(label)
                                .ghost()
                                .small()
                                .w_full()
                                .when(chosen, |button| {
                                    button.icon(gpui_kit::assets::IconName::Check)
                                })
                                .on_click(move |_, window, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.set_viewer_video_quality(height, cx);
                                    });
                                    popover.update(cx, |state, cx| state.dismiss(window, cx));
                                })
                                .into_any_element(),
                        );
                    };
                    push(0, "Auto".to_string(), saved == 0, 0);
                    for option in &options {
                        push(
                            option.height as usize,
                            option.label(),
                            saved != 0
                                && current
                                    .as_ref()
                                    .is_some_and(|now| now.file_id == option.file_id),
                            option.height,
                        );
                    }
                    div().w(px(200.)).flex().flex_col().gap_1().children(rows)
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{orient_render_image, share_position_secs};
    use gpui_kit::RenderImage;
    use quill::media_viewer::ViewerOrientation;
    use smallvec::SmallVec;

    #[test]
    fn share_position_is_whole_seconds_never_negative() {
        assert_eq!(share_position_secs(12.9), 12);
        assert_eq!(share_position_secs(0.0), 0);
        assert_eq!(share_position_secs(-3.0), 0);
    }

    #[test]
    fn rotation_state_cycles_through_four_turns() {
        let mut orientation = ViewerOrientation::default();
        assert!(orientation.is_identity());
        orientation.rotate_cw();
        assert!(orientation.swaps_axes());
        orientation.rotate_cw();
        assert!(!orientation.swaps_axes());
        assert_eq!(orientation.turns, 2);
        orientation.rotate_cw();
        orientation.rotate_cw();
        assert!(orientation.is_identity());
    }

    #[test]
    fn a_turned_frame_swaps_its_axes() {
        let mut orientation = ViewerOrientation::default();
        orientation.rotate_cw();
        let source = image::RgbaImage::from_raw(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let frame = RenderImage::new(SmallVec::from_buf([image::Frame::new(source)]));
        let turned = orient_render_image(&frame, orientation).unwrap();
        let size = turned.size(0);
        assert_eq!((size.width.0, size.height.0), (1, 2));
        // Clockwise: the left pixel ends on top.
        assert_eq!(turned.as_bytes(0).unwrap(), &[1, 2, 3, 4, 5, 6, 7, 8]);
    }
}

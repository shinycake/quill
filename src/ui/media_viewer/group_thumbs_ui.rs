//! The thumbnail strip of an album or a profile photo gallery, the bottom
//! row of the viewer's bottom band (tdesktop `GroupThumbs`).
//! `quill::media_viewer::group_strip` lays it out from the viewer state;
//! this draws it, slides it to a new current thumb, opens the item of a
//! clicked one, and fetches the profile photo thumbs it shows.
#![deny(clippy::indexing_slicing)]

use super::geometry::{CAPTION_GAP, lane_width};
use super::*;
use crate::ui::image_budget::{Fit, sized_media};
use quill::connect::THUMB_DOWNLOAD_PRIORITY;
use quill::media_viewer::{
    GROUP_STRIP_PADDING, GROUP_THUMB_HEIGHT, GROUP_THUMB_SLIDE_MS, GROUP_THUMB_WIDTH_MAX,
    GroupStrip, ThumbFrame, strip_frames, thumb_file_ids,
};

/// What a thumb shows until its picture is local (or if it fails to load).
const PLACEHOLDER: u32 = 0x1f1f1f;

/// What a thumb draws: its picture, if local, and the item's kind.
#[derive(Clone)]
struct ThumbPicture {
    path: Option<PathBuf>,
    natural: Option<(i32, i32)>,
    label: &'static str,
}

impl QuillApp {
    /// The current item's album strip at this window width. `None` outside
    /// an album, for an item alone in its album, and in video full screen.
    pub(super) fn viewer_group_strip(&self, viewport_w: f32) -> Option<GroupStrip> {
        if self.viewer.extra.video_fullscreen.is_on() {
            return None;
        }
        self.viewer.state.group_strip(lane_width(viewport_w))
    }

    /// The strip row for the bottom band, or `None` without one. After a
    /// step it slides from the strip of the item the step left.
    pub(super) fn viewer_group_thumbs(
        &self,
        viewport_w: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let strip = self.viewer_group_strip(viewport_w)?;
        let from = self
            .viewer
            .state
            .group_strip_before(lane_width(viewport_w))
            .filter(|from| from.group == strip.group);
        let step = self.viewer.state.steps() as usize;
        let roots = self.media_display_roots();
        let files = self.session().map(|session| &session.media.files);
        let items = self.viewer.state.items();
        let positions = strip
            .thumbs
            .iter()
            .chain(from.iter().flat_map(|from| from.thumbs.iter()))
            .map(|thumb| thumb.position);
        let mut pictures: HashMap<usize, ThumbPicture> = HashMap::new();
        for position in positions {
            let Some(item) = items.get(strip.start.saturating_add(position)) else {
                continue;
            };
            pictures.entry(position).or_insert_with(|| ThumbPicture {
                path: files.and_then(|files| thumb_path(item, files, &roots)),
                natural: item.natural_size,
                label: item.kind.label(),
            });
        }
        // The current thumb sits on the strip's centre, so the row is as
        // wide on both sides as the farther edge.
        let (min, max) = strip.extent();
        let half = (-min).max(max);
        let view = cx.entity().downgrade();
        let row = div()
            .id("media-viewer-group-thumbs")
            .debug_selector(|| "media-viewer-group-thumbs".into())
            .on_mouse_move(Self::over_controls_listener(cx))
            .occlude()
            .relative()
            .mt(px(GROUP_STRIP_PADDING - CAPTION_GAP))
            .w(px(2.0 * half))
            .h(px(GROUP_THUMB_HEIGHT))
            .with_animation(
                ("viewer-group-strip", step),
                Animation::new(Duration::from_millis(GROUP_THUMB_SLIDE_MS)),
                move |row, t| {
                    let t = if still_frame() { 1.0 } else { t };
                    let frames = strip_frames(from.as_ref(), &strip, t);
                    row.children(frames.into_iter().filter_map(|frame| {
                        let picture = pictures.get(&frame.position)?;
                        let index = strip.start.saturating_add(frame.position);
                        Some(thumb(frame, half, picture, index, view.clone()))
                    }))
                },
            );
        Some(row.into_any_element())
    }

    /// Ask for the profile photo thumbs the strip shows that are not local
    /// yet, at the width the last frame laid it out in. Opening, a step
    /// and the poll loop call it; the driver drops what is local or on its
    /// way.
    pub(in crate::ui) fn download_viewer_strip_thumbs(&mut self) {
        let Some(lane) = self.viewer.extra.strip_lane else {
            return;
        };
        let wanted = self.viewer.state.strip_downloads(lane);
        let Some(live) = self.live.as_mut() else {
            return;
        };
        for file_id in wanted {
            if live
                .driver
                .download_file(file_id, THUMB_DOWNLOAD_PRIORITY)
                .is_err()
            {
                break;
            }
        }
    }

    /// A click on a thumb: open its item, as the arrows do.
    fn open_viewer_group_item(&mut self, index: usize, cx: &mut Context<Self>) {
        self.viewer.state.go_to(index);
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }
}

/// The local picture a thumb draws, if any (`thumb_file_ids` order).
fn thumb_path(
    item: &MediaViewerItem,
    files: &quill::state::FileMap,
    roots: &[PathBuf],
) -> Option<PathBuf> {
    thumb_file_ids(item).into_iter().find_map(|id| {
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, roots))
    })
}

/// One thumb at its place in the row (`half` from the row's left edge to
/// the strip's centre): a cover crop of the picture, or the placeholder.
fn thumb(
    frame: ThumbFrame,
    half: f32,
    picture: &ThumbPicture,
    index: usize,
    view: WeakEntity<QuillApp>,
) -> AnyElement {
    let n = frame.position + 1;
    let image = picture.path.as_deref().map(|path| {
        img(sized_media(
            path,
            (px(GROUP_THUMB_WIDTH_MAX), px(GROUP_THUMB_HEIGHT)),
            picture.natural,
            Fit::Cover,
        ))
        .size_full()
        .object_fit(ObjectFit::Cover)
    });
    let label = picture.label;
    div()
        .id(("media-viewer-group-thumb", frame.position))
        .debug_selector(move || format!("media-viewer-group-thumb-{n}"))
        .absolute()
        .top_0()
        .left(px(half + frame.left))
        .w(px(frame.width))
        .h(px(GROUP_THUMB_HEIGHT))
        .overflow_hidden()
        .bg(rgb(PLACEHOLDER))
        .opacity(frame.opacity)
        .children(image)
        .when(frame.opens, |thumb| {
            thumb
                .role(Role::Button)
                .aria_label(label)
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.open_viewer_group_item(index, cx));
                })
        })
        .into_any_element()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "group_thumbs_ui_tests.rs"]
mod group_thumbs_ui_tests;

//! One photo or video tile of an album mosaic, drawn like tdesktop's
//! grouped parts (`Photo::drawGrouped`, `Gif::drawGrouped`): the picture
//! (the blurred inline preview until the file is local), a video's
//! duration and play disc, a download disc while a photo is remote, and the
//! spoiler cover until it is revealed. A click opens the viewer, which
//! downloads and plays the item.

use super::super::message_media::{
    MediaCorners, MediaDisc, download_fraction, fading_spoiler, file_is_downloading, media_disc,
    photo_display_path, spoiler_cover, video_badge,
};
use super::super::spoiler_fx::MediaCover;
use super::*;
use quill::lang::{keys, tr};
use quill::voice::format_voice_duration;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "video_spoiler_tests.rs"]
mod tests;

pub(in crate::ui) fn album_tile(
    message: &HistoryMessage,
    part: &quill::album::AlbumRect,
    mosaic: (i32, i32),
    mosaic_corners: MediaCorners,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    // Which item of the message the tile is (a bought paid-media album
    // holds several under one message id); 0 for ordinary albums.
    item_offset: usize,
    // What a press on the tile that moves picks up, when it may.
    drag: Option<quill::forward_drag::ForwardDrag>,
    // The tile's spoiler cover, while it shows (`spoiler_fx::media_cover`).
    cover: Option<MediaCover>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = (message.id.0 as u64)
        .wrapping_mul(64)
        .wrapping_add(item_offset as u64);
    let corners = mosaic_corners.tile(
        part.x <= 0,
        part.y <= 0,
        part.x + part.width >= mosaic.0,
        part.y + part.height >= mosaic.1,
    );
    let (chat_id, message_id) = (message.chat_id, message.id);
    let size = (px(part.width as f32), px(part.height as f32));
    let covered = matches!(cover, Some(MediaCover::Covered));
    let frame = div()
        .id(("album-tile", row_id))
        .test_support()
        .relative()
        .w(size.0)
        .h(size.1)
        .map(|this| corners.round(this))
        .overflow_hidden();
    let tile = if covered {
        frame
            .child(spoiler_cover(
                row_id,
                chat_id,
                message_id,
                &message.content,
                files,
                downloading,
                size,
                corners,
                cx,
            ))
            .into_any_element()
    } else {
        let media = plain_tile(
            message,
            row_id,
            size,
            corners,
            files,
            downloading,
            media_roots,
        );
        let media = match cover {
            Some(MediaCover::Fading(opacity)) => {
                // Built outside the animation layer, like a single row's.
                let cover = super::super::anim_layer::with_layer(None, || {
                    spoiler_cover(
                        row_id,
                        chat_id,
                        message_id,
                        &message.content,
                        files,
                        downloading,
                        size,
                        corners,
                        cx,
                    )
                });
                fading_spoiler(media, cover, opacity)
            }
            _ => media,
        };
        frame
            .role(gpui_kit::Role::Button)
            .aria_label(tr(keys::Q_MEDIA_OPEN_ALBUM_MEDIA))
            .tab_index(0)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_media_viewer_at(chat_id, message_id, item_offset, cx);
            }))
            .child(media)
            .into_any_element()
    };
    // A press on a tile that moves picks up that one message (`GroupPart`),
    // not the album; the tile's own click stays on `frame`, inside.
    let spot = div()
        .id(("album-drag", row_id))
        .absolute()
        .left(px(part.x as f32))
        .top(px(part.y as f32))
        .w(size.0)
        .h(size.1)
        .child(tile);
    super::forward_drag_source::draggable(spot, drag).into_any_element()
}

/// The tile's picture and marks with no spoiler over them.
fn plain_tile(
    message: &HistoryMessage,
    row_id: u64,
    (w, h): (Pixels, Pixels),
    corners: MediaCorners,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
) -> AnyElement {
    let local_path = |ids: &[FileId]| {
        ids.iter().filter(|id| id.0 != 0).find_map(|id| {
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    };
    let (path, dims, minithumbnail, disc, duration) = match &message.content {
        MessageContent::Photo(photo) => {
            let open_id = photo.open_file_id().unwrap_or(FileId(0));
            let path = photo_display_path(photo, files, media_roots);
            let disc = path.is_none().then(|| {
                if file_is_downloading(open_id, files, downloading) {
                    MediaDisc::Progress(download_fraction(open_id, files))
                } else {
                    MediaDisc::Download
                }
            });
            let dims = photo
                .largest_size()
                .or_else(|| photo.thumb_size())
                .map(|size| (size.width, size.height));
            (path, dims, photo.minithumbnail.as_ref(), disc, None)
        }
        MessageContent::Video(video) => {
            let play_id = video.play_file_id().unwrap_or(FileId(0));
            let thumb_id = video.thumb_file_id().unwrap_or(FileId(0));
            let disc = if file_is_downloading(play_id, files, downloading) {
                MediaDisc::Progress(download_fraction(play_id, files))
            } else {
                MediaDisc::Play
            };
            (
                local_path(&[thumb_id, play_id]),
                Some((video.width, video.height)),
                video.minithumbnail.as_ref(),
                Some(disc),
                Some(format_voice_duration(video.duration)),
            )
        }
        _ => return div().into_any_element(),
    };
    let picture = path.map(|path| {
        img(super::super::image_budget::sized_media(
            &path,
            (w, h),
            dims,
            super::super::image_budget::Fit::Cover,
        ))
        .id(("album-picture", row_id))
        .absolute()
        .inset_0()
        .w(w)
        .h(h)
        .map(|this| corners.round(this))
        .object_fit(ObjectFit::Cover)
    });
    // The inline preview, soft like a blur, until the picture is local.
    let preview = minithumbnail
        .filter(|mini| !mini.data.is_empty())
        .filter(|_| picture.is_none())
        .map(|mini| {
            img(super::super::mini_image::mini_image(mini))
                .absolute()
                .inset_0()
                .size_full()
                .map(|this| corners.round(this))
                .object_fit(ObjectFit::Cover)
        });
    div()
        .relative()
        .w(w)
        .h(h)
        .map(|this| corners.round(this))
        .bg(fill_muted())
        .children(preview)
        .children(picture)
        .children(duration.map(|duration| video_badge(duration, false)))
        .children(disc.map(|disc| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(media_disc(("album-disc", row_id), disc).test_support())
        }))
        .into_any_element()
}

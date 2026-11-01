//! Photo, GIF and video attachments.

use super::*;
use quill::lang::{keys, tr};

pub(in crate::ui) fn photo_attachment(
    row_id: u64,
    photo: &quill::telegram::envelope::PhotoContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    sponsored: Option<(ChatId, i64)>,
    // Phase 4.5: `(chat_id, message_id)` when a click should open the
    // fullscreen viewer (history rows only; album tiles and sponsored rows
    // pass `None`).
    viewer: Option<(ChatId, MessageId)>,
    corners: MediaCorners,
    // The caption's longest line (0 without one): a wide caption widens
    // the picture, as in Telegram Desktop (`media_frame_for`).
    caption_width: i32,
    // The widest the bubble lets the picture be (`BubbleRoom::media`).
    width_max: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let open_id = photo.open_file_id().unwrap_or(FileId(0));
    let (frame_w, frame_h) = photo
        .largest_size()
        .or_else(|| photo.thumb_size())
        .map(|size| {
            media_frame_for(
                MediaFrameKind::Photo,
                size.width,
                size.height,
                caption_width,
                width_max,
            )
        })
        .unwrap_or_else(|| media_frame_for(MediaFrameKind::Photo, 0, 0, caption_width, width_max));
    if !photo.is_secret
        && !photo.has_spoiler
        && let Some(path) = photo_display_path(photo, files, media_roots)
    {
        let dims = photo
            .largest_size()
            .or_else(|| photo.thumb_size())
            .map(|size| (size.width, size.height));
        return img(crate::ui::image_budget::sized_media(
            &path,
            (frame_w, frame_h),
            dims,
            crate::ui::image_budget::Fit::Cover,
        ))
        .id(("photo-img", row_id))
        .debug_selector(move || format!("photo-{row_id}"))
        .w(frame_w)
        .h(frame_h)
        .aspect_ratio(frame_w / frame_h)
        .map(|this| corners.round(this))
        .object_fit(ObjectFit::Cover)
        .when_some(viewer, |this, (chat_id, message_id)| {
            this.role(gpui_kit::Role::Button)
                .aria_label(tr(keys::Q_MISC_OPEN_PHOTO))
                .tab_index(0)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_media_viewer(chat_id, message_id, cx);
                }))
        })
        .with_fallback(move || {
            div()
                .w(frame_w)
                .h(frame_h)
                .map(|this| corners.round(this))
                .bg(fill_muted())
                .flex()
                .items_center()
                .justify_center()
                .child(tr(keys::Q_MISC_PHOTO))
                .into_any_element()
        })
        .into_any_element();
    }
    let downloading_now = file_is_downloading(open_id, files, downloading);
    let ready = files
        .get(&open_id.0)
        .and_then(|f| f.usable_path())
        .is_some();
    // Secret and spoiler photos keep a text label; the rest show a disc.
    let status = photo.placeholder_label(downloading_now, ready);
    let viewable = !photo.is_secret && !photo.has_spoiler;
    // A secret photo opens the viewer on click when it has a self-destruct
    // timer (the viewer decides; view-once media stays closed).
    let viewer_open = (viewable || (photo.is_secret && !photo.has_spoiler))
        .then_some(viewer)
        .flatten();
    let has_viewer_open = viewer_open.is_some();
    // The inline minithumbnail, scaled to the frame, previews the picture
    // (soft, like a blur) while the real size downloads. Secret and
    // spoiler photos never reveal it.
    let preview = viewable
        .then_some(photo.minithumbnail.as_ref())
        .flatten()
        .filter(|mini| !mini.data.is_empty())
        .map(|mini| {
            img(crate::ui::mini_image::mini_image(mini))
                .absolute()
                .inset_0()
                .size_full()
                .map(|this| corners.round(this))
                .object_fit(ObjectFit::Cover)
        });
    let has_preview = preview.is_some();
    div()
        .id(("photo-ph", row_id))
        .debug_selector(move || format!("photo-{row_id}"))
        .relative()
        .overflow_hidden()
        .w(frame_w)
        .h(frame_h)
        .map(|this| corners.round(this))
        .bg(fill_muted())
        .flex()
        .items_center()
        .justify_center()
        .children(preview)
        // Phase 4.5: viewable photos open the viewer (it triggers the
        // download when needed); spoiler photos keep the old
        // click-to-download placeholder, secret photos stay inert.
        .when_some(viewer_open, |this, (chat_id, message_id)| {
            this.role(gpui_kit::Role::Button)
                .aria_label(tr(keys::Q_MISC_OPEN_PHOTO))
                .tab_index(0)
                .cursor_pointer()
                .pressable(cx.theme())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_media_viewer(chat_id, message_id, cx);
                }))
        })
        .when(
            !has_viewer_open && photo.click_requests_download(),
            |this| {
                this.role(gpui_kit::Role::Button)
                    .aria_label(tr(keys::Q_MISC_DOWNLOAD_PHOTO))
                    .tab_index(0)
                    .cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_media_download(open_id, sponsored, cx);
                    }))
            },
        )
        .map(|this| {
            if viewable {
                this.child(media_disc(
                    ("photo-disc", row_id),
                    if downloading_now {
                        MediaDisc::Progress(download_fraction(open_id, files))
                    } else {
                        MediaDisc::Download
                    },
                ))
            } else {
                this.child(
                    div()
                        .text_xs()
                        .when(has_preview, |this| {
                            // Over the preview: a legible pill.
                            this.px_2()
                                .py_0p5()
                                .rounded_full()
                                .bg(gpui_kit::black().opacity(0.45))
                                .text_color(gpui_kit::white())
                        })
                        .when(!has_preview, |this| this.text_color(text_bright()))
                        .child(status),
                )
            }
        })
        .into_any_element()
}

pub(in crate::ui) fn animation_attachment(
    message_id: MessageId,
    animation: &quill::telegram::envelope::AnimationContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    playing: bool,
    // The `image/gif` file looping in the row (`gif_clip`).
    gif: Option<crate::ui::gif_clip::GifPlay>,
    inline: Option<crate::ui::inline_video::InlineFrame>,
    sponsored: Option<(ChatId, i64)>,
    // `(chat_id, message_id)` when a click should open the fullscreen
    // viewer, which plays the GIF in a loop (history rows only).
    viewer: Option<(ChatId, MessageId)>,
    corners: MediaCorners,
    // The caption's longest line (0 without one); see `media_frame_for`.
    caption_width: i32,
    width_max: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = animation.play_file_id().unwrap_or(FileId(0));
    let thumb_id = animation.thumb_file_id().unwrap_or(FileId(0));
    let mime = animation.mime_type.clone();
    let play_label = if playing {
        tr(keys::Q_MISC_PAUSE)
    } else {
        tr(keys::Q_MISC_PLAY)
    };
    let gif = gif.filter(|_| playing);
    // The GIF and the controls over it are drawn by the animation layer.
    let gif_live = gif.is_some() && !(animation.is_secret || animation.has_spoiler);
    let visual = [thumb_id, play_id].into_iter().find_map(|id| {
        if id.0 == 0 {
            return None;
        }
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, media_roots))
            .map(|path| {
                crate::ui::image_budget::sized_media(
                    &path,
                    media_frame_for(
                        MediaFrameKind::Gif,
                        animation.width,
                        animation.height,
                        caption_width,
                        width_max,
                    ),
                    Some((animation.width, animation.height)),
                    crate::ui::image_budget::Fit::Cover,
                )
            })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = animation.is_secret || animation.has_spoiler;
    let (frame_w, frame_h) = media_frame_for(
        MediaFrameKind::Gif,
        animation.width,
        animation.height,
        caption_width,
        width_max,
    );
    let live = inline.is_some();
    let still = move |path| {
        img(path)
            .id(("gif-img", row_id))
            .w(frame_w)
            .h(frame_h)
            .aspect_ratio(frame_w / frame_h)
            .map(|this| corners.round(this))
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || {
                div()
                    .w(frame_w)
                    .h(frame_h)
                    .map(|this| corners.round(this))
                    .bg(accent_strong())
                    .into_any_element()
            })
            .into_any_element()
    };
    let picture = if let Some(inline) = inline {
        // The badge rides with the clip: an animation layer redraws both.
        // The still shows under it until the first picture.
        let under = visual.filter(|_| !blocked).map(still);
        live_tile(inline, under, (frame_w, frame_h), move |frame| {
            div()
                .relative()
                .w(frame_w)
                .h(frame_h)
                .child(inline_surface(frame, frame_w, frame_h, corners))
                .child(gif_badge())
                .into_any_element()
        })
    } else if !blocked && let Some(gif) = gif {
        // The looping GIF: drawn by the conversation's animation layer at
        // its own frame cadence while the row renders, the slice keeping
        // the picture it rendered with underneath. The badge and the Pause
        // disc (shown on hover; the row's own disc under it takes the
        // click) ride with it, since the layer paints over the row. The
        // layer's copy takes no hover, so it is told; the slice's reads
        // the group.
        let draw = move |frame: Arc<RenderImage>, hovered: Option<bool>| {
            div()
                .relative()
                .w(frame_w)
                .h(frame_h)
                .child(
                    img(ImageSource::Render(frame))
                        .w(frame_w)
                        .h(frame_h)
                        .map(|this| corners.round(this))
                        .object_fit(ObjectFit::Cover),
                )
                .child(gif_badge())
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child({
                            let disc = media_disc(("gif-disc-shown", row_id), MediaDisc::Pause);
                            match hovered {
                                Some(true) => disc,
                                Some(false) => disc.invisible(),
                                None => disc
                                    .invisible()
                                    .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible()),
                            }
                        }),
                )
                .into_any_element()
        };
        let under = match gif.frame_now() {
            Some(frame) => draw(frame, None),
            None => visual.map_or_else(|| div().into_any_element(), still),
        };
        let fps = gif.fps();
        crate::ui::anim_layer::hover_tile(under, fps, move |hovered| {
            gif.frame_now().map(|frame| draw(frame, Some(hovered)))
        })
        .into_any_element()
    } else if !blocked && let Some(path) = visual {
        still(path)
    } else {
        div()
            .id(("gif-ph", row_id))
            .w(frame_w)
            .h(frame_h)
            .map(|this| corners.round(this))
            .bg(fill_muted())
            .into_any_element()
    };
    let gif_disc = if !playing && file_is_downloading(play_id, files, downloading) {
        MediaDisc::Cancel(download_fraction(play_id, files))
    } else if downloading_now && !playing {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else if playing {
        MediaDisc::Pause
    } else {
        MediaDisc::Play
    };
    div()
        .id(("gif-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .id(("gif-visual", row_id))
                .relative()
                .when_some(
                    (!blocked).then_some(viewer).flatten(),
                    |this, (chat_id, message_id)| {
                        this.role(gpui_kit::Role::Button)
                            .aria_label(tr(keys::Q_MISC_OPEN_GIF))
                            .tab_index(0)
                            .cursor_pointer()
                            .pressable(cx.theme())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_media_viewer(chat_id, message_id, cx);
                            }))
                    },
                )
                .group(MEDIA_VISUAL_GROUP)
                .child(picture)
                .when(!live && !gif_live, |this| this.child(gif_badge()))
                // Autoplaying GIFs show no play control (Telegram Desktop).
                .when(!blocked && !live, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                media_disc(("gif-disc", row_id), gif_disc)
                                    .role(gpui_kit::Role::Button)
                                    .aria_label(play_label)
                                    .cursor_pointer()
                                    .map(|disc| match (playing, gif_live) {
                                        // Drawn with the GIF; this one
                                        // only takes the click.
                                        (true, true) => disc.opacity(0.),
                                        (true, false) => disc
                                            .invisible()
                                            .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible()),
                                        (false, _) => disc,
                                    })
                                    // The press bubbles, so the frame's drag can start
                                    // from the disc; a click on the disc stops here and
                                    // does not also open the viewer.
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        if let Some((chat_id, sponsored_id)) = sponsored {
                                            this.click_sponsored_message(
                                                chat_id,
                                                sponsored_id,
                                                true,
                                                cx,
                                            );
                                        }
                                        let mime = mime.clone();
                                        this.press_media(play_id, cx, |this, cx| {
                                            this.toggle_animation_playback(
                                                message_id, play_id, mime, cx,
                                            );
                                        });
                                    })),
                            ),
                    )
                }),
        )
        .into_any_element()
}

pub(in crate::ui) fn video_attachment(
    message_id: MessageId,
    video: &quill::telegram::envelope::VideoContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    inline: Option<crate::ui::inline_video::InlineFrame>,
    sponsored: Option<(ChatId, i64)>,
    // Phase 4.5: `(chat_id, message_id)` when a click should open the
    // fullscreen viewer (history rows only; sponsored rows pass `None`).
    // Secret/spoiler videos never get the handler.
    viewer: Option<(ChatId, MessageId)>,
    corners: MediaCorners,
    // The caption's longest line (0 without one); see `media_frame_for`.
    caption_width: i32,
    width_max: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = video.play_file_id().unwrap_or(FileId(0));
    let thumb_id = video.thumb_file_id().unwrap_or(FileId(0));
    let play_label = tr(keys::Q_MISC_PLAY);
    // Autoplaying: the time left and a muted mark (Telegram Desktop).
    let live_remaining = inline.as_ref().map(|inline| inline.remaining_secs);
    let duration = match live_remaining {
        Some(Some(left)) => format_voice_duration(left.ceil() as i32),
        _ => format_voice_duration(video.duration),
    };
    let visual = [thumb_id, play_id].into_iter().find_map(|id| {
        if id.0 == 0 {
            return None;
        }
        files
            .get(&id.0)
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, media_roots))
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = video.is_secret || video.has_spoiler;
    let (frame_w, frame_h) = media_frame_for(
        MediaFrameKind::Video,
        video.width,
        video.height,
        caption_width,
        width_max,
    );
    let live = inline.is_some();
    let total = video.duration;
    let (width, height) = (video.width, video.height);
    let still = move |path: PathBuf| {
        img(crate::ui::image_budget::sized_media(
            &path,
            (frame_w, frame_h),
            Some((width, height)),
            crate::ui::image_budget::Fit::Cover,
        ))
        .id(("video-img", row_id))
        .w(frame_w)
        .h(frame_h)
        .aspect_ratio(frame_w / frame_h)
        .map(|this| corners.round(this))
        .object_fit(ObjectFit::Cover)
        .with_fallback(move || {
            div()
                .w(frame_w)
                .h(frame_h)
                .map(|this| corners.round(this))
                .bg(success_bg())
                .into_any_element()
        })
        .into_any_element()
    };
    let picture = if let Some(inline) = inline {
        // The time left counts down with the clip: an animation layer
        // redraws the badge with it. The still shows under it until the
        // first picture.
        let under = visual.clone().filter(|_| !blocked).map(still);
        live_tile(inline, under, (frame_w, frame_h), move |frame| {
            let left = match frame.remaining_secs {
                Some(left) => format_voice_duration(left.ceil() as i32),
                None => format_voice_duration(total),
            };
            div()
                .relative()
                .w(frame_w)
                .h(frame_h)
                .child(inline_surface(frame, frame_w, frame_h, corners))
                .child(video_badge(left, true))
                .into_any_element()
        })
    } else if !blocked && let Some(path) = visual {
        still(path)
    } else {
        div()
            .id(("video-ph", row_id))
            .w(frame_w)
            .h(frame_h)
            .map(|this| corners.round(this))
            .bg(fill_muted())
            .flex()
            .items_center()
            .justify_center()
            .when(blocked, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(text_muted())
                        .child(tr(keys::MEDIA_VIDEO)),
                )
            })
            .into_any_element()
    };
    let video_disc = if downloading_now {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else {
        MediaDisc::Play
    };
    div()
        .id(("video-row", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .child({
            // Phase 4.5: clicking the visual opens the viewer (the viewer
            // triggers the download when nothing is local yet). The
            // Play/Pause button below keeps its own handler.
            let viewer_open = (!blocked || (video.is_secret && !video.has_spoiler))
                .then_some(viewer)
                .flatten();
            div()
                .id(("video-visual", row_id))
                .debug_selector(move || format!("video-{row_id}"))
                .relative()
                .when_some(viewer_open, |this, (chat_id, message_id)| {
                    this.role(gpui_kit::Role::Button)
                        .aria_label(tr(keys::Q_MISC_OPEN_VIDEO))
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_media_viewer(chat_id, message_id, cx);
                        }))
                })
                .group(MEDIA_VISUAL_GROUP)
                .child(picture)
                .when(!live, |this| this.child(video_badge(duration, false)))
                .when(!blocked && !live, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                media_disc(("video-disc", row_id), video_disc)
                                    .role(gpui_kit::Role::Button)
                                    .aria_label(play_label)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        if let Some((chat_id, sponsored_id)) = sponsored {
                                            this.click_sponsored_message(
                                                chat_id,
                                                sponsored_id,
                                                true,
                                                cx,
                                            );
                                        }
                                        // Videos play in the media viewer
                                        // (Telegram Desktop's behavior).
                                        if let Some((chat_id, message_id)) = viewer {
                                            this.open_media_viewer(chat_id, message_id, cx);
                                        }
                                    })),
                            ),
                    )
                })
        })
        .into_any_element()
}

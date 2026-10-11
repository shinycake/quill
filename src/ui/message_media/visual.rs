//! Photo, GIF and video attachments.

use super::*;

pub(in crate::ui) fn photo_attachment(
    row_id: u64,
    photo: &quill::telegram::envelope::PhotoContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
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
            )
        })
        .unwrap_or_else(|| media_frame_for(MediaFrameKind::Photo, 0, 0, caption_width));
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
        .w(frame_w)
        .h(frame_h)
        .aspect_ratio(frame_w / frame_h)
        .map(|this| corners.round(this))
        .object_fit(ObjectFit::Cover)
        .when_some(viewer, |this, (chat_id, message_id)| {
            this.role(gpui_kit::Role::Button)
                .aria_label("Open photo")
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
                .child("Photo")
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
    let viewer_open = viewable.then_some(viewer).flatten();
    let has_viewer_open = viewer_open.is_some();
    // The inline minithumbnail, scaled to the frame, previews the picture
    // (soft, like a blur) while the real size downloads. Secret and
    // spoiler photos never reveal it.
    let preview = viewable
        .then_some(photo.minithumbnail.as_ref())
        .flatten()
        .filter(|mini| !mini.data.is_empty())
        .map(|mini| {
            img(ImageSource::Image(Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Jpeg,
                mini.data.clone(),
            ))))
            .absolute()
            .inset_0()
            .size_full()
            .map(|this| corners.round(this))
            .object_fit(ObjectFit::Cover)
        });
    let has_preview = preview.is_some();
    div()
        .id(("photo-ph", row_id))
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
                .aria_label("Open photo")
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
                    .aria_label("Download photo")
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
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<Arc<RenderImage>>,
    inline: Option<crate::ui::inline_video::InlineFrame>,
    sponsored: Option<(ChatId, i64)>,
    // `(chat_id, message_id)` when a click should open the fullscreen
    // viewer, which plays the GIF in a loop (history rows only).
    viewer: Option<(ChatId, MessageId)>,
    corners: MediaCorners,
    // The caption's longest line (0 without one); see `media_frame_for`.
    caption_width: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = animation.play_file_id().unwrap_or(FileId(0));
    let thumb_id = animation.thumb_file_id().unwrap_or(FileId(0));
    let mime = animation.mime_type.clone();
    let play_label = if playing { "Pause" } else { "Play" };
    let visual = if playing {
        frame.map(ImageSource::Render)
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
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
                        ),
                        Some((animation.width, animation.height)),
                        crate::ui::image_budget::Fit::Cover,
                    )
                })
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
    );
    let live = inline.is_some();
    let picture = if let Some(inline) = inline {
        // The badge rides with the clip: an animation layer redraws both.
        live_tile(inline, move |frame| {
            div()
                .relative()
                .w(frame_w)
                .h(frame_h)
                .child(inline_surface(frame, frame_w, frame_h, corners))
                .child(gif_badge())
                .into_any_element()
        })
    } else if !blocked && let Some(path) = visual {
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
    } else {
        div()
            .id(("gif-ph", row_id))
            .w(frame_w)
            .h(frame_h)
            .map(|this| corners.round(this))
            .bg(fill_muted())
            .into_any_element()
    };
    let gif_disc = if downloading_now && !playing {
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
                            .aria_label("Open GIF")
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
                .when(!live, |this| this.child(gif_badge()))
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
                                    .when(playing, |disc| {
                                        disc.invisible()
                                            .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible())
                                    })
                                    // Press on the disc must not start the frame's click (open viewer).
                                    .swallow_press()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some((chat_id, sponsored_id)) = sponsored {
                                            this.click_sponsored_message(
                                                chat_id,
                                                sponsored_id,
                                                true,
                                                cx,
                                            );
                                        }
                                        this.toggle_animation_playback(
                                            message_id,
                                            play_id,
                                            mime.clone(),
                                            cx,
                                        );
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
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    inline: Option<crate::ui::inline_video::InlineFrame>,
    sponsored: Option<(ChatId, i64)>,
    // Phase 4.5: `(chat_id, message_id)` when a click should open the
    // fullscreen viewer (history rows only; sponsored rows pass `None`).
    // Secret/spoiler videos never get the handler.
    viewer: Option<(ChatId, MessageId)>,
    corners: MediaCorners,
    // The caption's longest line (0 without one); see `media_frame_for`.
    caption_width: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = video.play_file_id().unwrap_or(FileId(0));
    let thumb_id = video.thumb_file_id().unwrap_or(FileId(0));
    let mime = video.mime_type.clone();
    let start_timestamp = video.start_timestamp;
    let play_label = if playing { "Pause" } else { "Play" };
    // Autoplaying: the time left and a muted mark (Telegram Desktop).
    let live_remaining = inline.as_ref().map(|inline| inline.remaining_secs);
    let duration = match live_remaining {
        Some(Some(left)) => format_voice_duration(left.ceil() as i32),
        _ => format_voice_duration(video.duration),
    };
    let visual = if playing {
        frame.and_then(|path| sandboxed_display_path(&path.to_string_lossy(), media_roots))
    } else {
        None
    };
    let visual = visual.or_else(|| {
        [thumb_id, play_id].into_iter().find_map(|id| {
            if id.0 == 0 {
                return None;
            }
            files
                .get(&id.0)
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, media_roots))
        })
    });
    let downloading_now = file_is_downloading(play_id, files, downloading)
        || file_is_downloading(thumb_id, files, downloading);
    let blocked = video.is_secret || video.has_spoiler;
    let (frame_w, frame_h) = media_frame_for(
        MediaFrameKind::Video,
        video.width,
        video.height,
        caption_width,
    );
    let live = inline.is_some();
    let total = video.duration;
    let picture = if let Some(inline) = inline {
        // The time left counts down with the clip: an animation layer
        // redraws the badge with it.
        live_tile(inline, move |frame| {
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
        img(crate::ui::image_budget::sized_media(
            &path,
            (frame_w, frame_h),
            Some((video.width, video.height)),
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
                this.child(div().text_xs().text_color(text_muted()).child("Video"))
            })
            .into_any_element()
    };
    let video_disc = if downloading_now && !playing {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else if playing {
        MediaDisc::Pause
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
            let viewer_open = (!blocked).then_some(viewer).flatten();
            div()
                .id(("video-visual", row_id))
                .relative()
                .when_some(viewer_open, |this, (chat_id, message_id)| {
                    this.role(gpui_kit::Role::Button)
                        .aria_label("Open video")
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
                                    .when(playing, |disc| {
                                        disc.invisible()
                                            .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible())
                                    })
                                    // Press on the disc must not start the frame's click (open viewer).
                                    .swallow_press()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some((chat_id, sponsored_id)) = sponsored {
                                            this.click_sponsored_message(
                                                chat_id,
                                                sponsored_id,
                                                true,
                                                cx,
                                            );
                                        }
                                        // With the native player, videos play in
                                        // the media viewer (Telegram Desktop's
                                        // behavior); inline frames otherwise.
                                        if crate::ui::native_video::supported()
                                            && let Some((chat_id, message_id)) = viewer
                                        {
                                            this.open_media_viewer(chat_id, message_id, cx);
                                            return;
                                        }
                                        this.toggle_video_playback(
                                            message_id,
                                            play_id,
                                            mime.clone(),
                                            start_timestamp,
                                            None,
                                            cx,
                                        );
                                    })),
                            ),
                    )
                })
        })
        .into_any_element()
}

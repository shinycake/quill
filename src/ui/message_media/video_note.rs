//! Round video messages.

use super::*;

/// Round video message diameter.
/// Telegram Desktop `maxVideoMessageSize`.
pub(in crate::ui) const VIDEO_NOTE_DIAMETER: f32 =
    quill::bubble_layout::MAX_VIDEO_MESSAGE_SIZE as f32;

pub(in crate::ui) fn video_note_attachment(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VideoNoteContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    playing: bool,
    frame: Option<&std::path::Path>,
    inline: Option<crate::ui::inline_video::InlineFrame>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row_id = message_id.0 as u64;
    let play_id = note.play_file_id().unwrap_or(FileId(0));
    let thumb_id = note.thumb_file_id().unwrap_or(FileId(0));
    // Playing with sound: the time left (Telegram Desktop).
    let duration = match inline.as_ref() {
        Some(inline) if inline.sound => {
            format_voice_duration(inline.remaining_secs.unwrap_or(0.0).ceil() as i32)
        }
        _ => format_voice_duration(note.duration),
    };
    let live = inline.is_some();
    let sounding = inline.as_ref().is_some_and(|inline| inline.sound);
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
    let blocked = note.is_secret;
    let unseen = !outgoing && !note.is_viewed && !playing && !sounding;
    let has_visual = !blocked && (live || visual.is_some());
    // tdesktop's seek ring over a round video playing with sound.
    let seek_ring = inline.as_ref().filter(|inline| inline.sound).map(|inline| {
        crate::ui::round_seek::round_seek_overlay(
            chat_id.0,
            message_id.0,
            inline.progress.unwrap_or(0.),
            inline.seek_shown,
            inline.seek_grabbed,
            cx,
        )
    });
    // A muted loop drawn by the conversation's animation layer: the ring
    // and the time ride with the video.
    let layered = inline.as_ref().is_some_and(|inline| inline.live.is_some());
    let ring_color = cx.theme().primary;
    let picture = if let Some(inline) = inline {
        let duration = duration.clone();
        live_tile(inline, move |frame| {
            let video = round_inline_surface(frame);
            if !layered {
                return video;
            }
            div()
                .relative()
                .size(px(VIDEO_NOTE_DIAMETER))
                .child(video)
                .when(unseen, |this| this.child(unseen_ring(ring_color)))
                .child(note_duration_badge(duration.clone(), unseen))
                .into_any_element()
        })
    } else {
        match visual.filter(|_| !blocked) {
            // GPUI clips overflow to rectangles: the image and the placeholder
            // round themselves.
            Some(path) => img(path)
                .id(("video-note-img", row_id))
                .size_full()
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .with_fallback(|| {
                    div()
                        .size_full()
                        .rounded_full()
                        .bg(fill_muted())
                        .into_any_element()
                })
                .into_any_element(),
            None => div()
                .size_full()
                .rounded_full()
                .bg(fill_muted())
                .flex()
                .items_center()
                .justify_center()
                .when(blocked, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(text_muted())
                            .child("Video message"),
                    )
                })
                .into_any_element(),
        }
    };
    let disc = if downloading_now && !has_visual {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else if playing {
        MediaDisc::Pause
    } else {
        MediaDisc::Play
    };
    let viewed = note.is_viewed;
    let toggle = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
        if blocked {
            return;
        }
        // A muted inline loop: play it once with sound (Telegram Desktop).
        if live {
            if this
                .inline_videos
                .borrow_mut()
                .toggle_sound(chat_id.0, message_id.0)
            {
                if !viewed {
                    this.mark_voice_opened(chat_id, message_id);
                }
                cx.notify();
            }
            return;
        }
        this.toggle_video_playback(
            message_id,
            play_id,
            "video/mp4".into(),
            0,
            if viewed { None } else { Some(chat_id) },
            cx,
        );
    };
    let diameter = px(VIDEO_NOTE_DIAMETER);
    div()
        .id(("video-note", row_id))
        .flex()
        .flex_col()
        .gap_1()
        .when(outgoing, |this| this.items_end())
        .child(
            div()
                .id(("video-note-circle", row_id))
                .relative()
                .size(diameter)
                .rounded_full()
                .overflow_hidden()
                .group(MEDIA_VISUAL_GROUP)
                // An unseen incoming note carries an accent ring.
                .when(unseen && !live, |this| {
                    this.border_2().border_color(cx.theme().primary)
                })
                .role(gpui_kit::Role::Button)
                .aria_label(if playing || sounding {
                    "Pause video message"
                } else {
                    "Play video message"
                })
                .tab_index(0)
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| toggle(this, cx)))
                .child(picture)
                .children(seek_ring)
                // Over the video's mask, so the ring stays visible.
                .when(unseen && live && !layered, |this| {
                    this.child(unseen_ring(ring_color))
                })
                // A looping round video shows no play control.
                .when(!blocked && !live, |this| {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(media_disc(("video-note-disc", row_id), disc).when(
                                playing,
                                |disc| {
                                    disc.invisible()
                                        .group_hover(MEDIA_VISUAL_GROUP, |s| s.visible())
                                },
                            )),
                    )
                })
                .when(!layered, |this| {
                    this.child(note_duration_badge(duration, unseen))
                }),
        )
        // MED2: transcription under the play button (schema 1.8.67
        // `speechRecognitionResult` on `videoNote`).
        .child(transcription_row(
            chat_id,
            message_id,
            &note.transcription,
            cx.theme().primary,
            cx,
        ))
        .into_any_element()
}

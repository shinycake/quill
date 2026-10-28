//! Round video messages.

use super::*;
use quill::lang::{keys, tr};

/// Round video message diameter.
/// Telegram Desktop `maxVideoMessageSize`.
pub(in crate::ui) const VIDEO_NOTE_DIAMETER: f32 =
    quill::bubble_layout::MAX_VIDEO_MESSAGE_SIZE as f32;

pub(in crate::ui) fn video_note_attachment(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VideoNoteContent,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
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
    let blocked = note.is_secret;
    let unseen = !outgoing && !note.is_viewed && !sounding;
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
    // The thumbnail, a circle of its own.
    let still = move |path: PathBuf| {
        img(path)
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
            .into_any_element()
    };
    let diameter = px(VIDEO_NOTE_DIAMETER);
    let picture = if let Some(inline) = inline {
        let duration = duration.clone();
        // The thumbnail shows under the video until its first picture.
        let under = visual.clone().filter(|_| !blocked).map(still);
        live_tile(inline, under, (diameter, diameter), move |frame| {
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
            Some(path) => still(path),
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
                            .child(tr(keys::IN_DLG_VIDEO_MESSAGE)),
                    )
                })
                .into_any_element(),
        }
    };
    // Over the thumbnail too: a press while the clip downloads cancels it.
    let clip_downloading = file_is_downloading(play_id, files, downloading);
    let disc = if clip_downloading {
        MediaDisc::Cancel(download_fraction(play_id, files))
    } else if downloading_now && !has_visual {
        MediaDisc::Progress(download_fraction(play_id, files))
    } else {
        MediaDisc::Play
    };
    // A muted inline loop plays once with sound (Telegram Desktop); one
    // not playing inline (autoplay off, data saver, not downloaded) plays
    // with sound on demand, downloaded first if need be.
    let clicked = note.clone();
    let toggle = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
        if !blocked {
            this.play_video_note(chat_id, message_id, &clicked, cx);
        }
    };
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
                .aria_label(if sounding {
                    tr(keys::Q_AV_PAUSE_VIDEO_MESSAGE)
                } else {
                    tr(keys::Q_AV_PLAY_VIDEO_MESSAGE)
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
                            .child(media_disc(("video-note-disc", row_id), disc)),
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

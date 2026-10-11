//! Voice notes, music files, waveforms, seek bars and speech
//! transcription.

use super::*;

/// Round video note. tdesktop paints `history/view/media` round video as a
/// circle (JPEG thumb, duration, play). Quill uses the same ffmpeg frames as
/// `messageVideo`, clipped to a circle. Diameter on screen is fixed; schema
/// `length` is the sender's pixel size, shown when the file is not local yet.
/// MED2: the transcription row under a voice/video note. When TDLib has
/// delivered a `speechRecognitionResult` (via `updateMessageContent`), the
/// transcript shows; otherwise the row offers a real `recognizeSpeech`
/// request — pending/error states are shown honestly, never faked.
pub(in crate::ui) fn transcription_row(
    chat_id: ChatId,
    message_id: MessageId,
    transcription: &Option<SpeechRecognition>,
    accent: Hsla,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let row = message_id.0 as u64;
    match transcription {
        None => div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                inline_link(("transcribe", row), "Transcribe", accent)
                    .tooltip(|window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(
                            "Send speech-recognition request to Telegram",
                        )
                        .build(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.request_transcription(chat_id, message_id, cx);
                    })),
            )
            .children(
                cx.try_global::<crate::ui::updates_sync_ui::SpeechTrialHint>()
                    .and_then(|hint| hint.0.clone())
                    .map(|hint| div().text_xs().opacity(0.7).child(hint)),
            )
            .into_any_element(),
        Some(SpeechRecognition::Pending { partial_text }) => div()
            .text_xs()
            .opacity(0.7)
            .child(if partial_text.is_empty() {
                "Transcribing…".to_string()
            } else {
                format!("Transcribing… {partial_text}")
            })
            .into_any_element(),
        Some(SpeechRecognition::Text { text }) => div()
            .text_sm()
            .child(format!("“{text}”"))
            .into_any_element(),
        Some(SpeechRecognition::Error { message }) => div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .text_xs()
            .child(
                div()
                    .opacity(0.7)
                    .child(format!("Transcription failed: {message}")),
            )
            .child(
                inline_link(("transcribe-retry", row), "Retry", accent).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.request_transcription(chat_id, message_id, cx);
                    },
                )),
            )
            .into_any_element(),
    }
}

/// A voice note's waveform across the whole row, as Telegram Desktop
/// paints it (`PaintWaveform`): `msgWaveformBar` bars `msgWaveformSkip`
/// apart, `msgWaveformMin` to `msgWaveformMax` tall, laid out at paint
/// time over the width the row gives (`bubble_width::waveform_bars`). The
/// bars left of `progress` (0..=1) take the full color, the rest fade, so
/// the waveform doubles as the progress indicator.
pub(in crate::ui) fn waveform_canvas(
    bars: Vec<u8>,
    color: Hsla,
    progress: f32,
) -> impl IntoElement {
    use crate::ui::history::bubble_width::{WAVEFORM_HEIGHT, waveform_bars};
    use quill::bubble_layout::WAVEFORM_BAR;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let available = f32::from(bounds.size.width).floor() as i32;
            let active_width = (available as f32 * progress.clamp(0., 1.)).round();
            let faded = color.opacity(0.35);
            for bar in waveform_bars(&bars, available) {
                let lit = (bar.left as f32) < active_width;
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.origin.x + px(bar.left as f32),
                            bounds.origin.y + px(bar.top),
                        ),
                        size(px(WAVEFORM_BAR as f32), px(bar.height as f32)),
                    ),
                    if lit { color } else { faded },
                ));
            }
        },
    )
    .w_full()
    .h(px(WAVEFORM_HEIGHT as f32))
}

/// Voice waveform bars at a fixed pitch (the record bar); the first
/// `played` fraction is drawn solid, the rest faded. Bubbles use
/// [`waveform_canvas`], which fills its row.
pub(in crate::ui) fn waveform_row(
    row_key: u64,
    bars: &[u8],
    color: Hsla,
    played: f32,
) -> impl IntoElement {
    let mut row = div()
        .id(("waveform", row_key))
        .flex()
        .items_center()
        .gap(px(2.))
        .h(px(24.));
    let shown: Vec<u8> = if bars.is_empty() {
        vec![6, 10, 14, 8, 12]
    } else {
        bars.iter().copied().take(48).collect()
    };
    let count = shown.len().max(1) as f32;
    for (index, bar) in shown.into_iter().enumerate() {
        let h = 3.0 + f32::from(bar.min(31)) * 0.65;
        let lit = (index as f32 + 0.5) / count <= played;
        row = row.child(
            div()
                .id(("wave-bar", row_key * 64 + index as u64))
                .w(px(2.))
                .h(px(h))
                .rounded_full()
                .bg(if lit { color } else { color.opacity(0.35) }),
        );
    }
    row
}

/// Phase 4.6 seek bar (tdesktop-style): the interactive gpui-component
/// `Slider` on the active row — click-to-seek and drag, with the UI layer
/// seeking the sound to the released offset — and a static
/// track + fill on every other audio/voice row.
pub(in crate::ui) fn seek_bar_element(row_key: u64, seek: &SeekBarView, color: Hsla) -> AnyElement {
    if let Some(slider) = &seek.slider {
        div()
            .id(("seek-bar", row_key))
            .role(gpui_kit::Role::Group)
            .aria_label("Playback position")
            .w_full()
            .child(Slider::new(slider).bg(color).text_color(color))
            .into_any_element()
    } else {
        div()
            .id(("seek-bar", row_key))
            .w_full()
            .h(px(3.))
            .rounded_full()
            .bg(color.opacity(0.25))
            .child(
                div()
                    .h_full()
                    .w(relative(seek.fraction() as f32))
                    .rounded_full()
                    .bg(color),
            )
            .into_any_element()
    }
}

pub(in crate::ui) fn voice_note_row(
    chat_id: ChatId,
    message_id: MessageId,
    outgoing: bool,
    note: &quill::telegram::envelope::VoiceNoteContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    seek: &SeekBarView,
    // The bubble look (plain bubbles have no padding to take off the
    // row's width) and the time footer's width, 0 when hidden.
    plain: bool,
    info_width: i32,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = note.file_id;
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let bars = voice::waveform_bars_from_bytes(&note.waveform);
    let listened = note.is_listened;
    let note_duration = f64::from(note.duration);
    let active = seek.slider.is_some();
    let accent = bubble_accent(outgoing, cx);
    // Phase 4.6: the active row shows elapsed / total (tdesktop-style).
    let total = format_voice_duration(note.duration);
    // The widest the status line gets ("played / total").
    let widest_status = format!("{total} / {total}");
    let meta = if downloading_now && !seek.is_playing {
        "Downloading…".to_string()
    } else if active {
        format!(
            "{} / {total}",
            format_voice_duration(seek.display_secs as i32)
        )
    } else {
        total
    };
    let unheard = !outgoing && !note.is_listened && !active;
    let (icon, label, ring) = if seek.is_playing {
        (gpui_kit::assets::IconName::Pause, "Pause", None)
    } else if downloading_now {
        (
            gpui_kit::assets::IconName::X,
            "Downloading",
            Some(download_fraction(file_id, files)),
        )
    } else {
        (gpui_kit::assets::IconName::Play, "Play voice message", None)
    };
    let row_key = message_id.0 as u64;
    // Telegram Desktop: an unplayed incoming note shows its whole
    // waveform in the active color; otherwise the played part does.
    let progress = if unheard { 1. } else { seek.fraction() as f32 };
    // The row is as wide as a full waveform (`Document::countOptimalSize`
    // for a voice note); the status label is the widest it gets.
    let status_width = crate::ui::history::bubble_width::longest_line_width(
        cx,
        &widest_status,
        px(12.),
        FontWeight::NORMAL,
        0,
    );
    let width = crate::ui::history::bubble_width::voice_row_width(status_width, info_width, plain);
    div()
        .id(("voice-note", row_key))
        .mt_1()
        .flex()
        .flex_col()
        .gap_1()
        .w(width)
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    action_disc(("voice-play", row_key), outgoing, icon, ring, label, cx).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_voice_playback(
                                chat_id,
                                message_id,
                                file_id,
                                listened,
                                note_duration,
                                cx,
                            );
                        }),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(waveform_canvas(bars, accent, progress))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .child(div().opacity(0.7).child(meta))
                                // Unheard incoming note: a small accent dot.
                                .when(unheard, |this| {
                                    this.child(div().size(px(6.)).rounded_full().bg(accent))
                                }),
                        ),
                ),
        )
        .when(active, |this| {
            this.child(seek_bar_element(row_key, seek, accent))
                // MED1: speed + mute on the active row.
                .child(row_playback_controls(row_key, "voice", seek, accent, cx))
        })
        // MED2: transcription (schema 1.8.67 `speechRecognitionResult`
        // on `voiceNote`).
        .child(transcription_row(
            chat_id,
            message_id,
            &note.transcription,
            accent,
            cx,
        ))
        .into_any_element()
}

pub(in crate::ui) fn audio_row(
    message_id: MessageId,
    outgoing: bool,
    audio: &quill::telegram::envelope::AudioContent,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    media_roots: &[PathBuf],
    seek: &SeekBarView,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = audio.file_id;
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let audio_duration = f64::from(audio.duration);
    let active = seek.slider.is_some();
    let accent = bubble_accent(outgoing, cx);
    let title = if !audio.title.is_empty() {
        audio.title.clone()
    } else if !audio.file_name.is_empty() {
        audio.file_name.clone()
    } else {
        "Audio".to_string()
    };
    // Phase 4.6: the active row shows elapsed / total (tdesktop-style).
    let total = voice::format_voice_duration(audio.duration);
    let duration_label = if active {
        format!(
            "{} / {total}",
            voice::format_voice_duration(seek.display_secs as i32)
        )
    } else {
        total
    };
    let meta = if downloading_now && !seek.is_playing {
        "Downloading…".to_string()
    } else if audio.performer.is_empty() {
        duration_label
    } else {
        format!("{} · {duration_label}", audio.performer)
    };
    let (icon, label, ring) = if seek.is_playing {
        (gpui_kit::assets::IconName::Pause, "Pause", None)
    } else if downloading_now {
        (
            gpui_kit::assets::IconName::X,
            "Downloading",
            Some(download_fraction(file_id, files)),
        )
    } else {
        (gpui_kit::assets::IconName::Play, "Play", None)
    };
    let row_key = message_id.0 as u64;
    let cover_id = audio.cover_file_id().unwrap_or(FileId(0));
    let cover = files
        .get(&cover_id.0)
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    let play = cx.listener(move |this, _, _, cx| {
        if let Some(chat_id) = this.open_chat_id() {
            this.toggle_audio_playback(chat_id, message_id, file_id, audio_duration, cx);
        }
    });
    // Album art, when there is one, carries the play glyph on a scrim;
    // otherwise the plain accent disc.
    let disc = match cover {
        Some(path) => div()
            .id(("audio-play", row_key))
            .relative()
            .size(px(44.))
            .flex_none()
            .rounded_md()
            .overflow_hidden()
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .cursor_pointer()
            .child(
                img(crate::ui::image_budget::sized_media(
                    &path,
                    (px(44.), px(44.)),
                    None,
                    crate::ui::image_budget::Fit::Cover,
                ))
                .id(("audio-cover", row_key))
                .size_full()
                .object_fit(ObjectFit::Cover),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black().opacity(0.35))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(icon).size(px(20.)).text_color(gpui_kit::white())),
            )
            .on_click(play),
        None => {
            action_disc(("audio-play", row_key), outgoing, icon, ring, label, cx).on_click(play)
        }
    };
    // `msgFileMinWidth` to `msgMaxWidth`; the title widens the row between.
    let (min_width, max_width) = crate::ui::history::bubble_width::file_row_bounds(false);
    div()
        .id(("audio", row_key))
        .mt_1()
        .flex()
        .flex_col()
        .gap_1()
        .min_w(min_width)
        .max_w(max_width)
        .child(
            div().flex().items_center().gap_3().child(disc).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_sm().font_medium().truncate().child(title))
                    .child(div().text_xs().opacity(0.7).truncate().child(meta)),
            ),
        )
        .when(active, |this| {
            this.child(seek_bar_element(row_key, seek, accent))
                // MED1: speed + mute on the active row.
                .child(row_playback_controls(row_key, "audio", seek, accent, cx))
        })
        .into_any_element()
}

//! Parts of the viewer overlay: the video player panel, the zoom row
//! and the saved-file toast.

use super::*;

impl QuillApp {
    /// Parity slice 5: video transport under the visual. the audio engine runs
    /// `-nodisp` for audio only (no GPUI video element in this stack);
    /// the decoded video frames render in-viewer above. The overlay
    /// shows Play/Pause plus elapsed/total, or a download CTA while the
    /// clip is not local.
    pub(super) fn viewer_video_controls(
        &mut self,
        item: &MediaViewerItem,
        row_id: u64,
        files: &HashMap<i32, ParsedFile>,
        downloading: &HashSet<i32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let clip_path = self.viewer_clip_path(item);
        if let Some(_clip) = clip_path {
            let playing = self
                .viewer_clock
                .as_ref()
                .is_some_and(|clock| clock.is_playing())
                && self.viewer_video == Some(item.message_id);
            // MED1: while scrubbing, the label previews the drag
            // position (the history-row seek pattern).
            let elapsed = self
                .viewer_seek_preview_secs
                .or_else(|| self.viewer_clock.as_ref().map(|clock| clock.elapsed_secs()))
                .unwrap_or(0.0);
            let total = item.duration_secs.unwrap_or(0) as f64;
            let label = format!(
                "{} / {}",
                format_voice_duration(elapsed as i32),
                format_voice_duration(total as i32)
            );
            // While ffmpeg extracts frames the thumbnail stays up;
            // the Play button appears once frames are ready.
            let extracting = self.viewer_extracting;
            let speed_dial = self.speed_dial("media-viewer-speed", cx);
            let muted = self.playback_volume < 0.01;
            let volume_pct = (self.playback_volume * 100.0).round() as i32;
            // Telegram Desktop's player panel: the seek bar spans the
            // panel with elapsed / remaining time at its ends; below it
            // play/pause, volume, then speed and picture-in-picture.
            let remaining = format!(
                "-{}",
                format_voice_duration((total - elapsed).max(0.0) as i32)
            );
            use gpui_kit::assets::IconName as Lucide;
            let icon_button = |id: &'static str, icon: Lucide, label: &'static str| {
                Button::new((id, row_id))
                    .icon(icon)
                    .ghost()
                    .text_color(gpui_kit::white())
                    .tooltip(label)
                    .accessibility_label(label)
            };
            let time = |text: String| {
                div()
                    .flex_none()
                    .min_w(px(44.))
                    .text_xs()
                    .text_color(gpui_kit::white().opacity(0.85))
                    .child(text)
            };
            let seek_row = div()
                .flex()
                .items_center()
                .gap_2()
                .child(time(format_voice_duration(elapsed as i32)))
                .child(
                    // The slider thumb overhangs its track; the padding
                    // keeps it clear of the labels beside it.
                    div().flex_1().px_2().when_some(
                        self.viewer_seek_slider.clone(),
                        |this, slider| {
                            this.child(Slider::new(&slider).bg(accent()).text_color(text_on_fill()))
                        },
                    ),
                )
                .child(time(remaining).text_right());
            let controls_row = div()
                .flex()
                .items_center()
                .gap_1()
                .child(if extracting {
                    div()
                        .text_sm()
                        .text_color(gpui_kit::white())
                        .child("Loading video…")
                        .into_any_element()
                } else {
                    icon_button(
                        "media-viewer-play",
                        if playing { Lucide::Pause } else { Lucide::Play },
                        if playing {
                            "Pause (Space)"
                        } else {
                            "Play (Space)"
                        },
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_viewer_video(cx);
                    }))
                    .into_any_element()
                })
                .child(
                    icon_button(
                        "media-viewer-mute",
                        if muted {
                            Lucide::VolumeX
                        } else {
                            Lucide::Volume2
                        },
                        if muted { "Unmute" } else { "Mute" },
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_playback_mute(cx);
                    })),
                )
                .when_some(self.viewer_volume_slider.clone(), |this, slider| {
                    this.child(
                        div()
                            .w(px(96.))
                            .px_2()
                            .child(Slider::new(&slider).bg(accent()).text_color(text_on_fill())),
                    )
                })
                .child(div().flex_1())
                .child(speed_dial)
                .when(
                    cfg!(target_os = "macos") && !self.viewer_video_frames.is_empty(),
                    |this| {
                        this.child(
                            icon_button(
                                "media-viewer-pip",
                                Lucide::PictureInPicture2,
                                "Picture-in-Picture",
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.open_video_pip(cx))),
                        )
                    },
                )
                .child({
                    let fullscreen = self.viewer_extra.video_fullscreen;
                    icon_button(
                        "media-viewer-fullscreen",
                        if fullscreen {
                            Lucide::Minimize
                        } else {
                            Lucide::Maximize
                        },
                        if fullscreen {
                            "Exit full screen (Esc)"
                        } else {
                            "Full screen (Alt+Enter)"
                        },
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.viewer_toggle_fullscreen(window, cx);
                    }))
                });
            let _ = (label, volume_pct);
            div()
                .id(("media-viewer-player", row_id))
                .w(px(640.))
                .max_w_full()
                .px_4()
                .py_2()
                .rounded_xl()
                .bg(gpui_kit::black().opacity(0.6))
                .flex()
                .flex_col()
                .gap_1()
                .child(seek_row)
                .child(controls_row)
                .into_any_element()
        } else {
            let clip_downloading = item
                .play_file_id
                .is_some_and(|id| file_is_downloading(id, files, downloading));
            let play_id = item.play_file_id;
            let message_id = item.message_id;
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(gpui_kit::white())
                        .child(if clip_downloading {
                            "Video — downloading clip…"
                        } else {
                            "Video — clip not downloaded"
                        }),
                )
                .when_some(play_id.filter(|_| !clip_downloading), |this, id| {
                    this.child(
                        Button::new(("media-viewer-download", row_id))
                            .label("Download")
                            .ghost()
                            .text_color(gpui_kit::white())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.viewer_pending_play = Some((message_id, id));
                                this.request_media_download(id, None, cx);
                            })),
                    )
                })
                .into_any_element()
        }
    }

    /// Parity slice 5: zoom controls share a row with the video
    /// transport — − / % / + / Reset, then Play/Pause + elapsed/total.
    pub(super) fn viewer_zoom_controls(
        zoom: ViewerZoom,
        row_id: u64,
        cx: &mut Context<Self>,
    ) -> Div {
        let zoom_pct = format!("{}%", (zoom.zoom * 100.0).round() as i32);
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new(("media-viewer-zoom-out", row_id))
                    .label("−")
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(false, cx);
                    })),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(gpui_kit::white())
                    .child(zoom_pct),
            )
            .child(
                Button::new(("media-viewer-zoom-in", row_id))
                    .label("+")
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_zoom_step(true, cx);
                    })),
            )
            .child(
                Button::new(("media-viewer-zoom-reset", row_id))
                    .label("Reset")
                    .ghost()
                    .text_color(gpui_kit::white())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_reset_zoom(cx);
                    })),
            )
    }

    /// tdesktop `showSaveMsgToast`: where the saved file went, with a
    /// link that shows it in the file manager.
    pub(super) fn viewer_saved_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.viewer_extra.saved_toast.clone().map(|toast| {
            div()
                .absolute()
                .top(px(VIEWER_TOP_BAR + 12.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("media-viewer-saved-toast")
                        .occlude()
                        .role(Role::Status)
                        .aria_label(format!(
                            "{}{}{}",
                            toast.text.before, toast.text.folder, toast.text.after
                        ))
                        .max_w(px(520.))
                        .pl_3()
                        .pr_1()
                        .py_1()
                        .rounded_full()
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().popover)
                        .text_color(cx.theme().popover_foreground)
                        .shadow_md()
                        .text_sm()
                        .flex()
                        .items_center()
                        .child(toast.text.before.clone())
                        .child(
                            Button::new("media-viewer-saved-folder")
                                .label(toast.text.folder.clone())
                                .link()
                                .small()
                                .tooltip("Show in folder")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.reveal_saved_toast_file(cx);
                                })),
                        )
                        .child(toast.text.after.clone()),
                )
                .into_any_element()
        })
    }
}

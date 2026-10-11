//! Methods moved out of `story_viewer.rs` to keep files under 1000 lines.

use super::*;

impl QuillApp {
    /// Phase 9.1: fullscreen story overlay, modeled on
    /// `media_viewer_overlay`: poster name + "Story N of M" header, the
    /// photo (video shows its thumbnail; live/unsupported show a
    /// placeholder), the caption, and Prev / Next / Close controls. The
    /// backdrop click and Escape (see `cancel_search`) close it.
    pub(in crate::ui) fn story_viewer_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = self
            .stories
            .viewer
            .current()
            .cloned()
            .unwrap_or_else(|| StoryViewerItem {
                chat_id: ChatId(0),
                story_id: 0,
                kind: StoryViewerKind::Unsupported,
                display_file_ids: Vec::new(),
                download_file_id: FileId(0),
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: None,
                duration_secs: None,
                video_file_id: None,
                is_live: false,
                live_call: None,
                areas: Vec::new(),
            });
        let (position, total) = self.stories.viewer.position().unwrap_or((0, 0));
        let now = Instant::now();
        let poster = self
            .session()
            .and_then(|s| s.chats.get(&item.chat_id.0))
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| format!("Chat {}", item.chat_id.0));
        let files: HashMap<i32, ParsedFile> = self
            .session()
            .map(|s| s.media.files.clone())
            .unwrap_or_default();
        let downloading: HashSet<i32> = self
            .session()
            .map(|s| s.media.downloading.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let path = story_viewer_display_path(&item, &files, &roots);
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, &files, &downloading));
        let kind_label = item.kind.label();
        let header_label = if total > 1 {
            format!("{poster} · {kind_label} {position} of {total}")
        } else {
            format!("{poster} · {kind_label}")
        };
        let video_frame = self.story_video_element(cx);
        let visual: AnyElement = if let Some(frame) = video_frame {
            frame
        } else if let Some(path) = path {
            img(path)
                .id(("story-viewer-img", item.story_id as u64))
                .w(px(360.))
                .h(px(640.))
                .aspect_ratio(px(360.) / px(640.))
                .rounded_md()
                .object_fit(ObjectFit::Contain)
                .bg(bg_deep())
                .with_fallback(move || {
                    div()
                        .w(px(360.))
                        .h(px(640.))
                        .rounded_md()
                        .bg(bg_deep())
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(text_bright())
                        .child(format!("{kind_label} — could not render"))
                        .into_any_element()
                })
                .into_any_element()
        } else {
            let status = if downloading_now {
                format!("{kind_label} — downloading…")
            } else {
                format!("{kind_label} — not downloaded")
            };
            let status = match (&item.duration_label, downloading_now) {
                (Some(duration), _) => format!("Video · {duration} — {status}"),
                _ => status,
            };
            // stories-live-play: live stories backed by an ordinary group
            // call get a Join button; unverified RTMP playback
            // and unsupported content keep an
            // honest placeholder.
            let joinable = matches!(item.kind, StoryViewerKind::Live)
                && item.live_call.is_some_and(|call| !call.is_rtmp_stream);
            let body: AnyElement =
                if joinable {
                    let chat_id = item.chat_id;
                    let story_id = item.story_id;
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(text_bright())
                                .child("🔴 Live story"),
                        )
                        .child(Button::new("story-join-live").label("Join live").on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.join_live_story_from_viewer(chat_id, story_id, cx);
                            }),
                        ))
                        .into_any_element()
                } else {
                    let status = if matches!(item.kind, StoryViewerKind::Live) {
                        format!("{kind_label} — RTMP playback is not supported yet")
                    } else if matches!(item.kind, StoryViewerKind::Unsupported) {
                        format!("{kind_label} — not supported in this slice")
                    } else {
                        status
                    };
                    div()
                        .text_sm()
                        .text_color(text_bright())
                        .child(status)
                        .into_any_element()
                };
            div()
                .id(("story-viewer-loading", item.story_id as u64))
                .w(px(360.))
                .h(px(640.))
                .rounded_md()
                .bg(bg_deep())
                .flex()
                .items_center()
                .justify_center()
                .child(body)
                .into_any_element()
        };
        // Phase 9.8: clickable story areas — chips over the 360x640 media
        // box, centered on the `storyAreaPosition` x/y fractions
        // (`schema/td_api.tl:6530`; x/y are the rectangle's CENTER). The
        // media box is the positioning context (areas are media fractions)
        // and clips overflowing chips; `rotation_angle` is not rendered in
        // this slice.
        let area_chips: Vec<AnyElement> = item
            .areas
            .iter()
            .enumerate()
            .map(|(index, area)| {
                let kind = area.kind.clone();
                let label = Self::story_area_label(&kind);
                // `storyAreaPosition` x/y are the rectangle's CENTER
                // (`schema/td_api.tl:6530`): the chip's top-left is the
                // center minus half the chip size.
                let chip_w = (area.width * 360.0).max(48.0) as f32;
                let chip_h = (area.height * 640.0).max(24.0) as f32;
                div()
                    .id((
                        "story-area",
                        (item.story_id as u64).wrapping_mul(1000) + index as u64,
                    ))
                    .absolute()
                    .left(px(area.x as f32 * 360.0 - chip_w / 2.0))
                    .top(px(area.y as f32 * 640.0 - chip_h / 2.0))
                    .w(px(chip_w))
                    .h(px(chip_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .role(gpui_kit::Role::Button)
                    .aria_label(label.clone())
                    .tab_index(0)
                    .cursor_pointer()
                    .rounded_md()
                    .bg(rgba(0x00000099))
                    .border_1()
                    .border_color(rgba(0xffffff66))
                    .text_xs()
                    .text_color(rgb(0xffffff))
                    .px_2()
                    .child(label)
                    // tdesktop only pauses on a press that is not on a
                    // clickable area (`ClickHandler::getPressed()`); keep
                    // the press from reaching the media's hold-to-pause.
                    .swallow_press()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.story_area_click(&kind, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        // B14: press-and-hold on the media pauses until release.
        let paused_chip = self.stories.pause.is_paused().then(|| {
            div()
                .absolute()
                .left(px(8.))
                .bottom(px(8.))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgba(0x00000099))
                .text_xs()
                .text_color(rgb(0xffffff))
                .child("Paused")
        });
        let visual: AnyElement = div()
            .id(("story-viewer-media", item.story_id as u64))
            .relative()
            .w(px(360.))
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.story_hold(true, cx)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.story_hold(false, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.story_hold(false, cx)),
            )
            .child(visual)
            .children(area_chips)
            .children(paused_chip)
            .into_any_element();
        let caption: Option<AnyElement> = (!item.caption.is_empty()).then(|| {
            rich_text_line(
                &item.caption,
                &item.caption_entities,
                (item.chat_id.0, item.story_id as u64),
                true,
                &self.message_ui.spoiler_revealed,
                // Settings → Appearance: captions follow the message font size.
                self.msg_font(),
                // Captions don't resolve custom emoji in this slice (text fallback).
                &HashMap::new(),
                cx,
            )
        });
        // Phase 9.2: own-story interaction counters under the caption.
        let counts: Option<String> = self.story_viewer_counts();
        div()
            .id("story-viewer-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("story-viewer-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .bg(scrim())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_story_viewer(cx);
                    })),
            )
            .child(
                div()
                    .id("story-viewer-panel")
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .p_4()
                    .max_w(px(480.))
                    .max_h_full()
                    .child(self.story_progress_bar(position, total, now))
                    .child(
                        div()
                            .flex()
                            .w(px(360.))
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_semibold()
                                    .text_color(text_bright())
                                    .min_w_0()
                                    .truncate()
                                    .child(super::bidi_line::one_line_plain(header_label)),
                            )
                            .child(
                                div()
                                    .id("story-viewer-close")
                                    .role(gpui_kit::Role::Button)
                                    .aria_label("Close story viewer")
                                    .tab_index(0)
                                    .cursor_pointer()
                                    .pressable(cx.theme())
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .text_color(text_bright())
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_story_viewer(cx);
                                    })),
                            ),
                    )
                    .child(visual)
                    .when_some(caption, |this, caption| {
                        this.child(div().text_color(text_bright()).child(caption))
                    })
                    .when_some(counts, |this, counts| {
                        this.child(div().text_xs().text_color(text_muted()).child(counts))
                    })
                    // Phase 9.5: "Reposted from …" / "edited" marker.
                    .when_some(self.story_viewer_meta_line(), |this, meta| {
                        this.child(div().text_xs().text_color(text_muted()).child(meta))
                    })
                    .child(self.story_more_row(cx))
                    .child(self.story_action_row(cx))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("story-viewer-prev")
                                    .label("‹ Prev")
                                    .ghost()
                                    .text_color(text_bright())
                                    .disabled(position <= 1)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_story_viewer(-1, cx);
                                    })),
                            )
                            .child(
                                Button::new("story-viewer-next")
                                    .label("Next ›")
                                    .ghost()
                                    .text_color(text_bright())
                                    .disabled(position >= total)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.step_story_viewer(1, cx);
                                    })),
                            ),
                    ),
            )
    }

    /// Phase 9.6: segmented progress bar — one segment per story in the
    /// viewer sequence; viewed segments full, the current one fills with
    /// playback progress, upcoming ones dim. Matches Telegram Android's
    /// `StoryLinesDrawable` (segment `a < index` full, `a == index` partial,
    /// the rest a dim track, `StoryLinesDrawable.java:111-140`) and
    /// Unigram's `StoryProgress` (viewed opacity 1, upcoming 0.3,
    /// `StoryContent.xaml.cs:2117`).
    pub(in crate::ui) fn story_progress_bar(
        &self,
        position: usize,
        total: usize,
        now: Instant,
    ) -> impl IntoElement {
        let current_progress = self.story_segment_progress(now);
        div()
            .id("story-viewer-progress")
            .flex()
            .w(px(360.))
            .gap_1()
            .children((0..total).map(|i| {
                let fill = if i + 1 < position {
                    1.0
                } else if i + 1 == position {
                    current_progress
                } else {
                    0.0
                };
                div()
                    .flex_1()
                    .h(px(3.))
                    .rounded_full()
                    .bg(rgba(0xffffff4d))
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .w(relative(fill)),
                    )
            }))
    }
}

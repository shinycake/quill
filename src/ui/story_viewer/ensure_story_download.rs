//! Methods moved out of `story_viewer.rs` to keep files under 1000 lines.

use super::*;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Trigger `downloadFile` for the current story when no display
    /// candidate is local yet (photo: largest size; video: thumbnail, else
    /// the clip itself). Live-only, like `ensure_viewer_download`.
    pub(in crate::ui) fn ensure_story_download(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let roots = self.media_display_roots();
        let local = self.session().is_some_and(|session| {
            item.display_file_ids.iter().any(|id| {
                session
                    .media
                    .files
                    .get(&id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
                    .is_some()
            })
        });
        if !local && item.download_file_id.0 != 0 {
            self.request_media_download(item.download_file_id, None, cx);
        }
        // B14: a video story also needs its clip to play.
        if let Some(video_id) = item.video_file_id
            && video_id != item.download_file_id
        {
            let roots = self.media_display_roots();
            let clip_local = self.session().is_some_and(|session| {
                session
                    .media
                    .files
                    .get(&video_id.0)
                    .and_then(|file| file.usable_path())
                    .and_then(|path| sandboxed_display_path(path, &roots))
                    .is_some()
            });
            if !clip_local {
                self.request_media_download(video_id, None, cx);
            }
        }
        self.preload_adjacent_stories();
    }

    /// tdesktop `preloadNext`: warm the next stories and the previous one
    /// below the current story's priority, and cancel the preloads of
    /// stories that are no longer next to it. The current story's files
    /// were asked for in full just before, so they are kept.
    fn preload_adjacent_stories(&mut self) {
        let plan = match self.session() {
            Some(session) => quill::story_preload::plan(session, &self.stories.viewer),
            None => return,
        };
        let keep: Vec<FileId> = self
            .stories
            .viewer
            .current()
            .map(|item| {
                item.display_file_ids
                    .iter()
                    .copied()
                    .chain([item.download_file_id])
                    .chain(item.video_file_id)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.sync_story_preloads(&plan, &keep);
        }
    }

    /// The viewer tick's custom-emoji warm-up. The downloads check is cheap
    /// and deduped, so it runs every tick (a failed or dropped download is
    /// asked for again). Only the sticker-metadata fetch is gated: it runs
    /// when the story, the ids it needs or the stickers known changed, and
    /// that fact is recorded only after the fetch succeeded, so a failed
    /// fetch is retried on the next tick. A TDLib error answer counts as a
    /// failure too (`StoriesState::custom_emoji_failures` is part of the
    /// record), and the retry waits out `custom_emoji_retry`. Returns
    /// whether a fetch ran.
    pub(in crate::ui) fn warm_story_custom_emoji(&mut self) -> bool {
        self.warm_story_custom_emoji_with(std::time::Instant::now(), |app, ids| {
            app.live.as_mut().is_some_and(|live| {
                live.driver
                    .maybe_fetch_story_custom_emoji_stickers(ids)
                    .is_ok()
            })
        })
    }

    /// `warm_story_custom_emoji` with the fetch supplied; it reports
    /// whether the fetch succeeded.
    pub(in crate::ui) fn warm_story_custom_emoji_with(
        &mut self,
        now: std::time::Instant,
        fetch: impl FnOnce(&mut Self, &[i64]) -> bool,
    ) -> bool {
        let ids = self.story_custom_emoji_fetch_ids();
        let (known, failures, waiting) = self.session().map_or((0, 0, false), |s| {
            (
                s.stories.custom_emoji_stickers.len(),
                s.stories.custom_emoji_failures,
                s.stories.custom_emoji_fetch_waiting(now),
            )
        });
        let key = (
            self.stories.viewer.current().map(|i| i.key()),
            ids,
            known,
            failures,
        );
        let fetched = !waiting && self.stories.emoji_warm.as_ref() != Some(&key);
        if fetched && fetch(self, &key.1) {
            self.stories.emoji_warm = Some(key);
        }
        self.ensure_story_custom_emoji_downloads();
        fetched
    }

    /// Phase 9.2+: warm `downloadFile` for custom-emoji reaction sticker
    /// display files (picker tiles + chosen badge). Metadata from
    /// `getCustomEmojiStickers` alone is not enough — `story_custom_emoji_path`
    /// needs a completed local file. Dedupes active downloads so the viewer
    /// tick can call this safely every frame.
    pub(in crate::ui) fn ensure_story_custom_emoji_downloads(&mut self) {
        let ids = self.story_custom_emoji_fetch_ids();
        let file_ids: Vec<_> = {
            let Some(session) = self.session() else {
                return;
            };
            ids.iter()
                .filter_map(|id| {
                    let sticker = session.stories.custom_emoji_stickers.get(id)?;
                    let file_id = sticker.display_file_id()?;
                    if file_id.0 == 0 {
                        return None;
                    }
                    match session.media.files.get(&file_id.0) {
                        Some(file)
                            if file.usable_path().is_some() || file.local.is_downloading_active =>
                        {
                            None
                        }
                        _ => Some(file_id),
                    }
                })
                .collect()
        };
        for file_id in file_ids {
            if let Some(live) = self.live.as_mut() {
                // Display chrome uses one-shot downloadFile, like chat
                // thumbnails, rather than the user's downloads list.
                let _ = live.driver.download_file(file_id, 1);
            }
        }
    }

    /// Phase 9.2+: the reaction picker popover fed by
    /// `getStoryAvailableReactions` (`availableReactions`,
    /// `schema/td_api.tl:13802`). Emoji taps set the reaction via
    /// `setStoryReaction`; custom-emoji taps use
    /// `set_story_custom_emoji_reaction` (Premium enforcement is
    /// server-side — `needs_premium` rows carry a badge). Paid reactions
    /// are never offered (`setStoryReaction` can't set them, schema
    /// comment `td_api.tl:13809`).
    pub(in crate::ui) fn story_reaction_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let reactions = self
            .session()
            .and_then(|session| session.stories.available_reactions.clone())
            .unwrap_or_default();
        let mut picker = div()
            .id("story-reaction-picker")
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_center()
            .gap_1()
            .max_w(px(360.))
            .p_2()
            .rounded_md()
            .bg(bg_canvas())
            .border_1()
            .border_color(border());
        if reactions.is_empty() {
            picker = picker.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child(tr(keys::Q_MEDIA_LOADING_REACTIONS)),
            );
        }
        for (index, reaction) in reactions.iter().enumerate() {
            match &reaction.kind {
                StoryAvailableReactionKind::Emoji(emoji) => {
                    let emoji = emoji.clone();
                    picker = picker.child(
                        div()
                            .id(("story-reaction-option", index))
                            .role(gpui_kit::Role::Button)
                            .aria_label(tr_with(
                                keys::Q_MEDIA_REACT_WITH,
                                &[("emoji", &emoji.to_string())],
                            ))
                            .tab_index(0)
                            .cursor_pointer()
                            .pressable(cx.theme())
                            .text_2xl()
                            .p_1()
                            .child(emoji.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.pick_story_reaction(&emoji, cx);
                            })),
                    );
                }
                StoryAvailableReactionKind::CustomEmoji(id) => {
                    let id = *id;
                    let path = self.story_custom_emoji_path(id);
                    let mut cell = div()
                        .id(("story-custom-emoji-option", index))
                        .role(gpui_kit::Role::Button)
                        .aria_label(tr_with(
                            keys::Q_MEDIA_REACT_WITH_CUSTOM_EMOJI,
                            &[("id", &id.to_string())],
                        ))
                        .tab_index(0)
                        .cursor_pointer()
                        .pressable(cx.theme())
                        .w(px(64.))
                        .h(px(64.))
                        .flex_shrink_0()
                        .rounded_md()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center();
                    if let Some(path) = path {
                        cell = cell.child(
                            img(path)
                                .id(("story-custom-emoji-img", id as u64))
                                .w(px(36.))
                                .h(px(36.))
                                .aspect_ratio(px(36.) / px(36.))
                                .flex_shrink_0()
                                .object_fit(ObjectFit::Contain)
                                .with_fallback(|| div().text_2xl().child("✨").into_any_element())
                                .into_any_element(),
                        );
                    } else {
                        cell = cell.child(div().text_2xl().child("✨"));
                    }
                    if reaction.needs_premium {
                        cell = cell.child(
                            div()
                                .text_xs()
                                .whitespace_nowrap()
                                .text_color(text_muted())
                                .child(tr(keys::Q_MEDIA_PREMIUM)),
                        );
                    }
                    picker = picker.child(cell.on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_story_custom_emoji_reaction(id, cx);
                    })));
                }
                StoryAvailableReactionKind::Paid => {}
            }
        }
        picker.into_any_element()
    }

    /// The panels the bottom bar and the ⋯ menu open under the viewer: the
    /// reaction picker, the viewers list, statistics, the report flow, the
    /// cover and privacy editors and the management status line.
    pub(in crate::ui) fn story_action_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut column = div().flex().flex_col().gap_2().items_center();
        if self.stories.reaction_picker_open {
            column = column.child(self.story_reaction_picker(cx));
        }
        if self.stories.viewers_open {
            column = column.child(self.story_viewers_panel(cx));
        }
        if self.stories.stats_open {
            column = column.child(self.story_stats_panel(cx));
        }
        if self.stories.report_open {
            column = column.child(self.story_report_ui(cx));
        }
        // Phase 9.5: cover-frame editor row (video stories) + privacy
        // editor panel + the management status line (pending / error).
        if self.stories.cover.is_some() {
            column = column.child(self.story_cover_editor(cx));
        }
        if self.stories.privacy_edit.is_some() {
            column = column.child(self.story_privacy_panel(cx));
        }
        if let Some(status) = self.story_manage_status() {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(chrome::secondary())
                    .child(status),
            );
        }
        column.into_any_element()
    }

    /// Phase 9.5: the viewers panel — one row per interaction (actor
    /// name, relative time, reaction emoji or Forwarded/Reposted), a
    /// count header, the next page on scrolling near the end, and
    /// the loading / error / empty states.
    pub(in crate::ui) fn story_viewers_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.session().and_then(|s| s.stories.viewers.clone());
        let mut panel = div()
            .id("story-viewers-panel")
            .flex()
            .flex_col()
            .gap_1()
            .max_w(px(360.))
            .max_h(px(260.))
            .overflow_y_scroll()
            .track_scroll(&self.stories.viewers_scroll)
            .on_scroll(cx.listener(|this, _, _, cx| this.story_viewers_scrolled(cx)))
            .p_2()
            .rounded_md()
            .bg(bg_canvas())
            .border_1()
            .border_color(border());
        let Some(state) = state else {
            return panel
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MEDIA_NO_VIEWERS_YET)),
                )
                .into_any_element();
        };
        panel = panel.child(
            div()
                .text_sm()
                .font_medium()
                .text_color(text_menu())
                .child(format!(
                    "{} viewer{}",
                    state.total_count,
                    if state.total_count == 1 { "" } else { "s" }
                )),
        );
        for viewer in &state.rows {
            let name = self.story_viewer_actor_name(&viewer.actor);
            // Row suffix: the chosen reaction, or the interaction kind
            // ("viewed" / "forwarded" / "reposted"), plus relative time.
            let detail = format!(
                "{} · {}",
                viewer.kind_label(),
                event_log_relative_time(viewer.interaction_date)
            );
            panel = panel.child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(initials_avatar(&name, 28.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(text_menu())
                                    .truncate()
                                    .child(super::bidi_line::one_line_plain(name)),
                            )
                            .child(div().text_xs().text_color(text_muted()).child(detail)),
                    ),
            );
        }
        if let Some(error) = state.error.clone() {
            panel = panel.child(div().text_sm().text_color(danger()).child(error));
        }
        if state.loading || !state.next_offset.is_empty() {
            panel = panel.child(
                div()
                    .id("story-viewers-loading-more")
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .py_2()
                    .text_sm()
                    .text_color(text_muted())
                    .child(gpui_kit::component::spinner::Spinner::new().small())
                    .child(tr(keys::Q_MEDIA_LOADING_VIEWERS)),
            );
        } else if state.rows.is_empty() && !state.loading {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child(tr(keys::Q_MEDIA_NO_ONE_HAS_VIEWED_THIS)),
            );
        }
        panel.into_any_element()
    }

    /// Phase 9.5: the report flow UI. Mirrors the `StoryReportStage`
    /// states: Checking/Sending (spinner text), the server-provided
    /// option picker, the details field, Reported / Failed.
    pub(in crate::ui) fn story_report_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let flow = self.session().and_then(|s| s.stories.report.clone());
        let mut panel = div()
            .id("story-report-panel")
            .flex()
            .flex_col()
            .gap_2()
            .max_w(px(360.))
            .p_2()
            .rounded_md()
            .bg(bg_canvas())
            .border_1()
            .border_color(border());
        let Some(flow) = flow else {
            return panel
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::BOT_DOWNLOAD_STARTING)),
                )
                .into_any_element();
        };
        match flow.stage {
            StoryReportStage::Checking | StoryReportStage::Sending => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(tr(keys::Q_MEDIA_REPORTING_STORY)),
                );
            }
            StoryReportStage::PickOption {
                ref title,
                ref options,
            } => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .font_medium()
                        .text_color(text_menu())
                        .child(title.clone()),
                );
                for option in options {
                    let option_id = option.id.clone();
                    panel = panel.child(
                        Button::new(format!("story-report-option-{}", option.id))
                            .label(option.text.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.pick_story_report_option(&option_id, cx);
                            })),
                    );
                }
            }
            StoryReportStage::TextRequired {
                ref option_id,
                is_optional,
            } => {
                let option_id = option_id.clone();
                panel = panel.child(div().text_sm().text_color(text_muted()).child(
                    if is_optional {
                        tr(keys::Q_MEDIA_ADD_DETAILS_OPTIONAL).to_string()
                    } else {
                        tr(keys::Q_MEDIA_ADD_DETAILS).to_string()
                    },
                ));
                panel = panel.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div().flex_1().child(
                                Textarea::new(&self.stories.report_text_input)
                                    .aria_label(tr(keys::Q_MEDIA_STORY_REPORT_EXPLANATION))
                                    .h(px(40.)),
                            ),
                        )
                        .child(
                            Button::new("story-report-send")
                                .label(tr(keys::Q_MEDIA_SEND))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.send_story_report_text(window, cx);
                                })),
                        ),
                );
                if is_optional {
                    panel = panel.child(
                        Button::new("story-report-skip")
                            .label(tr(keys::Q_MEDIA_SKIP))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let Some(item) = this.stories.viewer.current().cloned() else {
                                    return;
                                };
                                this.send_story_report_step(&item, &option_id, "", cx);
                            })),
                    );
                }
            }
            StoryReportStage::Reported => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(success())
                        .child(tr(keys::Q_MEDIA_STORY_REPORTED)),
                );
            }
            StoryReportStage::Failed(ref error) => {
                panel = panel.child(div().text_sm().text_color(danger()).child(error.clone()));
            }
        }
        panel.into_any_element()
    }
}

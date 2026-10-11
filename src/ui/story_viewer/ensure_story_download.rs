//! Methods moved out of `story_viewer.rs` to keep files under 1000 lines.

use super::*;

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

    /// Phase 9.2: the viewer's own-story interaction counters
    /// (`storyInteractionInfo`, `schema/td_api.tl:6712`) — only rendered
    /// when TDLib populated them (`story.can_get_interactions`) and at
    /// least one counter is nonzero.
    pub(in crate::ui) fn story_viewer_counts(&self) -> Option<String> {
        let story = self.current_story()?;
        if !story.can_get_interactions {
            return None;
        }
        let info = story.interaction_info.filter(|info| info.any_nonzero())?;
        let mut parts = Vec::new();
        if info.view_count > 0 {
            parts.push(format!("👁 {}", info.view_count));
        }
        if info.reaction_count > 0 {
            parts.push(format!("❤️ {}", info.reaction_count));
        }
        if info.forward_count > 0 {
            parts.push(format!("↩ {}", info.forward_count));
        }
        Some(parts.join(" · "))
    }

    /// Phase 9.5: "Reposted from …" / "edited" line under the caption
    /// (`story.repost_info`, `story.is_edited`, `td_api.tl:6742`).
    pub(in crate::ui) fn story_viewer_meta_line(&self) -> Option<String> {
        let story = self.current_story()?;
        let mut parts = Vec::new();
        if let Some(repost) = &story.repost_info {
            let origin = match &repost.origin {
                StoryOriginView::PublicStory { chat_id, .. } => self
                    .session()
                    .and_then(|s| s.chats.get(chat_id))
                    .map(|chat| chat.title.clone())
                    .unwrap_or_else(|| format!("Chat {chat_id}")),
                StoryOriginView::HiddenUser { poster_name } => poster_name.clone(),
            };
            parts.push(format!("Reposted from {origin}"));
        }
        if story.is_edited {
            parts.push("edited".into());
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
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
                    .child("Loading reactions…"),
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
                            .aria_label(format!("React with {emoji}"))
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
                        .aria_label(format!("React with custom emoji {id}"))
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
                                .child("Premium"),
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

    /// Phase 9.2: reaction / reply / delete affordances under the viewer
    /// visual, plus the reaction picker popover and the reply input row.
    /// The quick-react toggles ❤; Reply is gated on
    /// `story.can_be_replied`; Delete on `story.can_be_deleted`.
    /// Phase 9.5: Viewers (own stories, `can_get_interactions`), Report
    /// (other people's stories) and the Stealth toggle join the row.
    pub(in crate::ui) fn story_action_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let story = self.current_story();
        let chosen = story
            .as_ref()
            .and_then(|story| story.chosen_reaction_emoji.clone());
        let can_reply = story.as_ref().is_some_and(|story| story.can_be_replied);
        let can_delete = story.as_ref().is_some_and(|story| story.can_be_deleted);
        // Phase 9.5: posted-story management gates (schema 1.8.67,
        // `td_api.tl:6742`) — TDLib's flags are authoritative, so these
        // naturally cover own stories and admin-manageable
        // channel/group stories.
        let can_edit = story.as_ref().is_some_and(|story| story.can_be_edited);
        let can_set_privacy = story
            .as_ref()
            .is_some_and(|story| story.can_set_privacy_settings);
        let can_forward = story.as_ref().is_some_and(|story| story.can_be_forwarded);
        // Phase 9.5 (review fix-up): one shared `story_manage.pending`
        // slot — disable the management buttons while a call is in
        // flight so two ops can't overwrite each other's state.
        let manage_busy = self.session().is_some_and(|s| s.stories.manage.pending);
        let is_video = self
            .stories
            .viewer
            .current()
            .is_some_and(|item| matches!(item.kind, StoryViewerKind::Video));
        let quick_label = if chosen.as_deref() == Some("❤") {
            "❤️ ✓"
        } else {
            "❤️"
        };
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .items_center()
            .justify_center();
        row = row.child(
            Button::new("story-quick-react")
                .label(quick_label)
                .ghost()
                .text_color(text_bright())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.quick_react_story(cx);
                })),
        );
        // Phase 9.2+: the viewer's chosen custom-emoji / paid reaction.
        // Emoji chosen state already shows on the quick-react button;
        // custom emoji renders its sticker (✨ fallback while loading),
        // paid renders ⭐.
        if let Some(extra) = story.as_ref().and_then(|story| story.chosen_reaction_extra) {
            let badge: AnyElement = match extra {
                StoryChosenExtraReaction::CustomEmoji(id) => {
                    match self.story_custom_emoji_path(id) {
                        Some(path) => img(path)
                            .id(("story-chosen-custom-emoji", id as u64))
                            .w(px(28.))
                            .h(px(28.))
                            .aspect_ratio(px(28.) / px(28.))
                            .object_fit(ObjectFit::Contain)
                            .with_fallback(|| div().text_xl().child("✨").into_any_element())
                            .into_any_element(),
                        None => div().text_xl().child("✨").into_any_element(),
                    }
                }
                StoryChosenExtraReaction::Paid => div().text_xl().child("⭐").into_any_element(),
            };
            row = row.child(badge);
        }
        // Phase 9.7: entry point to the chat story page (albums, chat-page
        // stories, archive) for the current story's chat.
        row = row.child(
            Button::new("story-open-page")
                .label("Stories")
                .ghost()
                .text_color(rgb(0xffffff))
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(item) = this.stories.viewer.current() {
                        let chat_id = item.chat_id;
                        this.close_story_viewer(cx);
                        this.open_story_page(chat_id, window, cx);
                    }
                })),
        );
        row = row.child(
            Button::new("story-react-picker")
                .label("React…")
                .ghost()
                .text_color(text_bright())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_story_reaction_picker(cx);
                })),
        );
        if can_reply {
            row = row.child(
                Button::new("story-reply")
                    .label("Reply")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_story_reply(cx);
                    })),
            );
        }
        if can_delete {
            row = row.child(
                Button::new("story-delete")
                    .label("Delete")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.delete_story_viewer(cx);
                    })),
            );
        }
        // Phase 9.5: Viewers on own stories (`can_get_interactions` —
        // `getStoryInteractions` only serves stories posted on behalf of
        // the current user); Report on other people's stories.
        let can_get_interactions = story
            .as_ref()
            .is_some_and(|story| story.can_get_interactions);
        if can_get_interactions {
            row = row.child(
                Button::new("story-viewers")
                    .label("Viewers")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_story_viewers(cx);
                    })),
            );
        }
        if story.as_ref().is_some_and(|story| story.can_get_statistics) {
            row = row.child(
                Button::new("story-statistics")
                    .label("Statistics")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_story_stats(cx);
                    })),
            );
        }
        if let Some(query) = self.viewer_story_search_query() {
            row = row.child(
                Button::new("story-search-here")
                    .label("Stories here")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.search_stories_from_viewer(query.clone(), window, cx);
                    })),
            );
        }
        if !can_delete {
            row = row.child(
                Button::new("story-report")
                    .label("Report")
                    .ghost()
                    .text_color(text_bright())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_story_report(cx);
                    })),
            );
        }
        // Phase 9.5: stealth toggle — the label reflects
        // `updateStoryStealthMode` state.
        let stealth_label = self.story_stealth_label();
        row = row.child(
            Button::new("story-stealth")
                .label(stealth_label)
                .ghost()
                .text_color(text_bright())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_story_stealth(cx);
                })),
        );
        // Phase 9.5: posted-story management — Edit opens the composer
        // in edit mode, Cover edits the video cover frame, Privacy
        // opens the privacy editor, Repost opens the composer with
        // `from_story_full_id` set.
        if can_edit {
            row = row.child(
                Button::new("story-edit")
                    .label("Edit")
                    .ghost()
                    .text_color(text_bright())
                    .disabled(manage_busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let Some(item) = this.stories.viewer.current().cloned() else {
                            return;
                        };
                        this.open_story_edit(item.chat_id.0, item.story_id, window, cx);
                    })),
            );
            if is_video {
                row = row.child(
                    Button::new("story-cover")
                        .label("Cover")
                        .ghost()
                        .text_color(text_bright())
                        .disabled(manage_busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let Some(item) = this.stories.viewer.current().cloned() else {
                                return;
                            };
                            this.toggle_story_cover_edit(item.chat_id.0, item.story_id, cx);
                        })),
                );
            }
        }
        if can_set_privacy {
            row = row.child(
                Button::new("story-privacy")
                    .label("Privacy")
                    .ghost()
                    .text_color(text_bright())
                    .disabled(manage_busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_story_privacy_edit(window, cx);
                    })),
            );
        }
        if can_forward {
            row = row.child(
                Button::new("story-repost")
                    .label("Repost")
                    .ghost()
                    .text_color(text_bright())
                    .disabled(manage_busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let Some(item) = this.stories.viewer.current().cloned() else {
                            return;
                        };
                        this.open_story_repost(item.chat_id.0, item.story_id, window, cx);
                    })),
            );
        }
        let mut column = div().flex().flex_col().gap_2().items_center().child(row);
        if self.stories.reaction_picker_open {
            column = column.child(self.story_reaction_picker(cx));
        }
        if self.stories.reply_open {
            column =
                column.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .w(px(360.))
                        .child(
                            div().flex_1().child(
                                Textarea::new(&self.stories.reply_input)
                                    .aria_label("Reply to story")
                                    .h(px(40.)),
                            ),
                        )
                        .child(Button::new("story-reply-send").label("Send").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.send_story_reply(window, cx);
                            }),
                        )),
                );
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
        if let Some(stealth_err) = self
            .session()
            .and_then(|session| session.stories.stealth_error.clone())
        {
            column = column.child(div().text_sm().text_color(danger()).child(stealth_err));
        }
        // Phase 9.5: cover-frame editor row (video stories) + privacy
        // editor panel + the management status line (pending / error).
        if self.stories.cover_target.is_some() {
            column = column.child(self.story_cover_editor(cx));
        }
        if self.stories.privacy_edit.is_some() {
            column = column.child(self.story_privacy_panel(cx));
        }
        if let Some(status) = self.story_manage_status() {
            column = column.child(div().text_xs().text_color(text_muted()).child(status));
        }
        column.into_any_element()
    }

    /// Phase 9.5: the viewers panel — one row per interaction (actor
    /// name, relative time, reaction emoji or Forwarded/Reposted), a
    /// count header, "Load more" while `next_offset` is non-empty, and
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
                        .child("No viewers yet"),
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
        if state.loading {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child("Loading viewers…"),
            );
        } else if !state.next_offset.is_empty() {
            panel = panel.child(
                Button::new("story-viewers-load-more")
                    .label("Load more")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.load_more_story_viewers(cx);
                    })),
            );
        } else if state.rows.is_empty() && !state.loading {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child("No one has viewed this story yet"),
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
                .child(div().text_sm().text_color(text_muted()).child("Starting…"))
                .into_any_element();
        };
        match flow.stage {
            StoryReportStage::Checking | StoryReportStage::Sending => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child("Reporting story…"),
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
                        "Add details (optional)".to_string()
                    } else {
                        "Add details".to_string()
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
                                    .aria_label("Story report explanation")
                                    .h(px(40.)),
                            ),
                        )
                        .child(Button::new("story-report-send").label("Send").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.send_story_report_text(window, cx);
                            }),
                        )),
                );
                if is_optional {
                    panel = panel.child(Button::new("story-report-skip").label("Skip").on_click(
                        cx.listener(move |this, _, _, cx| {
                            let Some(item) = this.stories.viewer.current().cloned() else {
                                return;
                            };
                            this.send_story_report_step(&item, &option_id, "", cx);
                        }),
                    ));
                }
            }
            StoryReportStage::Reported => {
                panel = panel.child(
                    div()
                        .text_sm()
                        .text_color(success())
                        .child("Story reported"),
                );
            }
            StoryReportStage::Failed(ref error) => {
                panel = panel.child(div().text_sm().text_color(danger()).child(error.clone()));
            }
        }
        panel.into_any_element()
    }
}

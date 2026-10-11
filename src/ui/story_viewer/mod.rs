//! story viewer overlay.

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::message_media::{file_is_downloading, story_viewer_display_path};
use super::message_text::rich_text_line;
use super::nested_click::SwallowPress;
use super::pressable::PressableDiv;
use super::search_ui::chat_search_jump_note;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::state::{StoryReportStage, event_log_relative_time};
use quill::story_viewer::{StoryViewer, StoryViewerItem, StoryViewerKind, collect_story_items};
use quill::telegram::envelope::{
    MessageSender, ParsedFile, ParsedStory, StoryAreaKind, StoryAvailableReactionKind,
    StoryChosenExtraReaction, StoryOriginView,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

impl QuillApp {
    /// Phase 9.1: open the fullscreen story viewer on `(chat_id, story_id)`.
    /// Missing story details for the chat's active stories are fetched with
    /// `getStory` first; the clicked story opens once its `story` response
    /// lands in the cache (`pending_story_open`, resolved on the next
    /// render). `openStory` marks the current story as viewed.
    pub(super) fn open_story_viewer(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        let missing: Vec<i32> = self
            .session()
            .and_then(|session| session.stories.tray.get(&chat_id.0))
            .map(|tray| {
                tray.stories
                    .iter()
                    .map(|info| info.story_id)
                    .filter(|id| {
                        !self
                            .session()
                            .is_some_and(|s| s.stories.stories.contains_key(&(chat_id.0, *id)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            for id in missing {
                let _ = live.driver.get_story(chat_id, id);
            }
        } else if !missing.is_empty() {
            self.connection.status_note = "demo — getStory runs with live TDLib".into();
        }
        if !self.rebuild_story_viewer(chat_id, story_id, cx) {
            self.chat_list.pending_story_open = Some((chat_id.0, story_id));
        }
        cx.notify();
    }

    /// Build the viewer items for `chat_id`'s cached stories and open on
    /// `story_id`. Returns `false` when the clicked story is not cached yet.
    pub(super) fn rebuild_story_viewer(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cx: &mut Context<Self>,
    ) -> bool {
        let items: Vec<StoryViewerItem> = self
            .session()
            .and_then(|session| {
                session
                    .stories
                    .tray
                    .get(&chat_id.0)
                    .map(|tray| collect_story_items(chat_id, tray, &session.stories.stories))
            })
            .unwrap_or_default();
        let Some(index) = items.iter().position(|item| item.story_id == story_id) else {
            return false;
        };
        self.begin_story_viewer(items, index, cx);
        true
    }

    /// Open the viewer on `items[index]` and start playback.
    pub(super) fn begin_story_viewer(
        &mut self,
        items: Vec<StoryViewerItem>,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let current = items.get(index).map(|item| (item.chat_id, item.story_id));
        self.stories.viewer = StoryViewer::open(items, index);
        // Phase 9.5: viewers list and report flow are per-story.
        self.stories.viewers_open = false;
        self.stories.report_open = false;
        self.stories.stats_open = false;
        if let (Some(live), Some((chat_id, story_id))) = (self.live.as_mut(), current) {
            let _ = live.driver.open_story(chat_id, story_id);
        }
        // Phase 9.6: (re)start the playback clock + tick whenever the
        // viewer (re)opens — also covers the deferred `pending_story_open`
        // path, which funnels through here.
        self.stories.playback.start(Instant::now());
        self.stories.pause.reset();
        self.stories.native_failed = None;
        self.stories.video_wait_since = Some(Instant::now());
        self.sync_story_video();
        self.ensure_story_tick(cx);
        self.ensure_story_download(cx);
        self.ensure_story_custom_emoji_downloads();
    }

    /// Phase 9.2: the viewer story's freshest `ParsedStory` (reaction
    /// state and interaction counts arrive via `updateStory` without the
    /// viewer items being rebuilt).
    pub(super) fn current_story(&self) -> Option<ParsedStory> {
        let item = self.stories.viewer.current()?;
        self.session()
            .and_then(|session| {
                session
                    .stories
                    .stories
                    .get(&(item.chat_id.0, item.story_id))
            })
            .cloned()
    }

    /// Phase 9.2: quick-react — toggle the ❤ (`reactionTypeEmoji`,
    /// `schema/td_api.tl:2915`) reaction on the current story via
    /// `setStoryReaction` (`schema/td_api.tl:13809`). Removing sends
    /// `reaction_type: null`.
    pub(super) fn quick_react_story(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let chosen = self
            .current_story()
            .and_then(|story| story.chosen_reaction_emoji);
        let emoji = if chosen.as_deref() == Some("❤") {
            None
        } else {
            Some("❤")
        };
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live
                    .driver
                    .set_story_reaction(item.chat_id, item.story_id, emoji)
                {
                    Ok(_) => {
                        if emoji.is_some() {
                            "Reacted ❤".into()
                        } else {
                            "Reaction removed".into()
                        }
                    }
                    Err(_) => "could not set story reaction".into(),
                };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — setStoryReaction runs with live TDLib".into();
        }
        self.stories.reaction_picker_open = false;
        cx.notify();
    }

    /// stories-live-play: the viewer Join button for a live story.
    /// The driver two-steps (`getGroupCall`, then the pump issues
    /// `join_video_chat` once the tracker exists); failures surface as a
    /// status note instead of a silent no-op.
    pub(super) fn join_live_story_from_viewer(
        &mut self,
        chat_id: ChatId,
        story_id: i32,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.join_live_story(chat_id, story_id) {
                Ok(_) => "joining live story…".into(),
                Err(_) => "could not join the live story".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — live stories join with live TDLib".into();
        }
        cx.notify();
    }

    /// Phase 9.2: toggle the reaction picker above the viewer. The first
    /// open on a live connection fetches `getStoryAvailableReactions`
    /// (`schema/td_api.tl:13802`).
    pub(super) fn toggle_story_reaction_picker(&mut self, cx: &mut Context<Self>) {
        self.stories.reaction_picker_open = !self.stories.reaction_picker_open;
        if self.stories.reaction_picker_open {
            if let Some(live) = self.live.as_mut() {
                if live.driver.session.stories.available_reactions.is_none() {
                    match live.driver.get_story_available_reactions() {
                        Ok(_) => {}
                        Err(_) => {
                            self.connection.status_note = "could not load story reactions".into()
                        }
                    }
                }
            } else if self.demo_session.is_some() {
                self.connection.status_note = "demo — story reactions run with live TDLib".into();
            }
        }
        cx.notify();
    }

    /// Phase 9.2: set the current story's reaction to a picker emoji.
    pub(super) fn pick_story_reaction(&mut self, emoji: &str, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live
                    .driver
                    .set_story_reaction(item.chat_id, item.story_id, Some(emoji))
                {
                    Ok(_) => format!("Reacted {emoji}"),
                    Err(_) => "could not set story reaction".into(),
                };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — story reactions run with live TDLib".into();
        }
        self.stories.reaction_picker_open = false;
        cx.notify();
    }

    /// Phase 9.2+: display path for a custom-emoji reaction sticker
    /// (thumbnail first, else static WEBP — `StickerContent`'s rule) —
    /// `None` while the file isn't downloaded.
    fn story_custom_emoji_path(&self, custom_emoji_id: i64) -> Option<std::path::PathBuf> {
        let session = self.session()?;
        let sticker = session
            .stories
            .custom_emoji_stickers
            .get(&custom_emoji_id)?;
        let file_id = sticker.display_file_id()?;
        let path = session.media.files.get(&file_id.0)?.usable_path()?;
        sandboxed_display_path(path, &self.media_display_roots())
    }

    /// Phase 9.2+: custom-emoji ids the viewer still needs stickers for —
    /// the picker's custom options plus the current story's chosen
    /// custom-emoji reaction. The viewer tick feeds these to the driver.
    fn story_custom_emoji_fetch_ids(&self) -> Vec<i64> {
        let Some(session) = self.session() else {
            return Vec::new();
        };
        let mut ids: Vec<i64> = session
            .stories
            .available_reactions
            .iter()
            .flatten()
            .filter_map(|reaction| match reaction.kind {
                StoryAvailableReactionKind::CustomEmoji(id) => Some(id),
                _ => None,
            })
            .collect();
        if let Some(StoryChosenExtraReaction::CustomEmoji(id)) = self
            .current_story()
            .and_then(|story| story.chosen_reaction_extra)
        {
            ids.push(id);
        }
        ids
    }

    /// Phase 9.2+: set the current story's reaction to a picker custom
    /// emoji via `setStoryReaction` with `reactionTypeCustomEmoji`
    /// (`schema/td_api.tl:13809` — Premium-only, enforced server-side; a
    /// rejection surfaces as a failed request).
    pub(super) fn pick_story_custom_emoji_reaction(
        &mut self,
        custom_emoji_id: i64,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.set_story_custom_emoji_reaction(
                item.chat_id,
                item.story_id,
                custom_emoji_id,
            ) {
                Ok(_) => "Reacted ✨".into(),
                Err(_) => "could not set story reaction".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — story reactions run with live TDLib".into();
        }
        self.stories.reaction_picker_open = false;
        cx.notify();
    }

    /// Phase 9.8: a story area tap — the action each `StoryAreaType`
    /// performs. The tap actions follow the official clients' documented
    /// behavior: Telegram X implements the story tray only, so no
    /// area-click handling exists there to copy (DECISIONS.md Phase
    /// 9.8). Reuses the existing viewer actions wherever one exists:
    /// OSM map for location/venue (same as `location_row` /
    /// `venue_row`), `setStoryReaction` for suggested reactions (same as
    /// the picker), the URL opener for links, chat-open + message jump
    /// for messages. Weather shows its info line; the gift shows its
    /// name (a full gift info view doesn't exist in the app yet —
    /// DECISIONS.md Phase 9.8).
    pub(super) fn story_area_click(&mut self, kind: &StoryAreaKind, cx: &mut Context<Self>) {
        match kind {
            StoryAreaKind::Location {
                location, address, ..
            } => {
                let url = location.open_street_map_url();
                let label = Self::story_area_pin_label(&[address, &location.coords_label()]);
                self.connection.status_note = if quill::platform::open_external_url(&url) {
                    label
                } else {
                    "could not open map".into()
                };
                cx.notify();
            }
            StoryAreaKind::Venue {
                title,
                address,
                location,
                ..
            } => {
                let url = location.open_street_map_url();
                let mut label = Self::story_area_pin_label(&[title, "Venue"]);
                if !title.is_empty() && !address.is_empty() {
                    label.push_str(" — ");
                    label.push_str(address);
                }
                self.connection.status_note = if quill::platform::open_external_url(&url) {
                    label
                } else {
                    "could not open map".into()
                };
                cx.notify();
            }
            StoryAreaKind::SuggestedReaction { emoji, .. } => {
                self.pick_story_reaction(emoji, cx);
            }
            StoryAreaKind::Link { url } => self.open_message_url(url, cx),
            StoryAreaKind::Message {
                chat_id,
                message_id,
            } => {
                self.story_area_open_message(ChatId(*chat_id), MessageId(*message_id), cx);
            }
            StoryAreaKind::Weather { temperature, emoji } => {
                self.connection.status_note = format!("{emoji} {temperature:.1}°C");
                cx.notify();
            }
            StoryAreaKind::Gift { gift_name } => {
                self.connection.status_note = format!("🎁 {gift_name}");
                cx.notify();
            }
            StoryAreaKind::Unsupported { type_name } => {
                self.connection.status_note = format!("story area {type_name} isn't supported");
                cx.notify();
            }
        }
    }

    /// Phase 9.8: a `storyAreaTypeMessage` tap — close the viewer, open
    /// the target chat, and jump to the message (same jump pipeline as
    /// `jump_to_replied_message`).
    pub(super) fn story_area_open_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        self.close_story_viewer(cx);
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live
                .driver
                .select_chat(chat_id)
                .and_then(|_| live.driver.jump_to_replied_message(message_id))
            {
                Ok(_) => chat_search_jump_note(&live.driver.session),
                Err(_) => "could not open message".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.open_chat(chat_id);
            let _ = session.begin_chat_search_jump(message_id);
            self.connection.status_note = chat_search_jump_note(session);
        }
        cx.notify();
    }

    /// Phase 9.2: toggle the reply row in the viewer (`story.can_be_replied`
    /// gates the button).
    pub(super) fn toggle_story_reply(&mut self, cx: &mut Context<Self>) {
        self.stories.reply_open = !self.stories.reply_open;
        cx.notify();
    }

    /// Phase 9.2: send the reply row's text as a message to the story
    /// poster with `inputMessageReplyToStory` (`schema/td_api.tl:3099`).
    pub(super) fn send_story_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let text = self.stories.reply_input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live
                    .driver
                    .send_story_reply(item.chat_id, item.story_id, &text)
                {
                    Ok(_) => "Story reply sent".into(),
                    Err(_) => "could not send story reply".into(),
                };
            self.stories
                .reply_input
                .update(cx, |input, cx| input.set_value("", window, cx));
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — story replies run with live TDLib".into();
        }
        self.stories.reply_open = false;
        cx.notify();
    }

    /// Phase 9.2: delete the current story (`deleteStory`,
    /// `schema/td_api.tl:13754`; `story.can_be_deleted` gates the button).
    /// The deletion lands as `updateStoryDeleted`, which closes the viewer
    /// at render time.
    pub(super) fn delete_story_viewer(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note =
                match live.driver.delete_story(item.chat_id, item.story_id) {
                    Ok(_) => "Deleting story…".into(),
                    Err(_) => "could not delete story".into(),
                };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — deleteStory runs with live TDLib".into();
        }
        self.stories.reaction_picker_open = false;
        self.stories.reply_open = false;
        cx.notify();
    }

    /// Phase 9.5: toggle the viewers panel on the current story
    /// (`getStoryInteractions`, `schema/td_api.tl:13819` — there is no
    /// `getStoryViewers` constructor). Opening fetches the first page;
    /// `Load more` pages with the previous `next_offset`.
    pub(super) fn toggle_story_viewers(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        self.stories.viewers_open = !self.stories.viewers_open;
        self.stories.report_open = false;
        if self.stories.viewers_open {
            // Phase 9.5 review: reset rows before the fresh page-1
            // fetch — `begin_story_viewers` keeps rows for the same
            // story, so reopening would otherwise duplicate them.
            if let Some(live) = self.live.as_mut() {
                live.driver.session.clear_story_viewers();
            }
            self.fetch_story_viewers(&item, "", cx);
        }
        cx.notify();
    }

    pub(super) fn fetch_story_viewers(
        &mut self,
        item: &StoryViewerItem,
        offset: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            live.driver
                .session
                .begin_story_viewers(item.chat_id.0, item.story_id);
            if live
                .driver
                .get_story_interactions(item.chat_id, item.story_id, offset)
                .is_err()
            {
                self.connection.status_note = "could not load story viewers".into();
            }
        } else if self.demo_session.is_some() {
            // Demo seeds the panel state directly (see the
            // `ReadyStoryViewers` demo); a live fetch says so honestly.
            self.connection.status_note = "demo — story viewers run with live TDLib".into();
        }
        cx.notify();
    }

    /// Phase 9.5: fetch the next viewers page (`next_offset` non-empty).
    pub(super) fn load_more_story_viewers(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let offset = self
            .session()
            .and_then(|session| session.stories.viewers.as_ref())
            .map(|state| state.next_offset.clone())
            .unwrap_or_default();
        if offset.is_empty() {
            return;
        }
        self.fetch_story_viewers(&item, &offset, cx);
    }

    /// Phase 9.5: resolve a viewers-list actor to a display name —
    /// cached user / chat names, falling back to the raw id (the panel
    /// never invents a name).
    pub(super) fn story_viewer_actor_name(&self, actor: &MessageSender) -> String {
        match actor {
            MessageSender::User { user_id } => self
                .session()
                .and_then(|session| session.users.get(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}")),
            MessageSender::Chat { chat_id } => self
                .session()
                .and_then(|session| session.chats.get(chat_id))
                .map(|chat| chat.title.clone())
                .unwrap_or_else(|| format!("Chat {chat_id}")),
        }
    }

    /// Phase 9.5: open the report flow on the current story — the
    /// initial `reportStory` (empty option id / text); the server's
    /// `ReportStoryResult` answers drive the picker and details steps.
    pub(super) fn toggle_story_report(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        self.stories.report_open = !self.stories.report_open;
        self.stories.viewers_open = false;
        if self.stories.report_open {
            self.send_story_report_step(&item, "", "", cx);
        } else {
            self.clear_terminal_story_report();
        }
        cx.notify();
    }

    /// Phase 9.5: drop a terminal (`Reported`/`Failed`) report flow when
    /// its panel closes, so reopening starts a fresh `reportStory`
    /// instead of silently re-sending the initial request. A failed
    /// flow still retries: reopening begins a new flow and re-sends the
    /// initial request.
    pub(super) fn clear_terminal_story_report(&mut self) {
        let terminal = self
            .session()
            .and_then(|session| session.stories.report.as_ref())
            .is_some_and(|flow| {
                matches!(
                    flow.stage,
                    StoryReportStage::Reported | StoryReportStage::Failed(_)
                )
            });
        if !terminal {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.session.clear_story_report();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.clear_story_report();
        }
    }

    pub(super) fn send_story_report_step(
        &mut self,
        item: &StoryViewerItem,
        option_id: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .session
                .stories
                .report
                .as_ref()
                .is_none_or(|flow| flow.story_id != item.story_id)
            {
                live.driver
                    .session
                    .begin_story_report(item.chat_id.0, item.story_id);
            } else {
                live.driver
                    .session
                    .story_report_sending(item.chat_id.0, item.story_id);
            }
            self.connection.status_note =
                match live
                    .driver
                    .report_story(item.chat_id, item.story_id, option_id, text)
                {
                    Ok(_) => "Reporting story…".into(),
                    Err(_) => {
                        // Phase 9.5 review: the driver took the pending
                        // request back, so no answer will ever arrive —
                        // end the flow here instead of spinning forever.
                        live.driver.session.fail_story_report_send(
                            item.chat_id.0,
                            item.story_id,
                            "could not report story".into(),
                        );
                        "could not report story".into()
                    }
                };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — story reports run with live TDLib".into();
        }
        cx.notify();
    }

    /// Phase 9.5: the user picked a report reason — echo the option id
    /// back into `reportStory` with empty text.
    pub(super) fn pick_story_report_option(&mut self, option_id: &str, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        self.send_story_report_step(&item, option_id, "", cx);
    }

    /// Phase 9.5: submit the details text (`reportStoryResultTextRequired`
    /// step); an optional step can be skipped with empty text.
    pub(super) fn send_story_report_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let (option_id, is_optional) = self
            .session()
            .and_then(|session| session.stories.report.as_ref())
            .and_then(|flow| match &flow.stage {
                StoryReportStage::TextRequired {
                    option_id,
                    is_optional,
                } => Some((option_id.clone(), *is_optional)),
                _ => None,
            })
            .unwrap_or_default();
        if option_id.is_empty() && !is_optional {
            return;
        }
        let text = self.stories.report_text_input.read(cx).value().to_string();
        if text.trim().is_empty() && !is_optional {
            self.connection.status_note = "add details or cancel the report".into();
            cx.notify();
            return;
        }
        self.send_story_report_step(&item, &option_id, text.trim(), cx);
        self.stories
            .report_text_input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Phase 9.5: stealth-mode toggle. The button reflects
    /// `Session::story_stealth` (from `updateStoryStealthMode` — the
    /// schema exposes no getter, so the button is honest about only
    /// knowing pushed state): active → informational; cooling down →
    /// disabled; otherwise sends `activateStoryStealthMode`.
    pub(super) fn toggle_story_stealth(&mut self, cx: &mut Context<Self>) {
        let now = now_unix_secs();
        let stealth = self
            .session()
            .map(|session| session.stories.stealth)
            .unwrap_or_default();
        if stealth.is_active(now) {
            self.connection.status_note =
                "Stealth mode is active — your story views are hidden".into();
            cx.notify();
            return;
        }
        if stealth.is_cooling_down(now) {
            self.connection.status_note = "Stealth mode is cooling down — try again later".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.activate_story_stealth_mode() {
                Ok(_) => "Enabling stealth mode…".into(),
                Err(_) => "could not enable stealth mode".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo — stealth mode needs live TDLib".into();
        }
        cx.notify();
    }

    /// Phase 9.5: stealth button label for the action row.
    pub(super) fn story_stealth_label(&self) -> &'static str {
        let now = now_unix_secs();
        let stealth = self
            .session()
            .map(|session| session.stories.stealth)
            .unwrap_or_default();
        if stealth.is_active(now) {
            "Stealth on"
        } else if stealth.is_cooling_down(now) {
            "Stealth cooling down"
        } else {
            "Stealth"
        }
    }

    /// Phase 9.1: close the story viewer; `closeStory` marks the current
    /// story as no longer being viewed.
    pub(super) fn close_story_viewer(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = self.stories.viewer.current().cloned()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.close_story(item.chat_id, item.story_id);
        }
        self.stories.viewer.close();
        self.chat_list.pending_story_open = None;
        self.stories.reaction_picker_open = false;
        self.stories.reply_open = false;
        self.stories.viewers_open = false;
        self.stories.report_open = false;
        self.stories.stats_open = false;
        self.clear_terminal_story_report();
        // Phase 9.5: drop the cover / privacy editors with the viewer.
        self.stories.cover_target = None;
        self.stories.cover_sent = false;
        self.stories.privacy_edit = None;
        self.stories.privacy_sent = false;
        // Phase 9.6: the tick task self-exits on the next wake when the
        // viewer is no longer open.
        self.stories.playback.stop();
        // B14: drop the player and the viewer's extra state with it.
        *self.stories.native.borrow_mut() = None;
        self.stories.native_key = None;
        self.stories.native_failed = None;
        self.stories.pause.reset();
        self.stories.share_open = false;
        self.stories.close_friends_edit = None;
        self.stories.close_friends_saving = false;
        self.stories.notice = None;
        cx.notify();
    }

    pub(super) fn step_story_viewer(&mut self, delta: i32, cx: &mut Context<Self>) {
        let prev = self.stories.viewer.current().cloned();
        if delta < 0 {
            self.stories.viewer.prev();
        } else {
            self.stories.viewer.next();
        }
        let next = self.stories.viewer.current().cloned();
        if let (Some(prev), Some(next)) = (prev, next)
            && (prev.chat_id, prev.story_id) != (next.chat_id, next.story_id)
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.close_story(prev.chat_id, prev.story_id);
            let _ = live.driver.open_story(next.chat_id, next.story_id);
        }
        self.stories.reaction_picker_open = false;
        self.stories.reply_open = false;
        self.stories.stats_open = false;
        // B14: a new story starts playing (a Space pause is per story).
        self.stories.pause.reset();
        self.stories.share_open = false;
        self.stories.notice = None;
        self.stories.native_failed = None;
        // Phase 9.6: manual nav restarts the playback clock for the new
        // current story (same as the official clients).
        self.stories.playback.start(Instant::now());
        self.sync_story_video();
        self.ensure_story_tick(cx);
        self.ensure_story_download(cx);
        self.ensure_story_custom_emoji_downloads();
        cx.notify();
    }

    /// Phase 9.6: pause predicate for story playback — the timer must never
    /// auto-advance under an open panel. Mirrors Telegram Android pausing on
    /// any open popup/sheet/keyboard/reaction UI (`StoryViewer.isPaused`,
    /// `StoryViewer.java:2270`). The S4 viewers panel / report flow hook
    /// into this same predicate when they land (they live on their own
    /// parity branch; not touched here).
    pub(super) fn story_playback_paused(&self) -> bool {
        self.stories.reaction_picker_open
            || self.stories.reply_open
            || self.stories.pause.is_paused()
            || self.stories.share_open
            || self.stories.close_friends_edit.is_some()
    }

    /// Phase 9.6: 100ms tick while the story viewer is open (mirrors
    /// Telegram Desktop's `kPhotoProgressInterval`,
    /// `media_stories_controller.cpp:70`), at most one task — the same
    /// pattern as `ensure_call_tick`. Each wake updates the pause state,
    /// auto-advances when the current story's duration elapses, and closes
    /// the viewer at the end of the sequence (Telegram Desktop:
    /// `updatePlayback` → `subjumpFor(1)` else `storiesClose()`,
    /// `media_stories_controller.cpp:1235-1240`).
    pub(super) fn ensure_story_tick(&mut self, cx: &mut Context<Self>) {
        if !self.stories.viewer.is_open() || self.stories.tick_active {
            return;
        }
        self.stories.tick_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let cont = this
                    .update(cx, |this, cx| {
                        if !this.stories.viewer.is_open() {
                            this.stories.playback.stop();
                            this.stories.tick_active = false;
                            return false;
                        }
                        let now = Instant::now();
                        // B14: the native player drives video stories;
                        // the duration clock stays frozen meanwhile.
                        this.sync_story_video();
                        let user_paused = this.story_playback_paused();
                        let video_driven = this.story_video_driven();
                        this.stories
                            .playback
                            .set_paused(user_paused || video_driven, now);
                        this.apply_story_native_pause(user_paused);
                        this.tick_close_friends(cx);
                        if this.story_segment_finished(now) {
                            this.advance_story_playback(cx);
                        }
                        // Phase 9.2+: keep custom-emoji reaction stickers
                        // warm while the viewer is open (picker options +
                        // the chosen-reaction badge). Deduped in the
                        // driver — a no-op when nothing new is needed.
                        let ids = this.story_custom_emoji_fetch_ids();
                        if let Some(live) = this.live.as_mut() {
                            let _ = live.driver.maybe_fetch_story_custom_emoji_stickers(&ids);
                        }
                        // Warm display-file downloads once stickers are
                        // cached — metadata alone leaves the img() path dead.
                        this.ensure_story_custom_emoji_downloads();
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
        })
        .detach();
    }

    /// Phase 9.6: auto-advance from the playback tick — next story, or close
    /// the viewer at the end of the sequence (official behavior: no loop).
    pub(super) fn advance_story_playback(&mut self, cx: &mut Context<Self>) {
        if self
            .stories
            .viewer
            .position()
            .is_some_and(|(position, total)| position < total)
        {
            self.step_story_viewer(1, cx);
        } else {
            self.close_story_viewer(cx);
        }
    }
}

mod ensure_story_download;
mod story_viewer_overlay;

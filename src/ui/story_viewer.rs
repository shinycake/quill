//! story viewer overlay.

use super::app::QuillApp;
use super::chat_row::initials_avatar;
use super::demo::{demo_file_json, demo_thumb_png_path};
use super::message_media::{file_is_downloading, story_viewer_display_path};
use super::message_text::rich_text_line;
use super::nested_click::SwallowPress;
use super::pressable::PressableDiv;
use super::search_ui::chat_search_jump_note;
use super::story_composer::apply_ready_story_post;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, FileId, MessageId};
use quill::local_path::sandboxed_display_path;
use quill::state::{RequestPurpose, Session, StoryReportStage, event_log_relative_time};
use quill::story_viewer::{StoryViewer, StoryViewerItem, StoryViewerKind, collect_story_items};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::{
    MessageSender, ParsedFile, ParsedStory, StoryAreaKind, StoryAvailableReactionKind,
    StoryChosenExtraReaction, StoryOriginView,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};
/// `ReadyStoryViewers` fixture (Phase 9.5): the `ReadyStoryPost` seed
/// (own photo story 5 in chat 11, `can_get_interactions`), two demo
/// users, and a `storyInteractions` page injected through the real
/// reducer path — a registered `GetStoryInteractions` pending request
/// answered with the JSON — so the viewers panel renders exactly as it
/// would live: one ❤ view 5 minutes ago, one forward 2 hours ago.
pub(super) fn apply_ready_story_viewers(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    apply_ready_story_post(session, sink, seq);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let user = |id: i64, first: &str, last: &str| -> String {
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":null,"profile_photo":null,"is_contact":false,"is_mutual_contact":false,"is_close_friend":false,"is_verified":false,"is_premium":false,"is_support":false,"restriction_reason":"","is_scam":false,"is_fake":false,"is_bot":false,"type":{{"@type":"userTypeRegular"}}}}}}"#
        )
    };
    for json in [user(7001, "Dana", "Levi"), user(7002, "Omar", "Haddad")] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    // Register the pending fetch the way the driver does, then answer
    // it — the purpose-gated reducer only honors the page for the
    // viewer's own request.
    session.begin_story_viewers(11, 5);
    let extra = session.request_for_story(RequestPurpose::GetStoryInteractions, ChatId(11), 5);
    let now = now_unix_secs() as i32;
    let interactions = format!(
        r#"{{"@type":"storyInteractions","@extra":"{}","total_count":2,"total_forward_count":1,"total_reaction_count":1,"next_offset":"","interactions":[{{"@type":"storyInteraction","actor_id":{{"@type":"messageSenderUser","user_id":7001}},"interaction_date":{},"block_list":null,"type":{{"@type":"storyInteractionTypeView","chosen_reaction_type":{{"@type":"reactionTypeEmoji","emoji":"❤"}}}}}},{{"@type":"storyInteraction","actor_id":{{"@type":"messageSenderUser","user_id":7002}},"interaction_date":{},"block_list":null,"type":{{"@type":"storyInteractionTypeForward"}}}}]}}"#,
        extra.0,
        now - 300,
        now - 7200,
    );
    if let Some(owned) = copy_and_parse(&interactions, seq, &dyn_sink) {
        session.apply(owned);
    }
}

/// `ReadySponsored` fixture: open the demo channel (id 13, now ungated) and
/// inject a `sponsoredMessages` response through the same reducer the live
/// `getChatSponsoredMessages` path uses — one Sponsored row, one Recommended.
/// `ReadyStories` fixture (Phase 9.1): active-story tray entries for the two
/// seeded demo chats plus full story details, all through the normal
/// reducer — chat 11 "Demo chat A": order 30, `max_read_story_id` 4, stories
/// 4 (video, read) and 5 (photo, unread); chat 12 "Demo chat B": order 20,
/// all read (muted ring in the tray). Story media points at the existing
/// `demo-thumb.png` fixture as completed downloads, so the viewer renders
/// immediately. The demo opens on chat 11's story 5 ("Photo 2 of 2").
pub(super) fn apply_ready_stories(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let photo_file = demo_file_json(91, &demo_thumb_png_path(), true);
    let video_thumb_file = demo_file_json(92, &demo_thumb_png_path(), true);
    // B14: the video story plays the generated 12 s fixture clip.
    let clip_path = super::demo::demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let video_file = demo_file_json(93, &clip_path, true);
    let tray = |chat_id: i64, order: i64, max_read: i32, story_ids: &[i32]| -> String {
        let stories = story_ids
            .iter()
            .map(|id| {
                format!(
                    r#"{{"@type":"storyInfo","story_id":{id},"date":1700000000,"is_for_close_friends":false,"is_live":false}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"@type":"updateChatActiveStories","active_stories":{{"@type":"chatActiveStories","chat_id":{chat_id},"list":{{"@type":"storyListMain"}},"order":"{order}","can_be_archived":false,"max_read_story_id":{max_read},"stories":[{stories}]}}}}"#
        )
    };
    let caption = |text: &str| -> String {
        format!(
            r#"{{"@type":"formattedText","text":{},"entities":[]}}"#,
            serde_json::to_string(text).unwrap()
        )
    };
    let photo_story = |id: i32, chat_id: i64, text: &str, file: &str| -> String {
        format!(
            r#"{{"@type":"story","id":{id},"poster_chat_id":{chat_id},"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"y","photo":{file},"width":960,"height":1280,"progressive_sizes":[]}}]}}}},"caption":{}}}"#,
            caption(text),
        )
    };
    let jsons = [
        tray(11, 30, 4, &[4, 5]),
        tray(12, 20, 6, &[6]),
        // Chat 11, story 4: video story with a thumbnail (read).
        format!(
            r#"{{"@type":"story","id":4,"poster_chat_id":11,"date":1700000000,"content":{{"@type":"storyContentVideo","video":{{"@type":"storyVideo","duration":12.0,"video":{video_file},"thumbnail":{{"@type":"thumbnail","format":{{"@type":"thumbnailFormatJpeg"}},"width":90,"height":120,"file":{video_thumb_file}}}}},"alternative_video":null}},"caption":{}}}"#,
            caption("Demo video story — plays with the native player."),
        ),
        // Chat 11, story 5: photo story (unread).
        photo_story(5, 11, "Demo story — full-size photo render.", &photo_file),
        // Chat 12, story 6: photo story (read).
        photo_story(6, 12, "Demo chat B story.", &photo_file),
    ];
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

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
            .and_then(|session| session.story_tray.get(&chat_id.0))
            .map(|tray| {
                tray.stories
                    .iter()
                    .map(|info| info.story_id)
                    .filter(|id| {
                        !self
                            .session()
                            .is_some_and(|s| s.stories.contains_key(&(chat_id.0, *id)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(live) = self.live.as_mut() {
            for id in missing {
                let _ = live.driver.get_story(chat_id, id);
            }
        } else if !missing.is_empty() {
            self.status_note = "demo — getStory runs with live TDLib".into();
        }
        if !self.rebuild_story_viewer(chat_id, story_id, cx) {
            self.pending_story_open = Some((chat_id.0, story_id));
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
                    .story_tray
                    .get(&chat_id.0)
                    .map(|tray| collect_story_items(chat_id, tray, &session.stories))
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
            .and_then(|session| session.stories.get(&(item.chat_id.0, item.story_id)))
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
            self.status_note =
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
            self.status_note = "demo — setStoryReaction runs with live TDLib".into();
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
            self.status_note = match live.driver.join_live_story(chat_id, story_id) {
                Ok(_) => "joining live story…".into(),
                Err(_) => "could not join the live story".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — live stories join with live TDLib".into();
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
                if live.driver.session.story_available_reactions.is_none() {
                    match live.driver.get_story_available_reactions() {
                        Ok(_) => {}
                        Err(_) => self.status_note = "could not load story reactions".into(),
                    }
                }
            } else if self.demo_session.is_some() {
                self.status_note = "demo — story reactions run with live TDLib".into();
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
            self.status_note =
                match live
                    .driver
                    .set_story_reaction(item.chat_id, item.story_id, Some(emoji))
                {
                    Ok(_) => format!("Reacted {emoji}"),
                    Err(_) => "could not set story reaction".into(),
                };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — story reactions run with live TDLib".into();
        }
        self.stories.reaction_picker_open = false;
        cx.notify();
    }

    /// Phase 9.2+: display path for a custom-emoji reaction sticker
    /// (thumbnail first, else static WEBP — `StickerContent`'s rule) —
    /// `None` while the file isn't downloaded.
    fn story_custom_emoji_path(&self, custom_emoji_id: i64) -> Option<std::path::PathBuf> {
        let session = self.session()?;
        let sticker = session.story_custom_emoji_stickers.get(&custom_emoji_id)?;
        let file_id = sticker.display_file_id()?;
        let path = session.files.get(&file_id.0)?.usable_path()?;
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
            .story_available_reactions
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
            self.status_note = match live.driver.set_story_custom_emoji_reaction(
                item.chat_id,
                item.story_id,
                custom_emoji_id,
            ) {
                Ok(_) => "Reacted ✨".into(),
                Err(_) => "could not set story reaction".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — story reactions run with live TDLib".into();
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
                self.status_note = if quill::platform::open_external_url(&url) {
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
                self.status_note = if quill::platform::open_external_url(&url) {
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
                self.status_note = format!("{emoji} {temperature:.1}°C");
                cx.notify();
            }
            StoryAreaKind::Gift { gift_name } => {
                self.status_note = format!("🎁 {gift_name}");
                cx.notify();
            }
            StoryAreaKind::Unsupported { type_name } => {
                self.status_note = format!("story area {type_name} isn't supported");
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
            self.status_note = match live
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
            self.status_note = chat_search_jump_note(session);
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
            self.status_note =
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
            self.status_note = "demo — story replies run with live TDLib".into();
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
            self.status_note = match live.driver.delete_story(item.chat_id, item.story_id) {
                Ok(_) => "Deleting story…".into(),
                Err(_) => "could not delete story".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — deleteStory runs with live TDLib".into();
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
                self.status_note = "could not load story viewers".into();
            }
        } else if self.demo_session.is_some() {
            // Demo seeds the panel state directly (see the
            // `ReadyStoryViewers` demo); a live fetch says so honestly.
            self.status_note = "demo — story viewers run with live TDLib".into();
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
            .and_then(|session| session.story_viewers.as_ref())
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
            .and_then(|session| session.story_report.as_ref())
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
                .story_report
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
            self.status_note =
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
            self.status_note = "demo — story reports run with live TDLib".into();
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
            .and_then(|session| session.story_report.as_ref())
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
            self.status_note = "add details or cancel the report".into();
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
            .map(|session| session.story_stealth)
            .unwrap_or_default();
        if stealth.is_active(now) {
            self.status_note = "Stealth mode is active — your story views are hidden".into();
            cx.notify();
            return;
        }
        if stealth.is_cooling_down(now) {
            self.status_note = "Stealth mode is cooling down — try again later".into();
            cx.notify();
            return;
        }
        if let Some(live) = self.live.as_mut() {
            self.status_note = match live.driver.activate_story_stealth_mode() {
                Ok(_) => "Enabling stealth mode…".into(),
                Err(_) => "could not enable stealth mode".into(),
            };
        } else if self.demo_session.is_some() {
            self.status_note = "demo — stealth mode needs live TDLib".into();
        }
        cx.notify();
    }

    /// Phase 9.5: stealth button label for the action row.
    pub(super) fn story_stealth_label(&self) -> &'static str {
        let now = now_unix_secs();
        let stealth = self
            .session()
            .map(|session| session.story_stealth)
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
        self.pending_story_open = None;
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

    /// Trigger `downloadFile` for the current story when no display
    /// candidate is local yet (photo: largest size; video: thumbnail, else
    /// the clip itself). Live-only, like `ensure_viewer_download`.
    pub(super) fn ensure_story_download(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.stories.viewer.current().cloned() else {
            return;
        };
        let roots = self.media_display_roots();
        let local = self.session().is_some_and(|session| {
            item.display_file_ids.iter().any(|id| {
                session
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
    pub(super) fn ensure_story_custom_emoji_downloads(&mut self) {
        let ids = self.story_custom_emoji_fetch_ids();
        let file_ids: Vec<_> = {
            let Some(session) = self.session() else {
                return;
            };
            ids.iter()
                .filter_map(|id| {
                    let sticker = session.story_custom_emoji_stickers.get(id)?;
                    let file_id = sticker.display_file_id()?;
                    if file_id.0 == 0 {
                        return None;
                    }
                    match session.files.get(&file_id.0) {
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
    pub(super) fn story_viewer_counts(&self) -> Option<String> {
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
    pub(super) fn story_viewer_meta_line(&self) -> Option<String> {
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
    pub(super) fn story_reaction_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let reactions = self
            .session()
            .and_then(|session| session.story_available_reactions.clone())
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
    pub(super) fn story_action_row(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let manage_busy = self.session().is_some_and(|s| s.story_manage.pending);
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
            .and_then(|session| session.story_stealth_error.clone())
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
    pub(super) fn story_viewers_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.session().and_then(|s| s.story_viewers.clone());
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
    pub(super) fn story_report_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let flow = self.session().and_then(|s| s.story_report.clone());
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

    /// Phase 9.1: fullscreen story overlay, modeled on
    /// `media_viewer_overlay`: poster name + "Story N of M" header, the
    /// photo (video shows its thumbnail; live/unsupported show a
    /// placeholder), the caption, and Prev / Next / Close controls. The
    /// backdrop click and Escape (see `cancel_search`) close it.
    pub(super) fn story_viewer_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
        let files: HashMap<i32, ParsedFile> =
            self.session().map(|s| s.files.clone()).unwrap_or_default();
        let downloading: HashSet<i32> = self
            .session()
            .map(|s| s.downloading.clone())
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
    pub(super) fn story_progress_bar(
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

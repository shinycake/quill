use super::*;
use crate::ids::FileId;
use crate::telegram::story_areas::StoryAreaView;
use crate::telegram::story_areas::parse_story_areas;
use crate::text::TextEntity;
use serde_json::Value;

/// Phase 9.1: a story list identifier — `storyListMain` /
/// `storyListArchive` (TDLib 1.8.67, `schema/td_api.tl:6684-6690`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryListView {
    Main,
    Archive,
}

/// Phase 9.3: `canPostStoryResult*` — the `canPostStory` answer (TDLib
/// 1.8.67, `schema/td_api.tl:8535` – `td_api.tl:8553`). Only the fields
/// the composer shows survive: `retry_after` for the weekly/monthly
/// limits, `story_id` for the live-story conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanPostStoryResult {
    Ok { story_count: i32 },
    PremiumNeeded,
    BoostNeeded,
    ActiveStoryLimitExceeded,
    WeeklyLimitExceeded { retry_after: i32 },
    MonthlyLimitExceeded { retry_after: i32 },
    LiveStoryIsActive { story_id: i32 },
}

impl CanPostStoryResult {
    pub fn can_post(&self) -> bool {
        matches!(self, CanPostStoryResult::Ok { .. })
    }

    /// Honest one-line reason shown in the composer.
    pub fn user_message(&self) -> String {
        match self {
            CanPostStoryResult::Ok { .. } => "Eligible to post".to_string(),
            CanPostStoryResult::PremiumNeeded => {
                "Telegram Premium is required to post stories".to_string()
            }
            CanPostStoryResult::BoostNeeded => {
                "The chat needs more boosts before stories can be posted".to_string()
            }
            CanPostStoryResult::ActiveStoryLimitExceeded => {
                "Too many active stories — delete one or wait for the oldest to expire".to_string()
            }
            CanPostStoryResult::WeeklyLimitExceeded { retry_after } => format!(
                "Weekly story limit exceeded — try again in {}",
                format_retry_after(*retry_after)
            ),
            CanPostStoryResult::MonthlyLimitExceeded { retry_after } => format!(
                "Monthly story limit exceeded — try again in {}",
                format_retry_after(*retry_after)
            ),
            CanPostStoryResult::LiveStoryIsActive { .. } => {
                "A live story is active — delete it first".to_string()
            }
        }
    }
}

/// `retry_after` seconds → "2h 15m" / "3d". Kept local (one caller).
pub(crate) fn format_retry_after(seconds: i32) -> String {
    let seconds = seconds.max(0) as i64;
    let (days, rem) = (seconds / 86400, seconds % 86400);
    let (hours, rem) = (rem / 3600, rem % 3600);
    let minutes = rem / 60;
    if days > 0 {
        format!("{days}d")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Phase 9.1: `storyInfo` — basic information about one active story
/// (TDLib 1.8.67, `schema/td_api.tl:6767-6773`). `chatActiveStories.stories`
/// arrive in chronological order (increasing `story_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryInfoView {
    pub story_id: i32,
    pub date: i32,
    pub is_for_close_friends: bool,
    pub is_live: bool,
}

/// Phase 9.1: `chatActiveStories` — active stories posted by a chat (TDLib
/// 1.8.67, `schema/td_api.tl:6776-6783`). Only the tray needs are kept:
/// `list` (null when the stories are not shown in any story list),
/// `order` (tray sort key), `max_read_story_id` (unread rings), and the
/// `storyInfo` list. Dropped: `can_be_archived`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatActiveStoriesView {
    pub chat_id: i64,
    pub list: Option<StoryListView>,
    pub order: i64,
    pub max_read_story_id: i32,
    pub stories: Vec<StoryInfoView>,
}

impl ChatActiveStoriesView {
    /// True when any active story is newer than `max_read_story_id` — the
    /// tray ring renders unread.
    pub fn has_unread(&self) -> bool {
        self.stories
            .iter()
            .any(|story| story.story_id > self.max_read_story_id)
    }
}

/// Phase 9.1: story media Quill renders in the viewer: photo and video only.
/// Live stories and unsupported content keep the story item but render a
/// placeholder (no group-call join, no RTMP — out of scope).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryContentView {
    Photo {
        sizes: Vec<PhotoSizeView>,
    },
    Video {
        /// `storyVideo.thumbnail.file` (`thumbnail`, TDLib 1.8.67,
        /// `schema/td_api.tl:6633`) — the only display candidate; the full
        /// clip is not renderable by the image element (same call as the
        /// Phase 4.5 media viewer).
        thumb_file_id: Option<FileId>,
        thumb_width: i32,
        thumb_height: i32,
        /// `storyVideo.duration:double` — whole seconds for the label.
        duration_secs: i32,
        /// `storyVideo.video:file` — downloaded when there is no thumbnail
        /// so the file lands local.
        file_id: FileId,
    },
    Live,
    Unsupported,
}

/// Phase 9.2: `storyInteractionInfo` — interaction counters on a story
/// (TDLib 1.8.67, `schema/td_api.tl:6712`). Only populated by TDLib for
/// stories the current user posted (`story.can_get_interactions`); kept so
/// the viewer can render view/reaction counts on own stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoryInteractionInfoView {
    pub view_count: i32,
    pub forward_count: i32,
    pub reaction_count: i32,
}

impl StoryInteractionInfoView {
    /// True when at least one counter is nonzero.
    pub fn any_nonzero(&self) -> bool {
        self.view_count > 0 || self.forward_count > 0 || self.reaction_count > 0
    }
}

/// Phase 9.2: one emoji reaction the story picker can offer —
/// `availableReaction` (TDLib 1.8.67, `schema/td_api.tl:7321`). Only
/// `reactionTypeEmoji` entries render; custom-emoji entries are dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryAvailableReactionView {
    pub emoji: String,
    pub needs_premium: bool,
}

/// Phase 9.5: `storyRepostInfo` — the original story this story was
/// reposted from (TDLib 1.8.67, `schema/td_api.tl:6705`). `None` when the
/// story isn't a repost (`repost_info: null`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryRepostInfoView {
    pub origin: StoryOriginView,
    pub is_content_modified: bool,
}

/// Phase 9.5: `StoryOrigin` (TDLib 1.8.67, `schema/td_api.tl:6692`) —
/// where a reposted story came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryOriginView {
    /// `storyOriginPublicStory` (`td_api.tl:6696`) — a known public story.
    PublicStory { chat_id: i64, story_id: i32 },
    /// `storyOriginHiddenUser` (`td_api.tl:6699`) — an unknown poster's name.
    HiddenUser { poster_name: String },
}

/// Phase 9.1: `story` — the full story object (TDLib 1.8.67,
/// `schema/td_api.tl:6742`). Kept: ids, `date`, `content`, `caption`;
/// Phase 9.2 keeps: `chosen_reaction_type` (the user's own reaction),
/// `interaction_info` (view/forward/reaction counts), and the
/// `can_be_deleted` / `can_be_replied` / `can_get_interactions` gates.
/// Phase 9.5 keeps: `can_be_edited` / `can_set_privacy_settings` /
/// `can_be_forwarded` (edit / privacy / repost gates — the server folds
/// admin `can_post_stories` / `can_edit_stories` / `can_delete_stories`
/// rights into these), `is_edited`, `repost_info`, and the link +
/// suggested-reaction area texts (edit-surface prefill; other area types
/// stay out, same call as the 9.4 composer). Dropped: `privacy_settings`
/// (parsed separately via `StoryPrivacy::from_settings_json` only where
/// the privacy editor needs it), album ids, and the other `is_*` flags.
/// Phase 9.8 keeps: `areas` (clickable areas). `Eq` is not derived
/// because `StoryAreaKind::Weather` carries a `double` temperature.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedStory {
    pub id: i32,
    pub poster_chat_id: i64,
    pub date: i32,
    pub content: StoryContentView,
    pub caption: String,
    pub caption_entities: Vec<TextEntity>,
    /// Phase 9.2: the user's own reaction on this story (`emoji`), `None`
    /// when none is chosen or when the chosen reaction is a custom emoji /
    /// paid reaction (same call as message reactions).
    pub chosen_reaction_emoji: Option<String>,
    /// Phase 9.2+: the non-emoji half of the user's chosen reaction
    /// (`reactionTypeCustomEmoji` / `reactionTypePaid`) — `None` when the
    /// chosen reaction is a plain emoji or absent.
    pub chosen_reaction_extra: Option<StoryChosenExtraReaction>,
    /// Phase 9.2: `storyInteractionInfo` — counters, meaningful only when
    /// `can_get_interactions`.
    pub interaction_info: Option<StoryInteractionInfoView>,
    /// Phase 9.2: `story.can_be_deleted` — gates the viewer Delete button
    /// (`deleteStory`, `schema/td_api.tl:13754`).
    pub can_be_deleted: bool,
    /// Phase 9.2: `story.can_be_replied` — gates the viewer Reply affordance
    /// (`inputMessageReplyToStory`, `schema/td_api.tl:3099`).
    pub can_be_replied: bool,
    /// Phase 9.2: `story.can_get_interactions` — the interaction counters are
    /// the user's own.
    pub can_get_interactions: bool,
    /// Phase 9.5: `story.can_be_edited` — gates the viewer Edit button
    /// (`editStory`, `schema/td_api.tl:13732`).
    pub can_be_edited: bool,
    /// Phase 9.5: `story.can_set_privacy_settings` — gates the viewer
    /// Privacy editor (`setStoryPrivacySettings`, `td_api.tl:13743`).
    pub can_set_privacy_settings: bool,
    /// Phase 9.5: `story.can_be_forwarded` — gates the viewer Repost button
    /// (`postStory` with `from_story_full_id`, `td_api.tl:13715`; the
    /// schema comment: "True, if the story can be forwarded as a message
    /// or reposted as a story").
    pub can_be_forwarded: bool,
    /// Phase 9.5: `story.is_edited` — the viewer shows an "edited" marker.
    pub is_edited: bool,
    /// Phase 9.5: `story.repost_info` — the viewer shows "Reposted from …".
    pub repost_info: Option<StoryRepostInfoView>,
    /// Phase 9.5: raw `privacy_settings` (`StoryPrivacySettings`,
    /// `td_api.tl:6742`) — the privacy editor parses it back into
    /// `StoryPrivacy` for the prefill.
    pub privacy_settings: Option<serde_json::Value>,
    /// Phase 9.5: first `storyAreaTypeLink` URL (`td_api.tl:6552`) —
    /// prefills the edit surface's link input.
    pub area_link_url: Option<String>,
    /// Phase 9.5: `storyAreaTypeSuggestedReaction` emoji (`td_api.tl:6546`,
    /// `reactionTypeEmoji` only) — prefills the edit surface's reaction
    /// input.
    pub area_reaction_emojis: Vec<String>,
    /// Phase 9.7: `story.can_be_added_to_album` — gates the "Add to album"
    /// affordances (`createStoryAlbum`, `addStoryAlbumStories`,
    /// schema `td_api.tl:6724` comment: "True, if the story can be added
    /// to an album using createStoryAlbum and addStoryAlbumStories").
    pub can_be_added_to_album: bool,
    /// Phase 9.8: `story.areas` — clickable areas (`storyArea`,
    /// `schema/td_api.tl:6566`).
    pub areas: Vec<StoryAreaView>,
}

pub(crate) fn parse_story_list(value: Option<&Value>) -> Option<StoryListView> {
    match value
        .and_then(|value| value.get("@type"))
        .and_then(Value::as_str)
    {
        Some("storyListMain") => Some(StoryListView::Main),
        Some("storyListArchive") => Some(StoryListView::Archive),
        _ => None,
    }
}

pub(crate) fn parse_can_post_story_result(value: &Value) -> Option<CanPostStoryResult> {
    match value.get("@type").and_then(Value::as_str) {
        Some("canPostStoryResultOk") => Some(CanPostStoryResult::Ok {
            story_count: value
                .get("story_count")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        }),
        Some("canPostStoryResultPremiumNeeded") => Some(CanPostStoryResult::PremiumNeeded),
        Some("canPostStoryResultBoostNeeded") => Some(CanPostStoryResult::BoostNeeded),
        Some("canPostStoryResultActiveStoryLimitExceeded") => {
            Some(CanPostStoryResult::ActiveStoryLimitExceeded)
        }
        Some("canPostStoryResultWeeklyLimitExceeded") => {
            Some(CanPostStoryResult::WeeklyLimitExceeded {
                retry_after: value
                    .get("retry_after")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        }
        Some("canPostStoryResultMonthlyLimitExceeded") => {
            Some(CanPostStoryResult::MonthlyLimitExceeded {
                retry_after: value
                    .get("retry_after")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
            })
        }
        Some("canPostStoryResultLiveStoryIsActive") => {
            Some(CanPostStoryResult::LiveStoryIsActive {
                story_id: value.get("story_id").and_then(Value::as_i64).unwrap_or(0) as i32,
            })
        }
        _ => None,
    }
}

pub(crate) fn parse_story_info(value: &Value) -> Option<StoryInfoView> {
    Some(StoryInfoView {
        story_id: value.get("story_id")?.as_i64()? as i32,
        date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
        is_for_close_friends: value
            .get("is_for_close_friends")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_live: value
            .get("is_live")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn parse_chat_active_stories(value: &Value) -> Option<ChatActiveStoriesView> {
    Some(ChatActiveStoriesView {
        chat_id: int53(value.get("chat_id")).ok()?,
        list: parse_story_list(value.get("list")),
        order: int53_or_zero(value.get("order")),
        max_read_story_id: value
            .get("max_read_story_id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        stories: value
            .get("stories")
            .and_then(Value::as_array)
            .map(|stories| stories.iter().filter_map(parse_story_info).collect())
            .unwrap_or_default(),
    })
}

/// Parse one `story` object. Returns `None` when the required ids are
/// missing; unknown or missing `content` degrades to `Unsupported` rather
/// than failing the row. Collects the content's `file`s like the message
/// content parsers do.
pub(crate) fn parse_story(value: &Value) -> Option<(ParsedStory, Vec<ParsedFile>)> {
    let id = value.get("id")?.as_i64()? as i32;
    let poster_chat_id = int53(value.get("poster_chat_id")).ok()?;
    let (caption, caption_entities) = parse_caption(value.get("caption"));
    let mut files = Vec::new();
    let content = parse_story_content(value.get("content"), &mut files);
    files.retain(|file| file.id.0 != 0);
    // Phase 9.5: link + suggested-reaction area texts for the edit
    // surface prefill.
    let (area_link_url, area_reaction_emojis) = parse_story_area_texts(value.get("areas"));
    // Phase 9.2+: `chosen_reaction_type` splits into the emoji half and the
    // custom-emoji / paid half.
    let (chosen_reaction_emoji, chosen_reaction_extra) =
        parse_story_chosen_reaction(value.get("chosen_reaction_type"));
    Some((
        ParsedStory {
            id,
            poster_chat_id,
            date: value.get("date").and_then(Value::as_i64).unwrap_or(0) as i32,
            content,
            caption,
            caption_entities,
            chosen_reaction_emoji,
            chosen_reaction_extra,
            interaction_info: parse_story_interaction_info(value.get("interaction_info")),
            can_be_deleted: value
                .get("can_be_deleted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_be_replied: value
                .get("can_be_replied")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_get_interactions: value
                .get("can_get_interactions")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_be_edited: value
                .get("can_be_edited")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_set_privacy_settings: value
                .get("can_set_privacy_settings")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            can_be_forwarded: value
                .get("can_be_forwarded")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_edited: value
                .get("is_edited")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            repost_info: parse_story_repost_info(value.get("repost_info")),
            privacy_settings: value.get("privacy_settings").cloned(),
            area_link_url,
            area_reaction_emojis,
            can_be_added_to_album: value
                .get("can_be_added_to_album")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            areas: parse_story_areas(value.get("areas")),
        },
        files,
    ))
}

/// Phase 9.5: `storyRepostInfo` (`schema/td_api.tl:6705`) — `None` for
/// null / missing info or an unknown origin type.
pub(crate) fn parse_story_repost_info(value: Option<&Value>) -> Option<StoryRepostInfoView> {
    let info = value.filter(|value| !value.is_null())?;
    let origin = info.get("origin").filter(|origin| !origin.is_null())?;
    let origin = match origin.get("@type").and_then(Value::as_str) {
        Some("storyOriginPublicStory") => StoryOriginView::PublicStory {
            chat_id: int53(origin.get("chat_id")).ok()?,
            story_id: origin.get("story_id").and_then(Value::as_i64).unwrap_or(0) as i32,
        },
        Some("storyOriginHiddenUser") => StoryOriginView::HiddenUser {
            poster_name: origin
                .get("poster_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        _ => return None,
    };
    Some(StoryRepostInfoView {
        origin,
        is_content_modified: info
            .get("is_content_modified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Phase 9.5: pull the link URL + suggested-reaction emoji out of a
/// posted story's `areas` (output `storyArea`, `schema/td_api.tl:6566`)
/// so the edit surface can prefill the composer's text inputs. Other
/// area types stay out — same call as the 9.4 composer (text inputs can
/// only express link + reaction areas).
pub(crate) fn parse_story_area_texts(value: Option<&Value>) -> (Option<String>, Vec<String>) {
    let mut link_url = None;
    let mut reaction_emojis = Vec::new();
    let areas = value.and_then(Value::as_array);
    for area in areas.into_iter().flatten() {
        let area_type = area.get("type").filter(|t| !t.is_null());
        match area_type
            .and_then(|t| t.get("@type"))
            .and_then(Value::as_str)
        {
            // `storyAreaTypeLink` (td_api.tl:6552) — first one wins.
            Some("storyAreaTypeLink") if link_url.is_none() => {
                link_url = area_type
                    .and_then(|t| t.get("url"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            // `storyAreaTypeSuggestedReaction` (td_api.tl:6546) —
            // emoji reactions only, like the 9.2 chosen-reaction parse.
            Some("storyAreaTypeSuggestedReaction") => {
                let emoji = area_type
                    .and_then(|t| t.get("reaction_type"))
                    .filter(|t| !t.is_null())
                    .filter(|t| t.get("@type").and_then(Value::as_str) == Some("reactionTypeEmoji"))
                    .and_then(|t| t.get("emoji"))
                    .and_then(Value::as_str)
                    .filter(|emoji| !emoji.is_empty());
                if let Some(emoji) = emoji {
                    reaction_emojis.push(emoji.to_string());
                }
            }
            _ => {}
        }
    }
    (link_url, reaction_emojis)
}

/// Phase 9.2+: the non-emoji half of a story's `chosen_reaction_type`
/// (`reactionTypeCustomEmoji` / `reactionTypePaid`, TDLib 1.8.67,
/// `schema/td_api.tl:2918` / `:2921`). The Phase 9.2 parser dropped these;
/// they now ride alongside `chosen_reaction_emoji` so the post-Phase-9
/// viewer can render the real chosen state. `Copy` like the id it wraps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryChosenExtraReaction {
    CustomEmoji(i64),
    Paid,
}

/// Phase 9.2+: `chosen_reaction_type` on a `story` — the emoji half plus the
/// non-emoji half (`reactionTypeCustomEmoji` / `reactionTypePaid`, else
/// `None`). `(None, None)` when there is no reaction.
pub(crate) fn parse_story_chosen_reaction(
    value: Option<&Value>,
) -> (Option<String>, Option<StoryChosenExtraReaction>) {
    let reaction_type = match value.filter(|value| !value.is_null()) {
        Some(reaction_type) => reaction_type,
        None => return (None, None),
    };
    match reaction_type.get("@type").and_then(Value::as_str) {
        Some("reactionTypeEmoji") => (
            reaction_type
                .get("emoji")
                .and_then(Value::as_str)
                .filter(|emoji| !emoji.is_empty())
                .map(str::to_string),
            None,
        ),
        Some("reactionTypeCustomEmoji") => (
            None,
            int64(reaction_type.get("custom_emoji_id"))
                .filter(|id| *id > 0)
                .map(StoryChosenExtraReaction::CustomEmoji),
        ),
        Some("reactionTypePaid") => (None, Some(StoryChosenExtraReaction::Paid)),
        _ => (None, None),
    }
}

/// Phase 9.2: `storyInteractionInfo` counters; `None` when the field is
/// missing or null.
pub(crate) fn parse_story_interaction_info(
    value: Option<&Value>,
) -> Option<StoryInteractionInfoView> {
    let info = value.filter(|value| !value.is_null())?;
    Some(StoryInteractionInfoView {
        view_count: info.get("view_count").and_then(Value::as_i64).unwrap_or(0) as i32,
        forward_count: info
            .get("forward_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        reaction_count: info
            .get("reaction_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

/// Phase 9.2: one `availableReaction` row into the picker shape; drops
/// non-emoji reactions (custom emoji previews stay out of this slice).
pub(crate) fn parse_story_available_reaction(value: &Value) -> Option<StoryAvailableReactionView> {
    let reaction = value.get("type")?;
    if reaction.get("@type").and_then(Value::as_str) != Some("reactionTypeEmoji") {
        return None;
    }
    let emoji = reaction
        .get("emoji")
        .and_then(Value::as_str)
        .filter(|emoji| !emoji.is_empty())?
        .to_string();
    Some(StoryAvailableReactionView {
        emoji,
        needs_premium: value
            .get("needs_premium")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub(crate) fn parse_story_content(
    value: Option<&Value>,
    files: &mut Vec<ParsedFile>,
) -> StoryContentView {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return StoryContentView::Unsupported;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("storyContentPhoto") => match value.get("photo") {
            Some(photo) => {
                let (sizes, mut photo_files) = parse_photo_sizes(photo);
                files.append(&mut photo_files);
                StoryContentView::Photo { sizes }
            }
            None => StoryContentView::Unsupported,
        },
        Some("storyContentVideo") => {
            let video = value.get("video");
            let duration_secs = video
                .and_then(|video| video.get("duration"))
                .and_then(|duration| {
                    duration
                        .as_f64()
                        .or_else(|| duration.as_i64().map(|d| d as f64))
                })
                .unwrap_or(0.0) as i32;
            let file_id = video
                .and_then(|video| parse_file(video.get("video")).ok())
                .map(|file| {
                    let id = file.id;
                    files.push(file);
                    id
                })
                .unwrap_or(FileId(0));
            let thumb = video
                .and_then(|video| video.get("thumbnail"))
                .filter(|thumb| !thumb.is_null());
            let (thumb_file_id, thumb_width, thumb_height) = match thumb {
                Some(thumb) => (
                    parse_file(thumb.get("file"))
                        .ok()
                        .map(|file| {
                            let id = file.id;
                            files.push(file);
                            id
                        })
                        .filter(|id| id.0 != 0),
                    int53_or_zero(thumb.get("width")) as i32,
                    int53_or_zero(thumb.get("height")) as i32,
                ),
                None => (None, 0, 0),
            };
            StoryContentView::Video {
                thumb_file_id,
                thumb_width,
                thumb_height,
                duration_secs,
                file_id,
            }
        }
        Some("storyContentLive") => StoryContentView::Live,
        _ => StoryContentView::Unsupported,
    }
}

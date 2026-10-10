use super::*;
use crate::ids::FileId;
use serde_json::Value;

/// `advertisementSponsor` (TDLib 1.8.67): who backs a sponsored message.
/// `photo` is null when the sponsor must not show one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvertisementSponsor {
    pub url: String,
    pub photo: Option<PhotoContent>,
    pub info: String,
}

/// `sponsoredMessage` (TDLib 1.8.67). Content is text, animation, photo, or
/// video per the schema; `accent_color_id` / `background_custom_emoji_id` are
/// not rendered in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SponsoredMessage {
    /// int53; unique for the chat among both ordinary and sponsored messages.
    pub message_id: i64,
    pub is_recommended: bool,
    pub can_be_reported: bool,
    pub content: MessageContent,
    pub sponsor: AdvertisementSponsor,
    pub title: String,
    pub button_text: String,
    pub additional_info: String,
}

impl SponsoredMessage {
    /// "Recommended" when `is_recommended`, otherwise "Sponsored" (schema).
    pub fn kind_label(&self) -> &'static str {
        if self.is_recommended {
            "Recommended"
        } else {
            "Sponsored"
        }
    }

    /// The chip on the ad card: tdesktop's "Ad" (`lng_sponsored_message_title`),
    /// or "Recommended" when TDLib flags the message so.
    pub fn badge_label(&self) -> &'static str {
        if self.is_recommended {
            "Recommended"
        } else {
            "Ad"
        }
    }

    /// Thumb file ids worth auto-downloading at priority 1 (content + sponsor).
    pub fn thumb_file_ids(&self) -> Vec<FileId> {
        let mut ids = self.content_file_ids();
        if let Some(photo) = &self.sponsor.photo
            && let Some(size) = photo.thumb_size()
        {
            ids.push(size.file_id);
        }
        ids
    }

    /// Files the content needs before the ad may be shown.
    pub fn content_file_ids(&self) -> Vec<FileId> {
        let mut ids = Vec::new();
        match &self.content {
            MessageContent::Photo(photo) => {
                if let Some(size) = photo.thumb_size() {
                    ids.push(size.file_id);
                }
            }
            MessageContent::Animation(animation) => {
                if let Some(file_id) = animation.thumb_file_id() {
                    ids.push(file_id);
                }
            }
            MessageContent::Video(video) => {
                if let Some(file_id) = video.thumb_file_id() {
                    ids.push(file_id);
                }
            }
            _ => {}
        }
        ids
    }
}

/// `reportOption` (TDLib 1.8.67). `id` is `bytes` (base64 in JSON); echoed
/// back into `reportChatSponsoredMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOption {
    pub id: String,
    pub text: String,
}

/// `ReportSponsoredResult` (TDLib 1.8.67): outcome of `reportChatSponsoredMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportSponsoredResult {
    Ok,
    Failed,
    OptionRequired {
        title: String,
        options: Vec<ReportOption>,
    },
    AdsHidden,
    PremiumRequired,
}

/// `ReportChatResult` (TDLib 1.8.67, `schema/td_api.tl:9210-9219`):
/// the simple spam report from the chat list only handles `Ok`; the
/// message report flow walks `OptionRequired` / `TextRequired` the way
/// Telegram Desktop's `ShowReportFlowBox` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportChatOutcome {
    Ok,
    OptionRequired {
        title: String,
        options: Vec<ReportOption>,
    },
    TextRequired {
        option_id: String,
        is_optional: bool,
    },
    /// The report needs messages and none (or too few) were sent.
    MessagesRequired,
}

/// Phase 9.5: `ReportStoryResult` (TDLib 1.8.67,
/// `schema/td_api.tl:9222`–`9231`) — outcome of `reportStory`. Unlike
/// `ReportSponsoredResult` there is no `Failed` variant; a refused report
/// arrives as a raw `error` answer on the request instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportStoryResult {
    Ok,
    OptionRequired {
        title: String,
        options: Vec<ReportOption>,
    },
    TextRequired {
        option_id: String,
        is_optional: bool,
    },
}

/// Phase 9.5: one `storyInteraction` (TDLib 1.8.67,
/// `schema/td_api.tl:6805`) — who interacted with an own story, when,
/// and how. `storyInteractionTypeView` carries an optional chosen
/// reaction (`reactionTypeEmoji` only — custom/paid drop to `None`,
/// same as the story parser); forwards and reposts keep only their
/// kind (the embedded message/story is dropped — the viewers list
/// shows who and when, not the forwarded payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryInteractionView {
    pub actor: MessageSender,
    pub interaction_date: i32,
    pub reaction_emoji: Option<String>,
    /// Phase 9.2+: the non-emoji half of the viewer's chosen reaction
    /// (`reactionTypeCustomEmoji` / `reactionTypePaid`) — same split as
    /// `ParsedStory::chosen_reaction_extra`.
    pub reaction_extra: Option<StoryChosenExtraReaction>,
    pub kind: StoryInteractionKind,
}

impl StoryInteractionView {
    /// Short row suffix: the chosen reaction, or the interaction kind.
    pub fn kind_label(&self) -> String {
        if let Some(emoji) = self.reaction_emoji.as_deref() {
            return emoji.to_string();
        }
        match self.kind {
            StoryInteractionKind::View => "viewed".into(),
            StoryInteractionKind::Forward => "forwarded".into(),
            StoryInteractionKind::Repost => "reposted".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryInteractionKind {
    View,
    Forward,
    Repost,
}

/// Phase 9.5: `storyInteractions` (TDLib 1.8.67,
/// `schema/td_api.tl:6811`) — one page of `getStoryInteractions`
/// results. `next_offset` empty = no more pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryInteractionsView {
    pub total_count: i32,
    pub interactions: Vec<StoryInteractionView>,
    pub next_offset: String,
}

impl ReportSponsoredResult {
    /// Short user-facing note (no TDLib text is echoed).
    pub fn user_message(&self) -> &'static str {
        match self {
            ReportSponsoredResult::Ok => "Report sent",
            ReportSponsoredResult::Failed => "Could not report this message",
            ReportSponsoredResult::OptionRequired { .. } => "Choose a report reason",
            ReportSponsoredResult::AdsHidden => "Sponsored messages hidden",
            ReportSponsoredResult::PremiumRequired => {
                "Hiding sponsored messages needs Telegram Premium"
            }
        }
    }
}

/// `sponsoredMessages` (TDLib 1.8.67). Unparseable rows are skipped, like
/// other vector payloads.
pub(crate) fn parse_sponsored_messages(value: &Value) -> Result<EnvelopePayload, ParseError> {
    let mut messages = Vec::new();
    let mut files = Vec::new();
    if let Some(entries) = value.get("messages").and_then(Value::as_array) {
        for entry in entries {
            let Ok((message, message_files)) = parse_sponsored_message(entry) else {
                continue;
            };
            messages.push(message);
            files.extend(message_files);
        }
    }
    files.retain(|file| file.id.0 != 0);
    Ok(EnvelopePayload::Messages(
        MessagesPayload::SponsoredMessages {
            messages,
            files,
            messages_between: value
                .get("messages_between")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
        },
    ))
}

pub(crate) fn parse_sponsored_message(
    entry: &Value,
) -> Result<(SponsoredMessage, Vec<ParsedFile>), ParseError> {
    if entry.get("@type").and_then(Value::as_str) != Some("sponsoredMessage") {
        return Err(ParseError::MissingField);
    }
    let (content, mut files) = parse_content(entry.get("content"));
    let (sponsor, sponsor_files) = parse_advertisement_sponsor(entry.get("sponsor"));
    files.extend(sponsor_files);
    Ok((
        SponsoredMessage {
            message_id: int53(entry.get("message_id"))?,
            is_recommended: json_bool(entry.get("is_recommended"), false),
            can_be_reported: json_bool(entry.get("can_be_reported"), false),
            content,
            sponsor,
            title: json_field_str(entry, "title"),
            button_text: json_field_str(entry, "button_text"),
            additional_info: json_field_str(entry, "additional_info"),
        },
        files,
    ))
}

/// `advertisementSponsor` (TDLib 1.8.67). A null sponsor is an empty sponsor,
/// not an error.
pub(crate) fn parse_advertisement_sponsor(
    value: Option<&Value>,
) -> (AdvertisementSponsor, Vec<ParsedFile>) {
    let empty = (
        AdvertisementSponsor {
            url: String::new(),
            photo: None,
            info: String::new(),
        },
        Vec::new(),
    );
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return empty;
    };
    if value.get("@type").and_then(Value::as_str) != Some("advertisementSponsor") {
        return empty;
    }
    let (photo, files) = value
        .get("photo")
        .filter(|photo| !photo.is_null())
        .map(parse_sponsored_photo)
        .unwrap_or((None, Vec::new()));
    (
        AdvertisementSponsor {
            url: json_field_str(value, "url"),
            photo,
            info: json_field_str(value, "info"),
        },
        files,
    )
}

/// Sponsor photo is schema `photo`. Same shape as link-preview photos.
pub(crate) fn parse_sponsored_photo(photo: &Value) -> (Option<PhotoContent>, Vec<ParsedFile>) {
    let typed_photo = photo.get("@type").and_then(Value::as_str) == Some("photo");
    if !typed_photo && photo.get("sizes").and_then(Value::as_array).is_none() {
        return (None, Vec::new());
    }
    let (sizes, files) = parse_photo_sizes(photo);
    if sizes.is_empty() {
        return (None, Vec::new());
    }
    (
        Some(PhotoContent {
            has_stickers: false,
            caption: String::new(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes,
            is_secret: false,
            has_spoiler: false,
            minithumbnail: None,
        }),
        files,
    )
}

/// `reportOption` rows (`reportSponsoredResultOptionRequired.options`).
pub(crate) fn parse_report_options(value: Option<&Value>) -> Vec<ReportOption> {
    let mut options = Vec::new();
    let Some(entries) = value.and_then(Value::as_array) else {
        return options;
    };
    for entry in entries {
        if entry.get("@type").and_then(Value::as_str) != Some("reportOption") {
            continue;
        }
        options.push(ReportOption {
            id: entry
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            text: json_field_str(entry, "text"),
        });
    }
    options
}

/// Phase 9.5: `storyInteractions` — one page of an own story's viewers
/// (`getStoryInteractions`, `schema/td_api.tl:13819`). Entries with an
/// unparsable actor are skipped; the page itself is kept.
pub(crate) fn parse_story_interactions(value: &Value) -> StoryInteractionsView {
    let mut interactions = Vec::new();
    if let Some(entries) = value.get("interactions").and_then(Value::as_array) {
        for entry in entries {
            let Ok(actor) = parse_message_sender(entry.get("actor_id")) else {
                continue;
            };
            let interaction_type = entry.get("type");
            let (kind, (reaction_emoji, reaction_extra)) = match interaction_type
                .and_then(|t| t.get("@type"))
                .and_then(Value::as_str)
            {
                Some("storyInteractionTypeView") => (
                    StoryInteractionKind::View,
                    parse_story_chosen_reaction(
                        interaction_type.and_then(|t| t.get("chosen_reaction_type")),
                    ),
                ),
                Some("storyInteractionTypeForward") => {
                    (StoryInteractionKind::Forward, (None, None))
                }
                Some("storyInteractionTypeRepost") => (StoryInteractionKind::Repost, (None, None)),
                _ => continue,
            };
            interactions.push(StoryInteractionView {
                actor,
                interaction_date: entry
                    .get("interaction_date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                reaction_emoji,
                reaction_extra,
                kind,
            });
        }
    }
    StoryInteractionsView {
        total_count: value
            .get("total_count")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        interactions,
        next_offset: json_field_str(value, "next_offset"),
    }
}

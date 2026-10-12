//! Forwarded stories and story mentions (`messageStory`) in the open chat:
//! which stories to fetch with `getStory`, which pictures to download, and
//! what a story card should say. tdesktop `MediaStory`
//! (`data/data_media_types.cpp`) resolves the story through
//! `Data::Stories::resolve` and marks it expired when it is gone.
use super::*;
use crate::telegram::envelope::ServiceAction;

/// What a story card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryCardState {
    /// `getStory` has not answered yet.
    Loading,
    /// The story is cached and can be opened.
    Ready,
    /// The story is gone (`lng_forwarded_story_expired`).
    Expired,
}

/// What a story card draws from the cached story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryPreview {
    /// Picture files, sharpest first.
    pub picture_ids: Vec<FileId>,
    pub width: i32,
    pub height: i32,
    /// Whole seconds of a video story.
    pub duration_secs: Option<i32>,
    pub caption: String,
}

/// Stories are shown 9:16 when the file carries no size.
const DEFAULT_STORY_SIZE: (i32, i32) = (720, 1280);

/// The story a `messageStory` message points at: poster chat, story id and
/// whether it is a mention.
pub fn message_story_key(content: &MessageContent) -> Option<(i64, i32, bool)> {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::Story {
                via_mention,
                poster_chat_id,
                story_id,
            } if *poster_chat_id != 0 && *story_id != 0 => {
                Some((*poster_chat_id, *story_id, *via_mention))
            }
            _ => None,
        },
        _ => None,
    }
}

impl Session {
    /// Stories the open chat's story messages point at.
    fn open_chat_message_stories(&self) -> Vec<(i64, i32)> {
        let Some(history) = self.open_chat.and_then(|chat| self.histories.get(&chat.0)) else {
            return Vec::new();
        };
        history
            .messages
            .values()
            .filter_map(|message| message_story_key(&message.content))
            .map(|(chat, story, _)| (chat, story))
            .collect()
    }

    /// Message stories of the open chat that are not cached and were not
    /// asked for yet.
    pub(crate) fn message_story_candidates(&self) -> Vec<(ChatId, i32)> {
        self.open_chat_message_stories()
            .into_iter()
            .filter(|key| {
                !self.stories.stories.contains_key(key)
                    && !self.stories.reply_attempted.contains(key)
            })
            .map(|(chat, story)| (ChatId(chat), story))
            .collect()
    }

    /// Pictures of the cached message stories, for download.
    pub(crate) fn message_story_thumb_ids(&self) -> Vec<FileId> {
        self.open_chat_message_stories()
            .iter()
            .filter_map(|key| self.stories.stories.get(key))
            .flat_map(super::session_reply::story_thumb_candidates)
            .collect()
    }

    /// The state of the story behind a story message.
    pub fn story_card_state(&self, chat_id: i64, story_id: i32) -> StoryCardState {
        if self.stories.stories.contains_key(&(chat_id, story_id)) {
            StoryCardState::Ready
        } else if self.stories.missing.contains(&(chat_id, story_id)) {
            StoryCardState::Expired
        } else {
            StoryCardState::Loading
        }
    }

    /// The picture, size and caption of a cached story.
    pub fn story_preview(&self, chat_id: i64, story_id: i32) -> Option<StoryPreview> {
        use crate::telegram::envelope::StoryContentView;
        let story = self.stories.stories.get(&(chat_id, story_id))?;
        let (picture_ids, (width, height), duration_secs) = match &story.content {
            StoryContentView::Photo { sizes } => {
                let mut sorted: Vec<_> = sizes.iter().collect();
                sorted.sort_by_key(|size| {
                    std::cmp::Reverse(i64::from(size.width) * i64::from(size.height))
                });
                let dims = sorted
                    .first()
                    .map_or(DEFAULT_STORY_SIZE, |size| (size.width, size.height));
                (sorted.iter().map(|size| size.file_id).collect(), dims, None)
            }
            StoryContentView::Video {
                thumb_file_id,
                thumb_width,
                thumb_height,
                duration_secs,
                ..
            } => {
                let dims = if *thumb_width > 0 && *thumb_height > 0 {
                    (*thumb_width, *thumb_height)
                } else {
                    DEFAULT_STORY_SIZE
                };
                (
                    thumb_file_id.iter().copied().collect(),
                    dims,
                    Some(*duration_secs),
                )
            }
            StoryContentView::Live { .. } | StoryContentView::Unsupported => {
                (Vec::new(), DEFAULT_STORY_SIZE, None)
            }
        };
        Some(StoryPreview {
            picture_ids,
            width,
            height,
            duration_secs,
            caption: story.caption.clone(),
        })
    }

    /// Whether the story is an active, not yet read story of its poster:
    /// the mention picture's ring is then the unread gradient.
    pub fn story_is_unread(&self, chat_id: i64, story_id: i32) -> bool {
        self.stories.tray.get(&chat_id).is_some_and(|tray| {
            tray.stories
                .iter()
                .any(|info| info.story_id == story_id && story_id > tray.max_read_story_id)
        })
    }

    /// After a successful Stars payment for a paid-media message: the note
    /// tdesktop shows as a toast (`ShowPaidMediaUnlockedToast`, "Media
    /// Unlocked"). Only for paid media; an invoice message gets none.
    pub(crate) fn note_paid_media_unlocked(&mut self) {
        use crate::payment_invoice::InputInvoice;
        use crate::telegram::envelope::PaymentFormTypeData;
        let Some(InputInvoice::Message {
            chat_id,
            message_id,
        }) = self.payments.request.as_ref().map(|r| r.invoice.clone())
        else {
            return;
        };
        let Some(PaymentFormTypeData::Stars { star_count }) =
            self.payments.form.as_ref().map(|f| f.form_type.clone())
        else {
            return;
        };
        let paid_media = self
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| is_paid_media_content(&message.content));
        if !paid_media {
            return;
        }
        let chat = self
            .chats
            .get(&chat_id.0)
            .map_or("the chat", |chat| chat.title.as_str());
        self.payments.unlocked_notice = Some(media_unlocked_note(star_count, chat));
    }
}

/// Locked paid media, or the photo and video it turns into.
fn is_paid_media_content(content: &MessageContent) -> bool {
    match content {
        MessageContent::Photo(_) | MessageContent::Video(_) => true,
        MessageContent::Action(action) => {
            matches!(action.as_ref(), ServiceAction::PaidMedia { .. })
        }
        _ => false,
    }
}

/// `lng_credits_media_done_title` and `_text`, on one line.
pub fn media_unlocked_note(stars: i64, chat: &str) -> String {
    let count = if stars == 1 {
        "1 Star".to_string()
    } else {
        format!("{stars} Stars")
    };
    format!("Media Unlocked. {count} transferred to {chat}.")
}

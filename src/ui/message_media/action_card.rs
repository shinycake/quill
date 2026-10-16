//! Service-action content that draws as a media card in a bubble instead
//! of a centered pill: locked paid media and forwarded stories.

use super::paid_media::{paid_album_card, paid_media_card};
use super::story_card::story_card;
use super::*;
use quill::state::{HistoryMessage, message_story_key};
use quill::telegram::envelope::{MessageContent, PaidMediaPreview, ServiceAction};

/// A locked `messagePaidMedia`: price, locked previews, caption.
pub(in crate::ui) fn locked_paid_media(
    content: &MessageContent,
) -> Option<(i64, &[PaidMediaPreview], &str)> {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::PaidMedia {
                stars,
                locked,
                caption,
                ..
            } if !locked.is_empty() => Some((*stars, locked.as_slice(), caption.as_str())),
            _ => None,
        },
        _ => None,
    }
}

/// A bought paid-media album: its items and caption.
pub(in crate::ui) fn bought_paid_album(
    content: &MessageContent,
) -> Option<(&[MessageContent], &str)> {
    match content {
        MessageContent::Action(action) => match action.as_ref() {
            ServiceAction::PaidMedia {
                unlocked, caption, ..
            } if !unlocked.is_empty() => Some((unlocked.as_slice(), caption.as_str())),
            _ => None,
        },
        _ => None,
    }
}

/// A forwarded story (not a mention): poster chat and story id.
pub(in crate::ui) fn forwarded_story(content: &MessageContent) -> Option<(i64, i32)> {
    message_story_key(content)
        .filter(|(_, _, via_mention)| !via_mention)
        .map(|(chat, story, _)| (chat, story))
}

/// Whether the content is a bubble card, not a service row.
pub(in crate::ui) fn is_bubble_card(content: &MessageContent) -> bool {
    locked_paid_media(content).is_some()
        || bought_paid_album(content).is_some()
        || forwarded_story(content).is_some()
}

/// The card of a bubble-card action; `None` for other actions.
pub(in crate::ui) fn action_card(
    message: &HistoryMessage,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    if let Some((stars, locked, caption)) = locked_paid_media(&message.content) {
        return Some(paid_media_card(
            message.chat_id,
            message.id,
            stars,
            locked,
            caption,
            cx,
        ));
    }
    if let Some((unlocked, caption)) = bought_paid_album(&message.content) {
        return Some(paid_album_card(
            message,
            unlocked,
            caption,
            files,
            downloading,
            media_roots,
            cx,
        ));
    }
    let (poster, story) = forwarded_story(&message.content)?;
    Some(story_card(
        message.id.0 as u64,
        ChatId(poster),
        story,
        session,
        files,
        media_roots,
        cx,
    ))
}

#[cfg(test)]
mod tests {
    use super::{forwarded_story, is_bubble_card, locked_paid_media};
    use quill::telegram::envelope::{MessageContent, PaidMediaPreview, ServiceAction};

    fn action(action: ServiceAction) -> MessageContent {
        MessageContent::Action(Box::new(action))
    }

    fn story(via_mention: bool) -> MessageContent {
        action(ServiceAction::Story {
            via_mention,
            poster_chat_id: 9,
            story_id: 3,
        })
    }

    fn paid(locked: Vec<PaidMediaPreview>) -> MessageContent {
        action(ServiceAction::PaidMedia {
            stars: 10,
            locked,
            unlocked: Vec::new(),
            caption: String::new(),
        })
    }

    #[test]
    fn a_forwarded_story_is_a_bubble_card_and_a_mention_is_a_service_row() {
        assert_eq!(forwarded_story(&story(false)), Some((9, 3)));
        assert!(is_bubble_card(&story(false)));
        assert_eq!(forwarded_story(&story(true)), None);
        assert!(!is_bubble_card(&story(true)));
    }

    #[test]
    fn a_bought_album_is_a_card_with_its_items() {
        let items = vec![
            MessageContent::ScreenshotTaken,
            MessageContent::ScreenshotTaken,
        ];
        let album = action(ServiceAction::PaidMedia {
            stars: 5,
            locked: Vec::new(),
            unlocked: items,
            caption: "Pair".into(),
        });
        assert!(is_bubble_card(&album));
        assert!(locked_paid_media(&album).is_none());
        let (unlocked, caption) = super::bought_paid_album(&album).expect("album");
        assert_eq!(unlocked.len(), 2);
        assert_eq!(caption, "Pair");
    }

    #[test]
    fn paid_media_is_a_card_only_while_locked() {
        let preview = PaidMediaPreview {
            width: 4,
            height: 3,
            duration: 0,
            minithumbnail: None,
        };
        assert!(is_bubble_card(&paid(vec![preview])));
        assert!(locked_paid_media(&paid(Vec::new())).is_none());
        assert!(!is_bubble_card(&paid(Vec::new())));
    }
}

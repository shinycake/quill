//! What a reply to one checklist task or poll option points at: the text
//! the reply bar shows and the rows the right-click menu offers
//! (tdesktop `FillPollOptionPage`, `AddReplyAction` with `todoItemId`).
use super::ReplyItem;
use crate::telegram::envelope::{MessageContent, PollOptionProperties};

/// The text of the task or option `item` names inside `content`, or `None`
/// when the message has no such item (it was deleted, or is not a poll).
pub fn item_text(content: &MessageContent, item: &ReplyItem) -> Option<String> {
    match (content, item) {
        (MessageContent::Checklist(list), ReplyItem::Task(id)) => list
            .list
            .tasks
            .iter()
            .find(|task| task.id == *id)
            .map(|task| task.text.clone()),
        (MessageContent::Poll(poll), ReplyItem::PollOption(id)) => poll
            .poll
            .options
            .iter()
            .find(|option| option.id == *id)
            .map(|option| option.text.clone()),
        _ => None,
    }
}

/// Which rows the menu of one poll option shows (tdesktop's option page).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionMenu {
    /// "Reply to Option".
    pub reply: bool,
    /// "Copy Option".
    pub copy: bool,
    /// "Delete Item", only once TDLib said the option can be deleted.
    pub delete: bool,
}

/// `message_can_reply` is the message-level verdict, used until
/// `pollOptionProperties` arrives. A missing option shows nothing.
pub fn option_menu(
    has_option: bool,
    message_can_reply: bool,
    properties: Option<PollOptionProperties>,
) -> OptionMenu {
    OptionMenu {
        reply: has_option && properties.map_or(message_can_reply, |p| p.can_be_replied),
        copy: has_option,
        delete: has_option && properties.is_some_and(|p| p.can_be_deleted),
    }
}

#[cfg(test)]
mod tests {
    use super::{OptionMenu, item_text, option_menu};
    use crate::composer::ReplyItem;
    use crate::telegram::envelope::{
        Checklist, ChecklistContent, ChecklistTask, MessageContent, PollOptionProperties,
    };

    fn checklist() -> MessageContent {
        MessageContent::Checklist(ChecklistContent {
            list: Checklist {
                title: "Trip".into(),
                tasks: vec![
                    ChecklistTask {
                        id: 3,
                        text: "Passport".into(),
                        completed_by: None,
                        completion_date: 0,
                    },
                    ChecklistTask {
                        id: 7,
                        text: "Tickets".into(),
                        completed_by: None,
                        completion_date: 0,
                    },
                ],
                others_can_add_tasks: false,
                can_add_tasks: false,
                others_can_mark_tasks_as_done: false,
                can_mark_tasks_as_done: false,
            },
        })
    }

    #[test]
    fn a_task_is_found_by_its_id() {
        assert_eq!(
            item_text(&checklist(), &ReplyItem::Task(7)).as_deref(),
            Some("Tickets")
        );
        assert_eq!(item_text(&checklist(), &ReplyItem::Task(4)), None);
        assert_eq!(item_text(&checklist(), &ReplyItem::Message), None);
        assert_eq!(
            item_text(&checklist(), &ReplyItem::PollOption("0".into())),
            None
        );
    }

    #[test]
    fn delete_waits_for_the_properties() {
        let ask = PollOptionProperties::default();
        let allowed = PollOptionProperties {
            can_be_deleted: true,
            can_be_replied: true,
            ..ask
        };
        assert_eq!(
            option_menu(true, true, None),
            OptionMenu {
                reply: true,
                copy: true,
                delete: false
            }
        );
        assert!(option_menu(true, true, Some(allowed)).delete);
        assert!(!option_menu(true, true, Some(ask)).delete);
        assert!(!option_menu(true, true, Some(ask)).reply);
        assert_eq!(
            option_menu(false, true, Some(allowed)),
            OptionMenu {
                reply: false,
                copy: false,
                delete: false
            }
        );
    }
}

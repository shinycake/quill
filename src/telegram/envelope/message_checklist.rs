use super::*;
use serde_json::Value;

/// `checklistTask` (TDLib 1.8.67, `schema/td_api.tl:518`).
/// `completed_by` is `None` while the task is open; `completion_date` is 0
/// then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistTask {
    pub id: i32,
    pub text: String,
    pub completed_by: Option<MessageSender>,
    pub completion_date: i32,
}

impl ChecklistTask {
    /// A task counts as done when the server stamped a completion date.
    pub fn is_done(&self) -> bool {
        self.completion_date != 0
    }
}

/// `checklist` (TDLib 1.8.67, `schema/td_api.tl:532`). A checklist is a
/// "group checklist" when `others_can_mark_tasks_as_done` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checklist {
    pub title: String,
    pub tasks: Vec<ChecklistTask>,
    pub others_can_add_tasks: bool,
    pub can_add_tasks: bool,
    pub others_can_mark_tasks_as_done: bool,
    pub can_mark_tasks_as_done: bool,
}

impl Checklist {
    pub fn done_count(&self) -> usize {
        self.tasks.iter().filter(|task| task.is_done()).count()
    }
}

/// `messageChecklist` (TDLib 1.8.67, `schema/td_api.tl:5258`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistContent {
    pub list: Checklist,
}

/// `checklist` object parsed from its JSON.
pub(crate) fn parse_checklist(value: &Value) -> Checklist {
    let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Checklist {
        title: parse_formatted_text(value.get("title")),
        tasks: value
            .get("tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|task| ChecklistTask {
                id: task
                    .get("id")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
                text: parse_formatted_text(task.get("text")),
                completed_by: parse_message_sender(task.get("completed_by")).ok(),
                completion_date: task
                    .get("completion_date")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    .sat_i32(),
            })
            .collect(),
        others_can_add_tasks: flag("others_can_add_tasks"),
        can_add_tasks: flag("can_add_tasks"),
        others_can_mark_tasks_as_done: flag("others_can_mark_tasks_as_done"),
        can_mark_tasks_as_done: flag("can_mark_tasks_as_done"),
    }
}

/// `messageChecklist` (TDLib 1.8.67, `schema/td_api.tl:5258`).
pub(crate) fn parse_message_checklist(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    match value.get("list").filter(|list| list.is_object()) {
        Some(list) => (
            MessageContent::Checklist(ChecklistContent {
                list: parse_checklist(list),
            }),
            Vec::new(),
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "messageChecklist".into(),
            },
            Vec::new(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageContent, MessageSender, parse_message_checklist};

    #[test]
    fn parses_tasks_and_completion() {
        let value = serde_json::json!({
            "@type": "messageChecklist",
            "list": {
                "title": {"text": "Trip", "entities": []},
                "tasks": [
                    {"id": 1, "text": {"text": "Passport", "entities": []},
                     "completed_by": {"@type": "messageSenderUser", "user_id": 9},
                     "completion_date": 1700000000},
                    {"id": 2, "text": {"text": "Tickets", "entities": []}}
                ],
                "others_can_add_tasks": true,
                "can_add_tasks": true,
                "others_can_mark_tasks_as_done": true,
                "can_mark_tasks_as_done": true
            }
        });
        let (content, files) = parse_message_checklist(&value);
        assert!(files.is_empty());
        let MessageContent::Checklist(content) = content else {
            panic!("expected a checklist");
        };
        assert_eq!(content.list.title, "Trip");
        assert_eq!(content.list.tasks.len(), 2);
        assert!(content.list.tasks[0].is_done());
        assert_eq!(
            content.list.tasks[0].completed_by,
            Some(MessageSender::User { user_id: 9 })
        );
        assert!(!content.list.tasks[1].is_done());
        assert_eq!(content.list.tasks[1].completed_by, None);
        assert_eq!(content.list.done_count(), 1);
        assert!(content.list.can_add_tasks && content.list.can_mark_tasks_as_done);
    }

    #[test]
    fn missing_list_is_unsupported() {
        let (content, _) =
            parse_message_checklist(&serde_json::json!({"@type": "messageChecklist"}));
        assert!(matches!(content, MessageContent::Unsupported { .. }));
    }
}

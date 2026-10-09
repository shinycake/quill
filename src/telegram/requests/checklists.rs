use super::{SendReply, message_topic_value, send_reply_value};
use crate::ids::{ChatId, MessageId, RequestId};
use serde_json::{Value, json};

fn plain_formatted(text: &str) -> Value {
    json!({ "@type": "formattedText", "text": text, "entities": [] })
}

/// `markChecklistTasksAsDone` (TDLib 1.8.67, `schema/td_api.tl:12967`):
/// `markChecklistTasksAsDone chat_id:int53 message_id:int53
/// marked_as_done_task_ids:vector<int32>
/// marked_as_not_done_task_ids:vector<int32> = Ok;`. Needs Telegram
/// Premium and `messageProperties.can_mark_tasks_as_done`; the same method
/// also un-marks tasks (there is no separate "not done" request).
pub fn mark_checklist_tasks_as_done(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    done: &[i32],
    not_done: &[i32],
) -> String {
    json!({
        "@type": "markChecklistTasksAsDone",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "marked_as_done_task_ids": done,
        "marked_as_not_done_task_ids": not_done,
    })
    .to_string()
}

/// `inputChecklistTask` list (schema line 523): `(id, text)` pairs.
fn input_checklist_tasks(tasks: &[(i32, &str)]) -> Vec<Value> {
    tasks
        .iter()
        .map(|(id, text)| {
            json!({
                "@type": "inputChecklistTask",
                "id": id,
                "text": plain_formatted(text),
            })
        })
        .collect()
}

/// `addChecklistTasks` (TDLib 1.8.67, `schema/td_api.tl:12960`):
/// `addChecklistTasks chat_id:int53 message_id:int53
/// tasks:vector<inputChecklistTask> = Ok;`. Task ids must be unique in the
/// checklist, so the caller numbers them after the highest existing id.
pub fn add_checklist_tasks(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    tasks: &[(i32, &str)],
) -> String {
    json!({
        "@type": "addChecklistTasks",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "tasks": input_checklist_tasks(tasks),
    })
    .to_string()
}

/// Fields for `inputMessageChecklist` (TDLib 1.8.67, `schema/td_api.tl:6209`
/// / `inputChecklist` line 539).
pub struct ChecklistSend<'a> {
    pub title: &'a str,
    pub tasks: &'a [&'a str],
    pub others_can_add_tasks: bool,
    pub others_can_mark_tasks_as_done: bool,
    pub reply_to: Option<SendReply>,
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessageChecklist`. Tasks are numbered 1..=n.
pub fn send_checklist(extra: RequestId, chat_id: ChatId, checklist: ChecklistSend<'_>) -> String {
    let tasks: Vec<(i32, &str)> = checklist
        .tasks
        .iter()
        .enumerate()
        .map(|(index, text)| (index as i32 + 1, *text))
        .collect();
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(checklist.topic_id),
        "reply_to": send_reply_value(checklist.reply_to.as_ref()),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageChecklist",
            "checklist": {
                "@type": "inputChecklist",
                "title": plain_formatted(checklist.title),
                "tasks": input_checklist_tasks(&tasks),
                "others_can_add_tasks": checklist.others_can_add_tasks,
                "others_can_mark_tasks_as_done": checklist.others_can_mark_tasks_as_done,
            }
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn mark_tasks_shape() {
        let value = parse(&mark_checklist_tasks_as_done(
            RequestId(3),
            ChatId(7),
            MessageId(9),
            &[1, 2],
            &[4],
        ));
        assert_eq!(value["@type"], "markChecklistTasksAsDone");
        assert_eq!(value["chat_id"], 7);
        assert_eq!(value["message_id"], 9);
        assert_eq!(value["marked_as_done_task_ids"], json!([1, 2]));
        assert_eq!(value["marked_as_not_done_task_ids"], json!([4]));
    }

    #[test]
    fn add_tasks_shape() {
        let value = parse(&add_checklist_tasks(
            RequestId(3),
            ChatId(7),
            MessageId(9),
            &[(5, "Pack"), (6, "Leave")],
        ));
        assert_eq!(value["@type"], "addChecklistTasks");
        assert_eq!(value["tasks"][0]["@type"], "inputChecklistTask");
        assert_eq!(value["tasks"][0]["id"], 5);
        assert_eq!(value["tasks"][1]["text"]["text"], "Leave");
    }

    #[test]
    fn send_checklist_shape() {
        let value = parse(&send_checklist(
            RequestId(3),
            ChatId(7),
            ChecklistSend {
                title: "Trip",
                tasks: &["Passport", "Tickets"],
                others_can_add_tasks: true,
                others_can_mark_tasks_as_done: false,
                reply_to: None,
                topic_id: None,
            },
        ));
        assert_eq!(value["@type"], "sendMessage");
        let content = &value["input_message_content"];
        assert_eq!(content["@type"], "inputMessageChecklist");
        let list = &content["checklist"];
        assert_eq!(list["@type"], "inputChecklist");
        assert_eq!(list["title"]["text"], "Trip");
        assert_eq!(list["tasks"][0]["id"], 1);
        assert_eq!(list["tasks"][1]["id"], 2);
        assert_eq!(list["tasks"][1]["text"]["text"], "Tickets");
        assert_eq!(list["others_can_add_tasks"], true);
        assert_eq!(list["others_can_mark_tasks_as_done"], false);
    }
}

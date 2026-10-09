//! B15: pure checklist logic shared by the driver, the UI and the tests.
//!
//! Mirrors Telegram Desktop's `edit_todo_list_box.cpp` (creation limits,
//! "Please enter a title." / "Please enter at least one task.") and
//! `history_view_todo_list.cpp` ("N of M completed", tap to mark done).
//! Premium gating follows `PeerData::canCreateTodoLists`: Premium account,
//! not a broadcast channel, and the user may send polls (or it is a
//! private chat).

use crate::state::ChatSummary;
use crate::telegram::envelope::{ChatKind, Checklist};

/// `todo_title_length_max` default in tdesktop's `AppConfig`; TDLib's
/// `getOption("checklist_title_length_max")`.
pub const CHECKLIST_TITLE_MAX_CHARS: usize = 32;
/// `todo_item_length_max` default; `getOption("checklist_task_text_length_max")`.
pub const CHECKLIST_TASK_MAX_CHARS: usize = 64;
/// `todo_items_max` default; `getOption("checklist_task_count_max")`.
pub const CHECKLIST_TASKS_MAX: usize = 30;

/// Whether the attach menu offers "Checklist" in `chat`
/// (`PeerData::canCreateTodoLists`): Telegram Premium, no broadcast
/// channels, no secret chats (schema line 6207), and `can_send_polls`
/// unless it is a private chat.
pub fn can_create_checklist(chat: &ChatSummary, is_premium: bool) -> bool {
    if !is_premium || chat.is_channel() || matches!(chat.kind, ChatKind::Secret { .. }) {
        return false;
    }
    if matches!(chat.kind, ChatKind::Private { .. }) {
        return true;
    }
    chat.supported()
        && chat
            .permissions
            .as_ref()
            .is_none_or(|permissions| permissions.can_send_polls)
}

/// "N of M completed" / "None of M completed" under the checklist title
/// (`lng_todo_completed`, `lng_todo_completed_none`).
pub fn completed_label(done: usize, total: usize) -> String {
    if done == 0 {
        format!("None of {total} completed")
    } else {
        format!("{done} of {total} completed")
    }
}

/// "Checklist" for a personal list, "Group Checklist" once others may
/// change it (`lng_todo_title`, `lng_todo_title_group`).
pub fn checklist_kind_label(list: &Checklist) -> &'static str {
    if list.others_can_mark_tasks_as_done || list.others_can_add_tasks {
        "Group Checklist"
    } else {
        "Checklist"
    }
}

/// The `(marked_as_done, marked_as_not_done)` id lists for a tap on
/// `task_id`, or `None` when the tap is a no-op (task missing, or the
/// server did not say tasks can be marked).
pub fn toggle_task_request(list: &Checklist, task_id: i32) -> Option<(Vec<i32>, Vec<i32>)> {
    if !list.can_mark_tasks_as_done {
        return None;
    }
    let task = list.tasks.iter().find(|task| task.id == task_id)?;
    Some(if task.is_done() {
        (Vec::new(), vec![task_id])
    } else {
        (vec![task_id], Vec::new())
    })
}

/// Ids for tasks appended to `list`: unique, after the highest existing id.
pub fn next_task_ids(list: &Checklist, count: usize) -> Vec<i32> {
    let start = list.tasks.iter().map(|task| task.id).max().unwrap_or(0) + 1;
    (0..count as i32).map(|offset| start + offset).collect()
}

/// How many more tasks `list` can take (`lng_todo_create_limit`).
pub fn tasks_left(list: &Checklist) -> usize {
    CHECKLIST_TASKS_MAX.saturating_sub(list.tasks.len())
}

/// Validate task texts typed into the "Add Tasks" box against `list`: at
/// least one non-empty task, each within [`CHECKLIST_TASK_MAX_CHARS`] and
/// free of line feeds, and the total within [`CHECKLIST_TASKS_MAX`].
/// Returns the trimmed non-empty texts.
pub fn validate_added_tasks(
    list: &Checklist,
    texts: &[String],
) -> Result<Vec<String>, &'static str> {
    let tasks: Vec<String> = texts
        .iter()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect();
    if tasks.is_empty() {
        return Err("Please enter at least one task.");
    }
    if tasks.len() > tasks_left(list) {
        return Err("You have added the maximum number of tasks.");
    }
    if tasks
        .iter()
        .any(|task| task.chars().count() > CHECKLIST_TASK_MAX_CHARS || task.contains('\n'))
    {
        return Err("A task is too long (64 characters max).");
    }
    Ok(tasks)
}

/// A composer checklist dialog frozen at "Create".
#[derive(Debug, Clone, Default)]
pub struct ChecklistDraft {
    pub title: String,
    pub tasks: Vec<String>,
    /// "Allow Others to Add Tasks".
    pub others_can_add_tasks: bool,
    /// "Allow Others to Mark As Done".
    pub others_can_mark_tasks_as_done: bool,
}

impl ChecklistDraft {
    /// Non-empty task texts (trimmed), in dialog order.
    pub fn usable_tasks(&self) -> Vec<&str> {
        self.tasks
            .iter()
            .map(|task| task.trim())
            .filter(|task| !task.is_empty())
            .collect()
    }

    /// `None` when the draft is valid; otherwise the short user-facing
    /// reason (tdesktop's `lng_todo_choose_title` / `lng_todo_choose_tasks`).
    pub fn validate(&self) -> Option<&'static str> {
        let title = self.title.trim();
        if title.is_empty() {
            return Some("Please enter a title.");
        }
        if title.chars().count() > CHECKLIST_TITLE_MAX_CHARS {
            return Some("The title is too long (32 characters max).");
        }
        let tasks = self.usable_tasks();
        if tasks.is_empty() {
            return Some("Please enter at least one task.");
        }
        if tasks.len() > CHECKLIST_TASKS_MAX {
            return Some("You have added the maximum number of tasks.");
        }
        if tasks
            .iter()
            .any(|task| task.chars().count() > CHECKLIST_TASK_MAX_CHARS || task.contains('\n'))
        {
            return Some("A task is too long (64 characters max).");
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CHECKLIST_TASKS_MAX, ChecklistDraft, can_create_checklist, checklist_kind_label,
        completed_label, next_task_ids, tasks_left, toggle_task_request, validate_added_tasks,
    };
    use crate::telegram::envelope::{Checklist, ChecklistTask, MessageSender};

    fn task(id: i32, done: bool) -> ChecklistTask {
        ChecklistTask {
            id,
            text: format!("task {id}"),
            completed_by: done.then_some(MessageSender::User { user_id: 5 }),
            completion_date: if done { 1_700_000_000 } else { 0 },
        }
    }

    fn list(tasks: Vec<ChecklistTask>) -> Checklist {
        Checklist {
            title: "Trip".into(),
            tasks,
            others_can_add_tasks: false,
            can_add_tasks: true,
            others_can_mark_tasks_as_done: false,
            can_mark_tasks_as_done: true,
        }
    }

    #[test]
    fn completed_labels() {
        assert_eq!(completed_label(0, 4), "None of 4 completed");
        assert_eq!(completed_label(1, 4), "1 of 4 completed");
        assert_eq!(completed_label(4, 4), "4 of 4 completed");
    }

    #[test]
    fn kind_label_follows_group_flags() {
        let mut checklist = list(vec![]);
        assert_eq!(checklist_kind_label(&checklist), "Checklist");
        checklist.others_can_mark_tasks_as_done = true;
        assert_eq!(checklist_kind_label(&checklist), "Group Checklist");
    }

    #[test]
    fn toggle_flips_the_task_state() {
        let checklist = list(vec![task(1, false), task(2, true)]);
        assert_eq!(
            toggle_task_request(&checklist, 1),
            Some((vec![1], Vec::new()))
        );
        assert_eq!(
            toggle_task_request(&checklist, 2),
            Some((Vec::new(), vec![2]))
        );
        assert_eq!(toggle_task_request(&checklist, 9), None);
    }

    #[test]
    fn toggle_needs_permission() {
        let mut checklist = list(vec![task(1, false)]);
        checklist.can_mark_tasks_as_done = false;
        assert_eq!(toggle_task_request(&checklist, 1), None);
    }

    #[test]
    fn new_task_ids_follow_the_highest_id() {
        let checklist = list(vec![task(3, false), task(7, true)]);
        assert_eq!(next_task_ids(&checklist, 2), vec![8, 9]);
        assert_eq!(next_task_ids(&list(vec![]), 1), vec![1]);
    }

    #[test]
    fn added_tasks_are_validated_against_the_limit() {
        let checklist = list((1..=28).map(|id| task(id, false)).collect());
        assert_eq!(tasks_left(&checklist), 2);
        assert_eq!(
            validate_added_tasks(&checklist, &["  ".into()]),
            Err("Please enter at least one task.")
        );
        assert_eq!(
            validate_added_tasks(&checklist, &[" a ".into(), "b".into()]),
            Ok(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(
            validate_added_tasks(&checklist, &["a".into(), "b".into(), "c".into()]),
            Err("You have added the maximum number of tasks.")
        );
        assert_eq!(
            validate_added_tasks(&checklist, &["x".repeat(65)]),
            Err("A task is too long (64 characters max).")
        );
    }

    #[test]
    fn draft_validation() {
        let mut draft = ChecklistDraft::default();
        assert_eq!(draft.validate(), Some("Please enter a title."));
        draft.title = "Trip".into();
        assert_eq!(draft.validate(), Some("Please enter at least one task."));
        draft.tasks = vec!["  ".into(), "Passport".into()];
        assert_eq!(draft.validate(), None);
        assert_eq!(draft.usable_tasks(), vec!["Passport"]);
        draft.title = "x".repeat(33);
        assert_eq!(
            draft.validate(),
            Some("The title is too long (32 characters max).")
        );
        draft.title = "Trip".into();
        draft.tasks = (0..=CHECKLIST_TASKS_MAX).map(|i| format!("t{i}")).collect();
        assert_eq!(
            draft.validate(),
            Some("You have added the maximum number of tasks.")
        );
        draft.tasks = vec!["a".repeat(65)];
        assert_eq!(
            draft.validate(),
            Some("A task is too long (64 characters max).")
        );
    }

    #[test]
    fn creation_needs_premium_and_a_non_channel_chat() {
        use crate::state::ChatSummary;
        use crate::telegram::envelope::ChatKind;
        let private = ChatSummary {
            kind: ChatKind::Private {
                user_id: crate::ids::UserId(4),
            },
            ..crate::state::placeholder_chat(crate::ids::ChatId(1))
        };
        assert!(can_create_checklist(&private, true));
        assert!(!can_create_checklist(&private, false));
        let channel = ChatSummary {
            kind: ChatKind::Supergroup {
                supergroup_id: 2,
                is_channel: true,
            },
            ..private.clone()
        };
        assert!(!can_create_checklist(&channel, true));
        let secret = ChatSummary {
            kind: ChatKind::Secret {
                secret_chat_id: 3,
                user_id: crate::ids::UserId(4),
            },
            ..private.clone()
        };
        assert!(!can_create_checklist(&secret, true));
        let mut group = ChatSummary {
            kind: ChatKind::BasicGroup { basic_group_id: 5 },
            ..private
        };
        assert!(can_create_checklist(&group, true));
        let mut permissions = crate::telegram::ChatPermissions::all();
        permissions.can_send_polls = false;
        group.permissions = Some(permissions);
        assert!(!can_create_checklist(&group, true));
    }
}

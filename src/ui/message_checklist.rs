//! `messageChecklist` rendering (B15): the title, "N of M completed", one
//! row per task with a done mark and who completed it, and an "Add a task"
//! row. Mirrors tdesktop's `HistoryView::TodoList`
//! (`history_view_todo_list.cpp`).

use super::app::QuillApp;
use super::message_text::format_unix_date_time;
use super::pressable::PressableDiv;
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::checklist::{checklist_kind_label, completed_label};
use quill::ids::{ChatId, MessageId};
use quill::state::Session;
use quill::telegram::envelope::{Checklist, ChecklistTask, MessageSender};

/// Display name for the member who completed a task.
fn completer_name(session: Option<&Session>, sender: &MessageSender) -> String {
    match sender {
        MessageSender::User { user_id } => session
            .and_then(|session| session.user(*user_id))
            .map(|user| user.display_name())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("User {user_id}")),
        MessageSender::Chat { chat_id } => session
            .and_then(|session| session.chats.get(chat_id))
            .map(|chat| chat.title.clone())
            .unwrap_or_else(|| format!("Chat {chat_id}")),
    }
}

/// The caption under a done task: "Name · date" (tdesktop paints the
/// completer's userpic next to the check; Quill has the name instead).
pub(super) fn completion_caption(
    session: Option<&Session>,
    task: &ChecklistTask,
) -> Option<String> {
    if !task.is_done() {
        return None;
    }
    let when = format_unix_date_time(i64::from(task.completion_date));
    Some(match &task.completed_by {
        Some(sender) => format!("{} · {when}", completer_name(session, sender)),
        None => when,
    })
}

/// A checklist card. Tapping a task flips it when the server allows
/// marking (`checklist.can_mark_tasks_as_done`); the "Add a task" row shows
/// when `checklist.can_add_tasks`.
pub(super) fn checklist_body(
    chat_id: ChatId,
    message_id: MessageId,
    list: &Checklist,
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let total = list.tasks.len();
    let done = list.done_count();
    let fill = if total == 0 {
        0.0
    } else {
        done as f32 / total as f32
    };
    let mut body = div()
        .id(("checklist", message_id.0 as u64))
        .flex()
        .flex_col()
        .gap_1()
        .mt_1()
        .w(px(300.))
        .max_w_full()
        .child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child(checklist_kind_label(list)),
        )
        .child(
            super::bidi_line::aligned_block(list.title.clone())
                .text_sm()
                .font_semibold(),
        )
        .child(
            div()
                .text_xs()
                .text_color(text_muted())
                .child(completed_label(done, total)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .h(px(4.))
                .rounded_md()
                .overflow_hidden()
                .bg(bg_deep())
                .child(div().flex_grow(fill).bg(success()))
                .child(div().flex_grow(1.0 - fill)),
        );
    for task in &list.tasks {
        body = body.child(checklist_task_row(
            chat_id, message_id, list, task, session, cx,
        ));
    }
    if list.can_add_tasks {
        body = body.child(
            Button::new(format!("checklist-add-{}", message_id.0))
                .label("Add a task")
                .ghost()
                .text_color(accent())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_checklist_add_tasks(chat_id, message_id, window, cx);
                })),
        );
    }
    body.into_any_element()
}

fn checklist_task_row(
    chat_id: ChatId,
    message_id: MessageId,
    list: &Checklist,
    task: &ChecklistTask,
    session: Option<&Session>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let done = task.is_done();
    let task_id = task.id;
    let mark = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(18.))
        .h(px(18.))
        .rounded_full()
        .border_1()
        .border_color(if done { success() } else { border() })
        .bg(if done { success() } else { bg_canvas() })
        .text_xs()
        .text_color(text_bright())
        .child(if done { "\u{2713}" } else { "" });
    let mut text = div().flex().flex_col().flex_1().min_w_0().child(
        super::bidi_line::aligned_block(task.text.clone())
            .text_sm()
            .text_color(if done { text_muted() } else { text_menu() }),
    );
    if let Some(caption) = completion_caption(session, task) {
        text = text.child(
            super::bidi_line::aligned_block(caption)
                .text_xs()
                .text_color(text_muted()),
        );
    }
    let mut row = div()
        .id(("checklist-task", message_id.0 as u64 * 64 + task_id as u64))
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .child(mark)
        .child(text);
    if list.can_mark_tasks_as_done {
        row = row
            .role(gpui_kit::Role::Button)
            .aria_label(if done {
                format!("Mark \"{}\" as not done", task.text)
            } else {
                format!("Mark \"{}\" as done", task.text)
            })
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_checklist_task(chat_id, message_id, task_id, cx);
            }));
    }
    row.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::completion_caption;
    use quill::telegram::envelope::{ChecklistTask, MessageSender};

    #[test]
    fn open_tasks_have_no_caption() {
        let task = ChecklistTask {
            id: 1,
            text: "Passport".into(),
            completed_by: None,
            completion_date: 0,
        };
        assert_eq!(completion_caption(None, &task), None);
    }

    #[test]
    fn done_tasks_name_the_completer() {
        let task = ChecklistTask {
            id: 1,
            text: "Passport".into(),
            completed_by: Some(MessageSender::User { user_id: 7 }),
            completion_date: 1_700_000_000,
        };
        let caption = completion_caption(None, &task).unwrap();
        assert!(caption.starts_with("User 7 · "), "{caption}");
    }
}

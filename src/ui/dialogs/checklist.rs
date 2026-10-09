use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::checklist::{CHECKLIST_TASKS_MAX, ChecklistDraft};
use quill::ids::{ChatId, MessageId};

/// B15: the checklist composer ("New Checklist") or, with `add_to` set, the
/// "Add Tasks" box for an existing list. Mirrors tdesktop's
/// `EditTodoListBox`: a title, a task list that grows as you type, and the
/// "Allow Others to Add Tasks" / "Allow Others to Mark As Done" switches.
/// Text areas are created when the dialog opens (the rows are dynamic);
/// "Create" freezes them into a validated [`ChecklistDraft`].
pub struct ChecklistDialog {
    /// `Some` = adding tasks to this message's checklist.
    pub(crate) add_to: Option<(ChatId, MessageId)>,
    pub(crate) title_input: Entity<TextareaState>,
    pub(crate) task_inputs: Vec<Entity<TextareaState>>,
    pub(crate) others_can_add_tasks: bool,
    pub(crate) others_can_mark_tasks_as_done: bool,
    pub(crate) error: Option<&'static str>,
}

impl ChecklistDialog {
    pub(crate) fn new(
        add_to: Option<(ChatId, MessageId)>,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Self {
        let title_input = Self::textarea(window, cx, "Title");
        let task_inputs = vec![Self::textarea(window, cx, "Add a task...")];
        Self {
            add_to,
            title_input,
            task_inputs,
            others_can_add_tasks: false,
            others_can_mark_tasks_as_done: false,
            error: None,
        }
    }

    fn textarea(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        placeholder: &str,
    ) -> Entity<TextareaState> {
        cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(placeholder.to_string())
                .auto_grow(1, 2)
                .submit_on_enter(false)
        })
    }

    /// One more empty task row (until the 30-task limit).
    pub(crate) fn push_task_row(&mut self, window: &mut Window, cx: &mut Context<QuillApp>) {
        if self.task_inputs.len() < CHECKLIST_TASKS_MAX {
            self.task_inputs
                .push(Self::textarea(window, cx, "Add a task..."));
        }
    }

    pub(crate) fn task_texts(&self, cx: &App) -> Vec<String> {
        self.task_inputs
            .iter()
            .map(|input| input.read(cx).value().to_string())
            .collect()
    }

    /// Freeze the dialog inputs into a [`ChecklistDraft`].
    pub(crate) fn draft(&self, cx: &App) -> ChecklistDraft {
        ChecklistDraft {
            title: self.title_input.read(cx).value().to_string(),
            tasks: self.task_texts(cx),
            others_can_add_tasks: self.others_can_add_tasks,
            others_can_mark_tasks_as_done: self.others_can_mark_tasks_as_done,
        }
    }
}

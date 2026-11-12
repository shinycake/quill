use super::super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::checklist::{CHECKLIST_TASKS_MAX, ChecklistDraft};
use quill::ids::{ChatId, MessageId, RequestId};
use quill::lang::{keys, tr};
use quill::state::{Answer, RequestAcks};

/// Where a dialog's Create / Add stands. The dialog stays open, with what
/// was typed, while its request is out: the matching answer closes it, a
/// refusal returns it to editing. Closing the dialog ends the wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DialogSend {
    #[default]
    Editing,
    Sending {
        request: RequestId,
    },
}

impl DialogSend {
    pub(crate) fn request(self) -> Option<RequestId> {
        match self {
            Self::Editing => None,
            Self::Sending { request } => Some(request),
        }
    }

    /// Take the answer of the request waited for: the stage goes back to
    /// editing, and the answer is returned. `None` while nothing came.
    pub(crate) fn settle(&mut self, acks: &mut RequestAcks) -> Option<Answer> {
        let request = self.request()?;
        let answer = acks.take(request)?;
        *self = Self::Editing;
        Some(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::DialogSend;
    use quill::ids::RequestId;
    use quill::state::{Answer, RequestAcks};

    #[test]
    fn a_refusal_returns_the_dialog_to_editing_and_only_its_own_answer_counts() {
        let mut acks = RequestAcks::default();
        acks.expect(RequestId(7));
        acks.expect(RequestId(8));
        let mut send = DialogSend::Sending {
            request: RequestId(7),
        };
        // Another request's answer is not this dialog's.
        acks.settle(RequestId(8), Answer::Done);
        assert_eq!(send.settle(&mut acks), None);
        assert_eq!(send.request(), Some(RequestId(7)));
        acks.settle(RequestId(7), Answer::Failed("SLOWMODE_WAIT_5".into()));
        assert_eq!(
            send.settle(&mut acks),
            Some(Answer::Failed("SLOWMODE_WAIT_5".into()))
        );
        assert_eq!(send, DialogSend::Editing, "the box can be sent again");
        assert_eq!(send.settle(&mut acks), None);
    }
}

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
    pub(crate) others: OthersMay,
    /// Whether Create / Add is out (`checklists.rs`).
    pub(crate) send: DialogSend,
    /// The last refused Create, drawn under the fields; any edit to the
    /// title or a task clears it (see `watch`).
    pub(crate) error: Option<&'static str>,
    /// One per text area; dropping the dialog drops them.
    watchers: Vec<Subscription>,
    /// The field to focus once the dialog has mounted (the kit dialog
    /// takes focus when it opens, so focusing at open time is lost).
    pub(crate) focus_on_mount: Option<Entity<TextareaState>>,
}

/// What other members may do with the list: the two switches, one value.
#[derive(Debug, Default, Clone, Copy)]
pub struct OthersMay {
    pub(crate) add_tasks: bool,
    pub(crate) mark_tasks: bool,
}

impl ChecklistDialog {
    pub(crate) fn new(
        add_to: Option<(ChatId, MessageId)>,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Self {
        let title_input = Self::textarea(window, cx, tr(keys::Q_MEDIA_TITLE));
        let task_inputs = vec![Self::textarea(window, cx, tr(keys::Q_MEDIA_ADD_A_TASK))];
        let focus_on_mount = if add_to.is_some() {
            task_inputs.first().cloned()
        } else {
            Some(title_input.clone())
        };
        let mut dialog = Self {
            add_to,
            title_input,
            task_inputs,
            focus_on_mount,
            others: OthersMay::default(),
            send: DialogSend::Editing,
            error: None,
            watchers: Vec::new(),
        };
        dialog.watch_all(cx);
        dialog
    }

    /// Clear the shown refusal when the user edits a field, so a corrected
    /// list is not still marked wrong.
    fn watch(input: &Entity<TextareaState>, cx: &mut Context<QuillApp>) -> Subscription {
        // window-scope: held by the dialog in this window's chat_ui, dropped with it
        cx.subscribe(input, |app, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(dialog) = app.chat_ui.checklist_dialog.as_mut()
                && dialog.error.take().is_some()
            {
                cx.notify();
            }
        })
    }

    fn watch_all(&mut self, cx: &mut Context<QuillApp>) {
        let inputs = std::iter::once(&self.title_input).chain(self.task_inputs.iter());
        self.watchers = inputs.map(|input| Self::watch(input, cx)).collect();
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
            let row = Self::textarea(window, cx, tr(keys::Q_MEDIA_ADD_A_TASK));
            self.watchers.push(Self::watch(&row, cx));
            self.task_inputs.push(row);
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
            others_can_add_tasks: self.others.add_tasks,
            others_can_mark_tasks_as_done: self.others.mark_tasks,
        }
    }
}

//! Topic creates and saves whose manage dialog closed (Esc, the backdrop,
//! the close button) before TDLib answered. The answer is still the only
//! word on whether the topic was made or saved, so it ends in a toast
//! instead of going unseen.

use super::*;

/// The create (`target: None`) or save a closed dialog's editor sent. It
/// lives in this account's `AdminUi`, so a switch drops it with the screen.
pub(in crate::ui) struct ClosedTopicSend {
    target: Option<i32>,
    submit: Submit,
}

impl QuillApp {
    /// The manage dialog closed with `editor` in it: a send still out is
    /// kept until its answer.
    pub(super) fn keep_closed_topic_send(&mut self, editor: TopicEditor) {
        let requests = self.session().map(|session| &session.requests);
        if editor.submit.is_sending(requests) {
            self.admin.closed_topic_sends.push(ClosedTopicSend {
                target: editor.target,
                submit: editor.submit,
            });
        }
    }

    /// Each closed send that has its answer ends: the toast says the topic
    /// was created or saved, or why not.
    pub(super) fn settle_closed_topic_sends(&mut self, cx: &mut Context<Self>) {
        if self.admin.closed_topic_sends.is_empty() {
            return;
        }
        let sends = std::mem::take(&mut self.admin.closed_topic_sends);
        let requests = self.session().map(|session| &session.requests);
        let mut notes = Vec::new();
        let mut waiting = Vec::new();
        for send in sends {
            match send.submit.view(requests) {
                SubmitView::Sending => waiting.push(send),
                SubmitView::Editing => {}
                SubmitView::Done => notes.push(StatusNote::silent(tr(if send.target.is_none() {
                    keys::Q_PEOPLE_TOPIC_CREATED
                } else {
                    keys::Q_PEOPLE_TOPIC_SAVED
                }))),
                SubmitView::Failed(reason) => notes.push(StatusNote::error(reason)),
            }
        }
        self.admin.closed_topic_sends = waiting;
        if notes.is_empty() {
            return;
        }
        for note in notes {
            self.set_note(note);
        }
        cx.notify();
    }
}

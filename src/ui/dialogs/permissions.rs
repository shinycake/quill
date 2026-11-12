use quill::ids::{ChatId, RequestId};
use quill::permissions_box::{BoxContext, Permission};
use quill::state::{RequestRegistry, Submit, SubmitView};
use quill::telegram::envelope::ChatPermissions;

/// The group permissions box (tdesktop `ShowEditPeerPermissionsBox`):
/// the staged default permissions, slow mode and boosters values. Save
/// sends only what differs from the chat (`setChatPermissions`,
/// `setChatSlowModeDelay`, `setSupergroupUnrestrictBoostCount`).
pub struct PermissionsDialog {
    pub(crate) chat_id: ChatId,
    /// The chat's shape and the viewer's own rights, read on open.
    pub(crate) context: BoxContext,
    pub(crate) permissions: ChatPermissions,
    /// Whether the "Send media" group is expanded.
    pub(crate) media_open: bool,
    pub(crate) slow_mode_delay: i32,
    /// 0 when boosters are restricted like everyone else.
    pub(crate) boosts: i32,
    /// The locked row the viewer last clicked; its reason shows under it
    /// (tdesktop shows it as a toast).
    pub(crate) lock_hint: Option<Permission>,
    /// The last Save: the box closes on its answer, stays on its error.
    pub(crate) submit: PermissionsSubmit,
}

/// The stage of the box's Save: the plain [`Submit`], or a Save whose
/// later part was refused while an earlier part is still out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionsSubmit {
    Save(Submit),
    /// `sent` is out and the part after it was refused (`reason`): the box
    /// waits for `sent` (its own refusal shows first) and then shows
    /// `reason`, never as if everything was saved.
    Partial {
        sent: Vec<RequestId>,
        reason: String,
    },
}

impl Default for PermissionsSubmit {
    fn default() -> Self {
        PermissionsSubmit::Save(Submit::default())
    }
}

impl PermissionsSubmit {
    /// A Save that sent `ids`.
    pub(crate) fn sent(ids: Vec<RequestId>) -> Self {
        PermissionsSubmit::Save(Submit::Sending(ids))
    }

    pub(crate) fn view(&self, requests: Option<&RequestRegistry>) -> SubmitView {
        match self {
            PermissionsSubmit::Save(submit) => submit.view(requests),
            PermissionsSubmit::Partial { sent, reason } => {
                match Submit::Sending(sent.clone()).view(requests) {
                    SubmitView::Done => SubmitView::Failed(reason.clone()),
                    other => other,
                }
            }
        }
    }

    pub(crate) fn is_sending(&self, requests: Option<&RequestRegistry>) -> bool {
        self.view(requests) == SubmitView::Sending
    }

    /// Keep the failure `view` reported; the sent part is settled by now.
    /// `true` when the stage changed.
    pub(crate) fn keep_failure(&mut self, reason: String) -> bool {
        match self {
            PermissionsSubmit::Save(submit) => submit.keep_failure(reason),
            PermissionsSubmit::Partial { .. } => {
                *self = PermissionsSubmit::Save(Submit::Failed(reason));
                true
            }
        }
    }

    /// The line under the form.
    pub(crate) fn failure(&self) -> Option<&str> {
        match self {
            PermissionsSubmit::Save(Submit::Failed(reason)) => Some(reason),
            PermissionsSubmit::Partial { reason, .. } => Some(reason),
            PermissionsSubmit::Save(_) => None,
        }
    }
}

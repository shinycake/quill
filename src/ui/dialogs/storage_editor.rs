//! The Data & Storage per-network editor: one value for the stage it is in.
use super::super::app::QuillApp;
use quill::data_settings::{AutoDownloadNetSettings, NetworkKind};
use quill::ids::RequestId;
use quill::state::DialogAnswer;

/// The editor's stage. `Saving` holds the draft until TDLib answers the
/// request that carries it: `ok` closes the editor, an error reopens it with
/// the draft, so a refused save costs the user nothing. Leaving the page, or
/// the screen, drops it from any stage.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum StorageEditor {
    #[default]
    Closed,
    Open {
        network: NetworkKind,
        draft: AutoDownloadNetSettings,
    },
    Saving {
        network: NetworkKind,
        draft: AutoDownloadNetSettings,
        request: RequestId,
    },
}

impl StorageEditor {
    pub(crate) fn open(network: NetworkKind, draft: AutoDownloadNetSettings) -> Self {
        Self::Open { network, draft }
    }

    /// The network and draft the page draws (open or being saved).
    pub(crate) fn shown(&self) -> Option<(NetworkKind, AutoDownloadNetSettings)> {
        match *self {
            Self::Closed => None,
            Self::Open { network, draft } | Self::Saving { network, draft, .. } => {
                Some((network, draft))
            }
        }
    }

    /// The draft the sliders and switches edit: only while it is not sent.
    pub(crate) fn draft_mut(&mut self) -> Option<&mut AutoDownloadNetSettings> {
        match self {
            Self::Open { draft, .. } => Some(draft),
            Self::Closed | Self::Saving { .. } => None,
        }
    }

    /// The editor after the answer to the request it waits for (taken by
    /// that request's id, so no other request's answer reaches it): a save
    /// that went through closes it, a refusal reopens it with its draft.
    pub(crate) fn after_answer(self, answer: Option<DialogAnswer>) -> Self {
        match (self, answer) {
            (Self::Saving { network, draft, .. }, Some(DialogAnswer::Failed(_))) => {
                Self::Open { network, draft }
            }
            (Self::Saving { .. }, Some(_)) => Self::Closed,
            (stage, _) => stage,
        }
    }
}

impl QuillApp {
    /// A save the server answered closes or reopens the editor that sent it.
    /// Returns whether the editor changed.
    pub(crate) fn drain_storage_editor(&mut self) -> bool {
        let StorageEditor::Saving { request, .. } = self.settings.data_storage_editor else {
            return false;
        };
        let answer = self
            .session_mut()
            .and_then(|session| session.chat_list.dialog_answers.take(request));
        let next = self.settings.data_storage_editor.after_answer(answer);
        let changed = next != self.settings.data_storage_editor;
        self.settings.data_storage_editor = next;
        changed
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "storage_editor_tests.rs"]
mod tests;

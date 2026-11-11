use super::StorageEditor;
use quill::data_settings::{AutoDownloadNetSettings, NetworkKind};
use quill::ids::RequestId;
use quill::state::AutoDownloadAnswer;

fn saving(request: u64) -> StorageEditor {
    StorageEditor::Saving {
        network: NetworkKind::WiFi,
        draft: AutoDownloadNetSettings::default(),
        request: RequestId(request),
    }
}

#[test]
fn the_save_that_was_confirmed_closes_the_editor() {
    let after = saving(4).after_answer(Some(AutoDownloadAnswer::Saved(RequestId(4))));
    assert_eq!(after, StorageEditor::Closed);
}

#[test]
fn a_refused_save_reopens_the_editor_with_its_draft() {
    let mut draft = AutoDownloadNetSettings::default();
    draft.use_less_data_for_calls = !draft.use_less_data_for_calls;
    let editor = StorageEditor::Saving {
        network: NetworkKind::WiFi,
        draft,
        request: RequestId(4),
    };
    let after = editor.after_answer(Some(AutoDownloadAnswer::Refused(RequestId(4))));
    assert_eq!(after, StorageEditor::open(NetworkKind::WiFi, draft));
}

#[test]
fn another_requests_answer_leaves_the_save_waiting() {
    let editor = saving(4);
    assert_eq!(
        editor.after_answer(Some(AutoDownloadAnswer::Refused(RequestId(3)))),
        editor
    );
    assert_eq!(editor.after_answer(None), editor);
}

#[test]
fn a_draft_is_edited_only_while_it_is_not_sent() {
    let mut open = StorageEditor::open(NetworkKind::WiFi, AutoDownloadNetSettings::default());
    assert!(open.draft_mut().is_some());
    assert!(saving(1).draft_mut().is_none());
    assert!(StorageEditor::Closed.draft_mut().is_none());
}

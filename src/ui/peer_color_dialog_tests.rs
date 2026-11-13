#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The color box's Apply waits for the answers of its own requests
//! (sweep 11-044, 11-045): it is `Applied` only when every request was
//! accepted, `Failed` with the refusal when one was not, and another
//! request's answer never lands in it.

use crate::ui::peer_color_dialog::ApplyStage;
use quill::ids::RequestId;
use quill::state::{Answer, RequestAcks};

fn applying(ids: &[u64]) -> ApplyStage {
    ApplyStage::Applying {
        waiting: ids.iter().map(|id| RequestId(*id)).collect(),
        refused: None,
    }
}

fn answered(id: u64, answer: Answer) -> RequestAcks {
    let mut acks = RequestAcks::default();
    acks.expect(RequestId(id));
    acks.settle(RequestId(id), answer);
    acks
}

#[test]
fn applying_ends_applied_only_when_every_request_was_accepted() {
    let mut stage = applying(&[1, 2]);
    assert!(stage.settle(&mut answered(1, Answer::Done)));
    assert!(
        matches!(stage, ApplyStage::Applying { .. }),
        "2 still waits"
    );
    assert!(stage.settle(&mut answered(2, Answer::Done)));
    assert_eq!(stage, ApplyStage::Applied);
}

#[test]
fn a_refusal_ends_failed_in_either_order() {
    let mut stage = applying(&[1, 2]);
    stage.settle(&mut answered(2, Answer::Failed("PREMIUM".into())));
    assert!(matches!(stage, ApplyStage::Applying { .. }));
    stage.settle(&mut answered(1, Answer::Done));
    assert!(matches!(&stage, ApplyStage::Failed(why) if why.contains("PREMIUM")));

    let mut stage = applying(&[1, 2]);
    stage.settle(&mut answered(1, Answer::Done));
    stage.settle(&mut answered(2, Answer::Failed("PREMIUM".into())));
    assert!(matches!(stage, ApplyStage::Failed(_)));
}

#[test]
fn another_requests_answer_changes_nothing() {
    let mut stage = applying(&[1]);
    assert!(!stage.settle(&mut answered(9, Answer::Failed("X".into()))));
    assert_eq!(stage, applying(&[1]));
    let mut editing = ApplyStage::Editing;
    assert!(!editing.settle(&mut answered(1, Answer::Done)));
}

/// An account switch drops the box with its Apply still waiting: the answers
/// it waited for are forgotten with it, not left in the acks for good.
#[gpui_kit::test]
fn leaving_the_account_forgets_the_applys_waiting_answers(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) =
        crate::ui::notifications::viewer_progress_tests::open_app(cx, "ready-privacy");
    app.update(&mut vcx, |app, _| {
        app.dialogs.peer_color_dialog = Some(
            crate::ui::peer_color_dialog::PeerColorDialog::applying(vec![RequestId(7)]),
        );
        if let Some(session) = app.demo_session.as_mut() {
            session.acks.expect(RequestId(7));
            session.acks.settle(RequestId(7), Answer::Done);
        }
    });
    assert!(app.read_with(&vcx, |app, _| app.dialogs.peer_color_dialog.is_some()));
    app.update_in(&mut vcx, |app, window, cx| app.farewell_screen(window, cx));
    app.read_with(&vcx, |app, _| {
        assert!(app.dialogs.peer_color_dialog.is_none());
    });
    app.update(&mut vcx, |app, _| {
        let left = app
            .demo_session
            .as_mut()
            .and_then(|session| session.acks.take(RequestId(7)));
        assert!(left.is_none(), "the settled answer was forgotten");
    });
}

/// An Apply is on its way: opening the box again (the Settings window, a
/// color link) keeps the box and its answers, and edits wait for them.
#[gpui_kit::test]
fn an_apply_on_its_way_keeps_its_box_and_refuses_edits(cx: &mut gpui_kit::TestAppContext) {
    use quill::peer_color::PeerColorTab;
    let (app, mut vcx) =
        crate::ui::notifications::viewer_progress_tests::open_app(cx, "ready-privacy");
    app.update(&mut vcx, |app, cx| {
        app.dialogs.peer_color_dialog = Some(
            crate::ui::peer_color_dialog::PeerColorDialog::applying(vec![RequestId(7)]),
        );
        app.open_peer_color_dialog(PeerColorTab::Profile, cx);
        let dialog = app.dialogs.peer_color_dialog.as_ref().expect("the box");
        assert!(matches!(dialog.stage, ApplyStage::Applying { .. }), "kept");
        assert_eq!(dialog.tab, PeerColorTab::Profile, "on the tab asked for");
        let before = dialog.draft;
        app.edit_peer_color(cx, |draft, _| draft.name_color_id += 1);
        let dialog = app.dialogs.peer_color_dialog.as_ref().expect("the box");
        assert_eq!(dialog.draft, before, "no edit while applying");
        assert!(matches!(dialog.stage, ApplyStage::Applying { .. }));
    });
}

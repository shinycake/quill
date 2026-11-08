//! An agent's command belongs to the account it was asked of: a switch
//! before one of its steps (a notification click while the cursor glides)
//! stops the rest, which was planned on that account's tree.
use super::tests::{click, grant, message_box, new_app, read_tree, settle, turn_on};
use crate::ui::agent_state::TEST_LOCK;
use gpui_kit::TestAppContext;
use quill::agent::activity;
use quill::agent::grants::Scope;
use quill::agent::ui_driver::bridge::{self, Target, UiReply};
use quill::agent::ui_driver::{policy, takeover};
use quill::ids::AccountKey;

#[gpui_kit::test]
fn a_switch_between_steps_stops_the_command(cx: &mut TestAppContext) {
    let _guard = quill::locks::lock_or_recover(&TEST_LOCK);
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    turn_on();
    grant(11, Scope::ReadWrite);
    let _presence = activity::connect("Test agent");
    settle(vcx);
    let tree = read_tree(&app, vcx);

    // A click glides the ghost cursor first (`takeover::GLIDE`, a timer on
    // the test clock, which moves only when told): the switch lands while
    // it glides, before the click's step is checked.
    let ticket = bridge::post("Test agent", click(Target::Node(message_box(&tree))))
        .unwrap_or_else(|err| panic!("posted: {err:?}"));
    vcx.update(|window, cx| app.update(cx, |app, cx| app.agent_driver_tick(window, cx)));
    vcx.run_until_parked();
    assert!(ticket.poll().is_none(), "the click still glides");
    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
    vcx.executor().advance_clock(takeover::GLIDE);
    let mut answer = None;
    for _ in 0..40 {
        settle(vcx);
        if let Some(reply) = ticket.poll() {
            answer = Some(reply);
            break;
        }
    }
    let Some(UiReply::Refused(note)) = answer else {
        panic!("expected the click refused, got {answer:?}");
    };
    assert_eq!(note, policy::account_changed());
}

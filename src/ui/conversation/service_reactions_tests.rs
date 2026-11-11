//! A service message shows its reactions centred under the notice, and
//! follows them as they come and go (Telegram Desktop `Service` with a
//! centred reaction list).
use super::*;

const JOINED: &str = r#"{"@type":"updateNewMessage","message":{"id":210,"chat_id":31,"sender_id":{"@type":"messageSenderUser","user_id":21},"is_outgoing":false,"date":1790631900,"content":{"@type":"messageChatJoinByLink"}}}"#;

fn reactions(reacted: bool) -> String {
    let list = if reacted {
        r#","reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"🎉"},"total_count":2,"is_chosen":false,"recent_sender_ids":[]}],"are_tags":false,"can_get_added_reactions":true}"#
    } else {
        ""
    };
    format!(
        r#"{{"@type":"updateMessageInteractionInfo","chat_id":31,"message_id":210,"interaction_info":{{"@type":"messageInteractionInfo","view_count":0,"forward_count":0{list}}}}}"#
    )
}

#[gpui_kit::test]
fn a_service_message_shows_its_reactions_centred_under_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    apply(&app, vcx, &[JOINED]);
    app.update(vcx, |app, cx| app.jump_to_latest_messages(cx));
    frames(vcx);
    assert!(vcx.debug_bounds("service-row-210").is_some());
    assert!(vcx.debug_bounds("reaction-chips-210").is_none());
    let added = reactions(true);
    apply(&app, vcx, &[added.as_str()]);
    frames(vcx);
    let painted = ["service-row-210", "reaction-chips-210"].map(|id| vcx.debug_bounds(id));
    let [Some(row), Some(chips)] = painted else {
        panic!("the notice and its reactions are painted: {painted:?}");
    };
    assert!(row.top() <= chips.top() && chips.bottom() <= row.bottom());
    let middle =
        |bounds: gpui_kit::Bounds<gpui_kit::Pixels>| bounds.left() + bounds.size.width / 2.;
    assert!(
        (middle(row) - middle(chips)).abs() < px(2.),
        "the reactions are centred: {row:?} {chips:?}"
    );
    let removed = reactions(false);
    apply(&app, vcx, &[removed.as_str()]);
    frames(vcx);
    assert!(vcx.debug_bounds("reaction-chips-210").is_none());
}

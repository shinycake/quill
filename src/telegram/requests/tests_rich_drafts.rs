//! `setChatDraftMessage` with a rich draft
//! (`draftMessageContentInputRichMessage`, schema `td_api.tl` line 3858).
use crate::composer::DraftTopic;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::rich::RichDraft;
use crate::telegram::requests::{SendReply, set_rich_draft_message};
use serde_json::Value;

#[test]
fn a_rich_draft_saves_its_input_blocks_with_the_reply() {
    let draft = RichDraft::from_markup("# Plan\n\n- [ ] ship").expect("rich");
    let input = draft.input().expect("input blocks");
    let save = set_rich_draft_message(
        RequestId(7),
        ChatId(11),
        DraftTopic::Forum(3),
        input,
        Some(&SendReply::plain(MessageId(101))),
        None,
    );
    let v: Value = serde_json::from_str(&save).expect("json");
    assert_eq!(v["@type"], "setChatDraftMessage");
    assert_eq!(v["topic_id"]["forum_topic_id"], 3);
    let draft = &v["draft_message"];
    assert_eq!(draft["@type"], "draftMessage");
    assert_eq!(draft["reply_to"]["message_id"], 101);
    assert_eq!(draft["effect_id"], "0");
    assert_eq!(
        draft["content"]["@type"],
        "draftMessageContentInputRichMessage"
    );
    let message = &draft["content"]["message"];
    assert_eq!(message["@type"], "inputRichMessage");
    assert_eq!(message["source"]["@type"], "richMessageSourceBlocks");
    assert_eq!(
        message["source"]["blocks"][0]["@type"],
        "inputPageBlockSectionHeading"
    );
    assert_eq!(
        message["source"]["blocks"][1]["@type"],
        "inputPageBlockList"
    );
}

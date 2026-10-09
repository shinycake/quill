use crate::ids::RequestId;
use crate::telegram::requests::*;
use serde_json::{Value, json};

#[test]
fn new_chats_requests_match_the_schema() {
    // `getChatFolderNewChats chat_folder_id:int32 = Chats` (line 13809).
    let v: Value = serde_json::from_str(&get_chat_folder_new_chats(RequestId(1), 7)).unwrap();
    assert_eq!(v["@type"], "getChatFolderNewChats");
    assert_eq!(v["chat_folder_id"], 7);
    // `processChatFolderNewChats chat_folder_id:int32
    // added_chat_ids:vector<int53> = Ok` (line 13812).
    let v: Value =
        serde_json::from_str(&process_chat_folder_new_chats(RequestId(2), 7, &[41, 42])).unwrap();
    assert_eq!(v["@type"], "processChatFolderNewChats");
    assert_eq!(v["chat_folder_id"], 7);
    assert_eq!(v["added_chat_ids"], json!([41, 42]));
    // Dismissing sends an empty list.
    let v: Value =
        serde_json::from_str(&process_chat_folder_new_chats(RequestId(3), 7, &[])).unwrap();
    assert_eq!(v["added_chat_ids"], json!([]));
}

#[test]
fn read_folder_and_premium_limit_requests_match_the_schema() {
    // `readChatList chat_list:ChatList = Ok` with `chatListFolder`.
    let v: Value = serde_json::from_str(&read_chat_folder(RequestId(4), 9)).unwrap();
    assert_eq!(v["@type"], "readChatList");
    assert_eq!(v["chat_list"]["@type"], "chatListFolder");
    assert_eq!(v["chat_list"]["chat_folder_id"], 9);
    // `getPremiumLimit limit_type:PremiumLimitType = PremiumLimit`.
    let v: Value = serde_json::from_str(&get_premium_limit(
        RequestId(5),
        "premiumLimitTypeChatFolderCount",
    ))
    .unwrap();
    assert_eq!(v["@type"], "getPremiumLimit");
    assert_eq!(v["limit_type"]["@type"], "premiumLimitTypeChatFolderCount");
}

#[test]
fn the_tag_colour_is_sent_with_the_folder() {
    let spec = crate::telegram::envelope::ChatFolderSpec {
        name: "Work".into(),
        color_id: 4,
        ..Default::default()
    };
    let v: Value = serde_json::from_str(&create_chat_folder(RequestId(6), &spec)).unwrap();
    assert_eq!(v["folder"]["color_id"], 4);
    // -1 switches the tag off.
    let none = crate::telegram::envelope::ChatFolderSpec {
        color_id: -1,
        ..spec
    };
    let v: Value = serde_json::from_str(&edit_chat_folder(RequestId(7), 3, &none)).unwrap();
    assert_eq!(v["folder"]["color_id"], -1);
}

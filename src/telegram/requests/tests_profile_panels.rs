use crate::ids::{ChatId, RequestId};
use crate::telegram::requests::{
    add_contact, get_chat_similar_chats, get_groups_in_common, get_suitable_personal_chats,
    get_user_profile_photos, report_chat_photo, send_contact, set_birthdate, set_personal_chat,
    set_profile_photo_previous, set_user_note, set_user_personal_profile_photo,
    suggest_user_profile_photo,
};
use serde_json::Value;

fn parse(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

#[test]
fn set_birthdate_sends_the_day_month_and_optional_year() {
    let v = parse(&set_birthdate(RequestId(1), Some((9, 10, Some(1990)))));
    assert_eq!(v["@type"], "setBirthdate");
    assert_eq!(v["birthdate"]["@type"], "birthdate");
    assert_eq!(v["birthdate"]["day"], 9);
    assert_eq!(v["birthdate"]["month"], 10);
    assert_eq!(v["birthdate"]["year"], 1990);
    let no_year = parse(&set_birthdate(RequestId(2), Some((1, 2, None))));
    assert_eq!(no_year["birthdate"]["year"], 0);
    let removed = parse(&set_birthdate(RequestId(3), None));
    assert!(removed["birthdate"].is_null());
}

#[test]
fn personal_channel_requests_match_the_schema() {
    let v = parse(&get_suitable_personal_chats(RequestId(4)));
    assert_eq!(v["@type"], "getSuitablePersonalChats");
    let v = parse(&set_personal_chat(RequestId(5), -1001));
    assert_eq!(v["@type"], "setPersonalChat");
    assert_eq!(v["chat_id"], -1001);
    assert_eq!(parse(&set_personal_chat(RequestId(6), 0))["chat_id"], 0);
}

#[test]
fn note_and_contact_edits_carry_the_note_and_share_flag() {
    let v = parse(&set_user_note(RequestId(7), 31, "met at the fair"));
    assert_eq!(v["@type"], "setUserNote");
    assert_eq!(v["user_id"], 31);
    assert_eq!(v["note"]["@type"], "formattedText");
    assert_eq!(v["note"]["text"], "met at the fair");
    let v = parse(&add_contact(
        RequestId(8),
        31,
        "",
        "Ada",
        "Lovelace",
        "likes engines",
        true,
    ));
    assert_eq!(v["contact"]["note"]["text"], "likes engines");
    assert_eq!(v["share_phone_number"], true);
}

#[test]
fn list_requests_match_the_schema() {
    let v = parse(&get_groups_in_common(RequestId(9), 31, 0, 50));
    assert_eq!(v["@type"], "getGroupsInCommon");
    assert_eq!(v["user_id"], 31);
    assert_eq!(v["offset_chat_id"], 0);
    assert_eq!(v["limit"], 50);
    let v = parse(&get_chat_similar_chats(RequestId(10), ChatId(-1005)));
    assert_eq!(v["@type"], "getChatSimilarChats");
    assert_eq!(v["chat_id"], -1005);
    let v = parse(&get_user_profile_photos(RequestId(11), 31, 0, 100));
    assert_eq!(v["@type"], "getUserProfilePhotos");
    assert_eq!(v["limit"], 100);
}

#[test]
fn set_main_photo_reuses_a_previous_photo() {
    let v = parse(&set_profile_photo_previous(RequestId(12), 555));
    assert_eq!(v["@type"], "setProfilePhoto");
    assert_eq!(v["photo"]["@type"], "inputChatPhotoPrevious");
    assert_eq!(v["photo"]["chat_photo_id"], 555);
}

#[test]
fn send_contact_wraps_a_contact_in_input_message_contact() {
    let v = parse(&send_contact(
        RequestId(13),
        ChatId(77),
        None,
        "+15550131",
        "Ada",
        "Lovelace",
        31,
    ));
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["chat_id"], 77);
    let content = &v["input_message_content"];
    assert_eq!(content["@type"], "inputMessageContact");
    assert_eq!(content["contact"]["@type"], "contact");
    assert_eq!(content["contact"]["phone_number"], "+15550131");
    assert_eq!(content["contact"]["user_id"], 31);
}

#[test]
fn personal_photo_sets_a_static_local_file() {
    let v = parse(&set_user_personal_profile_photo(
        RequestId(14),
        31,
        Some("/tmp/a.jpg"),
    ));
    assert_eq!(v["@type"], "setUserPersonalProfilePhoto");
    assert_eq!(v["user_id"], 31);
    assert_eq!(v["photo"]["@type"], "inputChatPhotoStatic");
    assert_eq!(v["photo"]["photo"]["@type"], "inputFileLocal");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/a.jpg");
}

#[test]
fn personal_photo_removal_sends_null() {
    let v = parse(&set_user_personal_profile_photo(RequestId(15), 31, None));
    assert!(v["photo"].is_null());
}

#[test]
fn suggest_photo_sends_a_static_local_file() {
    let v = parse(&suggest_user_profile_photo(RequestId(16), 31, "/tmp/b.png"));
    assert_eq!(v["@type"], "suggestUserProfilePhoto");
    assert_eq!(v["photo"]["photo"]["path"], "/tmp/b.png");
}

#[test]
fn report_photo_carries_file_reason_and_text() {
    let v = parse(&report_chat_photo(
        RequestId(17),
        31,
        904,
        "reportReasonSpam",
        "",
    ));
    assert_eq!(v["@type"], "reportChatPhoto");
    assert_eq!(v["chat_id"], 31);
    assert_eq!(v["file_id"], 904);
    assert_eq!(v["reason"]["@type"], "reportReasonSpam");
}

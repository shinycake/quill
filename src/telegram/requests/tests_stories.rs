use crate::ids::{ChatId, RequestId};
use crate::story_composer::{StoryMediaKind, StoryPrivacy};
use crate::telegram::requests::*;
use serde_json::json;

#[test]
fn story_request_shapes_match_1_8_67() {
    // Phase 9.2 story reactions / picker / delete / reply builders.
    let set = set_story_reaction(RequestId(70), ChatId(11), 7, Some("❤"));
    let v: serde_json::Value = serde_json::from_str(&set).unwrap();
    // `setStoryReaction story_poster_chat_id:int53 story_id:int32
    // reaction_type:ReactionType update_recent_reactions:Bool = Ok`
    // (schema 1.8.67 line 13809).
    assert_eq!(v["@type"], "setStoryReaction");
    assert_eq!(v["@extra"], "70");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 7);
    assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
    assert_eq!(v["reaction_type"]["emoji"], "❤");
    assert_eq!(v["update_recent_reactions"], true);

    // Removing sends `reaction_type: null` (schema comment, line 13809).
    let remove = set_story_reaction(RequestId(71), ChatId(11), 7, None);
    let v: serde_json::Value = serde_json::from_str(&remove).unwrap();
    assert_eq!(v["@type"], "setStoryReaction");
    assert_eq!(v["reaction_type"], serde_json::Value::Null);

    // `getStoryAvailableReactions row_size:int32 = AvailableReactions`
    // (schema 1.8.67 line 13802); row_size 10 is inside 5–25.
    let avail = get_story_available_reactions(RequestId(72), 10);
    let v: serde_json::Value = serde_json::from_str(&avail).unwrap();
    assert_eq!(v["@type"], "getStoryAvailableReactions");
    assert_eq!(v["row_size"], 10);

    // `deleteStory story_poster_chat_id:int53 story_id:int32 = Ok`
    // (schema 1.8.67 line 13754).
    let delete = delete_story(RequestId(73), ChatId(11), 7);
    let v: serde_json::Value = serde_json::from_str(&delete).unwrap();
    assert_eq!(v["@type"], "deleteStory");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 7);

    // Story reply: `sendMessage` with `inputMessageReplyToStory
    // story_poster_chat_id:int53 story_id:int32 = InputMessageReplyTo`
    // (schema 1.8.67 line 3099).
    let reply = send_text_story_reply(RequestId(74), ChatId(11), ChatId(11), 7, "Nice!");
    let v: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["chat_id"], 11);
    assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
    assert_eq!(v["reply_to"]["story_poster_chat_id"], 11);
    assert_eq!(v["reply_to"]["story_id"], 7);
    assert_eq!(v["input_message_content"]["text"]["text"], "Nice!");
}

#[test]
fn s4_story_requests_match_1_8_67() {
    // Phase 9.5: `getStoryInteractions story_id:int32 query:string
    // only_contacts:Bool prefer_forwards:Bool prefer_with_reaction:Bool
    // offset:string limit:int32 = StoryInteractions` (schema line
    // 13819) — note: there is no `getStoryViewers` constructor; the
    // viewers list is this function.
    let viewers = get_story_interactions(RequestId(75), 7, "", "", 50);
    let v: serde_json::Value = serde_json::from_str(&viewers).unwrap();
    assert_eq!(v["@type"], "getStoryInteractions");
    assert_eq!(v["@extra"], "75");
    assert_eq!(v["story_id"], 7);
    assert_eq!(v["query"], "");
    assert_eq!(v["only_contacts"], false);
    assert_eq!(v["prefer_forwards"], false);
    assert_eq!(v["prefer_with_reaction"], false);
    assert_eq!(v["offset"], "");
    assert_eq!(v["limit"], 50);

    // `reportStory story_poster_chat_id:int53 story_id:int32
    // option_id:bytes text:string = ReportStoryResult` (schema line
    // 13835); the initial call leaves both empty.
    let report = report_story(RequestId(76), ChatId(11), 7, "", "");
    let v: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(v["@type"], "reportStory");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 7);
    assert_eq!(v["option_id"], "");
    assert_eq!(v["text"], "");
    let follow_up = report_story(RequestId(77), ChatId(11), 7, "aGk=", "details");
    let v: serde_json::Value = serde_json::from_str(&follow_up).unwrap();
    assert_eq!(v["option_id"], "aGk=");
    assert_eq!(v["text"], "details");

    // `activateStoryStealthMode = Ok` (schema line 13839) — no
    // parameters; state arrives as `updateStoryStealthMode`.
    let stealth = activate_story_stealth_mode(RequestId(78));
    let v: serde_json::Value = serde_json::from_str(&stealth).unwrap();
    assert_eq!(v["@type"], "activateStoryStealthMode");
    assert_eq!(v["@extra"], "78");
}

#[test]
fn story_requests_match_1_8_67() {
    // `loadActiveStories story_list:StoryList = Ok` (schema line
    // 13762); `getChatActiveStories chat_id:int53 = ChatActiveStories`
    // (line 13768); `getStory story_poster_chat_id:int53 story_id:int32
    // only_local:Bool = Story` (line 13695); `openStory` / `closeStory`
    // `story_poster_chat_id:int53 story_id:int32 = Ok` (lines 13794,
    // 13799).
    let v: serde_json::Value = serde_json::from_str(&load_active_stories(RequestId(1))).unwrap();
    assert_eq!(v["@type"], "loadActiveStories");
    assert_eq!(v["story_list"]["@type"], "storyListMain");

    let v: serde_json::Value =
        serde_json::from_str(&get_chat_active_stories(RequestId(2), ChatId(11))).unwrap();
    assert_eq!(v["@type"], "getChatActiveStories");
    assert_eq!(v["chat_id"], 11);

    let v: serde_json::Value =
        serde_json::from_str(&get_story(RequestId(3), ChatId(11), 5)).unwrap();
    assert_eq!(v["@type"], "getStory");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 5);
    assert_eq!(v["only_local"], false);

    let v: serde_json::Value =
        serde_json::from_str(&open_story(RequestId(4), ChatId(11), 5)).unwrap();
    assert_eq!(v["@type"], "openStory");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 5);

    let v: serde_json::Value =
        serde_json::from_str(&close_story(RequestId(5), ChatId(11), 5)).unwrap();
    assert_eq!(v["@type"], "closeStory");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 5);
}

#[test]
fn s1_post_story_shapes_match_1_8_67() {
    // Phase 9.3: `canPostStory chat_id:int53 = CanPostStoryResult`
    // (schema 1.8.67, line 13702); `postStory chat_id:int53
    // content:InputStoryContent areas:inputStoryAreas
    // caption:formattedText privacy_settings:StoryPrivacySettings
    // album_ids:vector<int32> active_period:int32
    // from_story_full_id:storyFullId is_posted_to_chat_page:Bool
    // protect_content:Bool = Story` (line 13715);
    // `inputStoryContentPhoto photo:InputFile
    // added_sticker_file_ids:vector<int32>` (line 6673);
    // `inputStoryContentVideo video:InputFile
    // added_sticker_file_ids:vector<int32> duration:double
    // cover_frame_timestamp:double is_animation:Bool` (line 6681);
    // `inputStoryAreas areas:vector<inputStoryArea>` (line 6619);
    // `inputFileLocal path:string` (line 325).
    let json = can_post_story(RequestId(70), ChatId(777));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "canPostStory");
    assert_eq!(v["@extra"], "70");
    assert_eq!(v["chat_id"], 777);

    let privacy = StoryPrivacy::Contacts.settings_json(&[]);
    let areas = json!({ "@type": "inputStoryAreas", "areas": [] });
    let json = post_story(
        RequestId(71),
        ChatId(777),
        StoryMediaKind::Photo,
        "/tmp/pic.jpg",
        "hello **bold**",
        privacy,
        areas.clone(),
        86400,
        None,
        false,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "postStory");
    assert_eq!(v["chat_id"], 777);
    assert_eq!(v["content"]["@type"], "inputStoryContentPhoto");
    assert_eq!(v["content"]["photo"]["@type"], "inputFileLocal");
    assert_eq!(v["content"]["photo"]["path"], "/tmp/pic.jpg");
    assert_eq!(
        v["content"]["added_sticker_file_ids"],
        serde_json::json!([])
    );
    assert_eq!(v["areas"]["@type"], "inputStoryAreas");
    assert_eq!(v["areas"]["areas"], serde_json::json!([]));
    assert_eq!(v["caption"]["text"], "hello bold");
    assert!(!v["caption"]["entities"].as_array().unwrap().is_empty());
    assert_eq!(
        v["privacy_settings"]["@type"],
        "storyPrivacySettingsContacts"
    );
    assert_eq!(v["album_ids"], serde_json::json!([]));
    assert_eq!(v["active_period"], 86400);
    assert!(v["from_story_full_id"].is_null());
    assert_eq!(v["is_posted_to_chat_page"], false);
    assert_eq!(v["protect_content"], false);

    let json = post_story(
        RequestId(72),
        ChatId(777),
        StoryMediaKind::Video,
        "/tmp/clip.mp4",
        "",
        StoryPrivacy::CloseFriends.settings_json(&[]),
        json!({ "@type": "inputStoryAreas", "areas": [] }),
        86400,
        None,
        false,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["content"]["@type"], "inputStoryContentVideo");
    assert_eq!(v["content"]["video"]["path"], "/tmp/clip.mp4");
    assert_eq!(v["content"]["duration"], 0.0);
    assert_eq!(v["content"]["is_animation"], false);
    assert_eq!(
        v["privacy_settings"]["@type"],
        "storyPrivacySettingsCloseFriends"
    );
}

#[test]
fn s2_post_story_options_match_1_8_67() {
    // Phase 9.4: `postStory` (schema 1.8.67, line 13715) with the
    // previously fixed fields wired — a link + reaction areas block
    // (`td_api.tl:6619`), a 48h active period (td_api.tl:13715
    // comment: legal), and both toggles on.
    let areas = json!({
        "@type": "inputStoryAreas",
        "areas": [{
            "@type": "inputStoryArea",
            "position": {
                "@type": "storyAreaPosition",
                "x_percentage": 35.0, "y_percentage": 80.0,
                "width_percentage": 30.0, "height_percentage": 9.0,
                "rotation_angle": 0.0, "corner_radius_percentage": 20.0
            },
            "type": { "@type": "inputStoryAreaTypeLink", "url": "https://t.me/quill" }
        }]
    });
    let json = post_story(
        RequestId(73),
        ChatId(777),
        StoryMediaKind::Photo,
        "/tmp/pic.jpg",
        "",
        StoryPrivacy::Everyone.settings_json(&[]),
        areas,
        172800,
        None,
        true,
        true,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["areas"]["areas"][0]["type"]["@type"],
        "inputStoryAreaTypeLink"
    );
    assert_eq!(v["areas"]["areas"][0]["type"]["url"], "https://t.me/quill");
    assert_eq!(v["active_period"], 172800);
    assert_eq!(v["is_posted_to_chat_page"], true);
    assert_eq!(v["protect_content"], true);
}

#[test]
fn s3_manage_story_request_shapes_match_1_8_67() {
    // Phase 9.5: `editStory` (td_api.tl:13732), `editStoryCover`
    // (td_api.tl:13738), `setStoryPrivacySettings` (td_api.tl:13743),
    // `getChatsToPostStories` (td_api.tl:13698), and `postStory` with
    // a repost source (`storyFullId`, td_api.tl:6766).
    let json = edit_story(
        RequestId(80),
        ChatId(11),
        5,
        None,
        None,
        Some("new caption"),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editStory");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 5);
    assert!(v["content"].is_null());
    assert!(v["areas"].is_null());
    assert_eq!(v["caption"]["text"], "new caption");

    // New media + areas ride as input blocks (null = keep).
    let json = edit_story(
        RequestId(81),
        ChatId(11),
        5,
        Some(input_story_content(StoryMediaKind::Photo, "/tmp/new.jpg")),
        Some(json!({ "@type": "inputStoryAreas", "areas": [] })),
        None,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["content"]["@type"], "inputStoryContentPhoto");
    assert_eq!(v["areas"]["@type"], "inputStoryAreas");
    assert!(v["caption"].is_null());

    let json = edit_story_cover(RequestId(82), ChatId(11), 5, 3.5);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editStoryCover");
    assert_eq!(v["story_poster_chat_id"], 11);
    assert_eq!(v["story_id"], 5);
    assert_eq!(v["cover_frame_timestamp"], 3.5);

    let json = set_story_privacy_settings(
        RequestId(83),
        5,
        StoryPrivacy::CloseFriends.settings_json(&[]),
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setStoryPrivacySettings");
    assert_eq!(v["story_id"], 5);
    assert_eq!(
        v["privacy_settings"]["@type"],
        "storyPrivacySettingsCloseFriends"
    );

    let json = get_chats_to_post_stories(RequestId(84));
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "getChatsToPostStories");

    // Repost: `from_story_full_id` carries the source story.
    let json = post_story(
        RequestId(85),
        ChatId(777),
        StoryMediaKind::Photo,
        "/tmp/pic.jpg",
        "",
        StoryPrivacy::Everyone.settings_json(&[]),
        json!({ "@type": "inputStoryAreas", "areas": [] }),
        86400,
        Some((22, 3)),
        false,
        false,
    );
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "postStory");
    assert_eq!(v["from_story_full_id"]["@type"], "storyFullId");
    assert_eq!(v["from_story_full_id"]["poster_chat_id"], 22);
    assert_eq!(v["from_story_full_id"]["story_id"], 3);
}

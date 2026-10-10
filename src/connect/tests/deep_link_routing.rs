//! Launch / OS-delivered links are routed by shape (tdesktop `openLocalUrl`),
//! with TDLib answers recorded as JSON.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::{DeepLinkAction, DeepLinkState};
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn feed(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    value: serde_json::Value,
) {
    driver
        .ingest(copy_and_parse(&value.to_string(), seq, sink).unwrap())
        .unwrap();
}

#[test]
fn launch_links_resolve_locally_instead_of_get_deep_link_info() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    // tg://resolve and t.me/<user> go straight to searchPublicChat; TDLib's
    // getDeepLinkInfo answers 404 for them.
    for link in ["tg://resolve?domain=durov", "https://t.me/durov"] {
        driver.session.deep_link = None;
        let extra = driver.request_deep_link_info(link).unwrap().unwrap();
        assert_eq!(
            sent_request(&recorder, "searchPublicChat")["username"],
            "durov"
        );
        assert!(
            !recorder
                .snapshot()
                .iter()
                .any(|j| j.contains("getDeepLinkInfo")),
            "{link}"
        );
        // Recorded shape of TDLib's searchPublicChat answer: a `chat`.
        feed(
            &mut driver,
            &seq,
            &sink,
            json!({"@type": "chat", "@extra": extra.as_extra(), "id": 42, "title": "Durov",
                "type": {"@type": "chatTypePrivate", "user_id": 42}, "unread_count": 0}),
        );
        assert!(matches!(
            driver.session.deep_link,
            Some(DeepLinkState::ChatReady { chat_id, action: DeepLinkAction::OpenUsername { ref domain, .. } })
                if chat_id.0 == 42 && domain == "durov"
        ));
    }

    // Invites (web `+hash`) go through the checked preview, never auto-join.
    driver.session.deep_link = None;
    driver
        .request_deep_link_info("https://t.me/+AbCd")
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "checkChatInviteLink")["invite_link"],
        "https://t.me/+AbCd"
    );

    // A link with no local form still asks TDLib.
    driver.session.deep_link = None;
    driver
        .request_deep_link_info("tg://proxy?server=x&port=1")
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "getInternalLinkType")["link"],
        "tg://proxy?server=x&port=1"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_links_use_tdesktop_wording() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let mut fail = |link: &str, code: i32, msg: &str| {
        driver.session.deep_link = None;
        let extra = driver.request_deep_link_info(link).unwrap().unwrap();
        feed(
            &mut driver,
            &seq,
            &sink,
            json!({"@type": "error", "@extra": extra.as_extra(), "code": code, "message": msg}),
        );
        match driver.session.deep_link.clone() {
            Some(DeepLinkState::ShowText(text)) => text,
            other => panic!("unexpected {other:?}"),
        }
    };
    assert_eq!(
        fail("https://t.me/nobody_here", 400, "USERNAME_NOT_OCCUPIED"),
        "The username \"nobody_here\" is not occupied by anyone."
    );
    assert_eq!(
        fail("tg://join?invite=Dead", 400, "INVITE_HASH_EXPIRED"),
        "This invite link is broken or has expired."
    );
    assert_eq!(
        fail("tg://proxy?server=x&port=1", 404, "Not Found"),
        "This link isn't supported by Quill."
    );
    assert_eq!(
        fail("tg://resolve?domain=durov", 500, "Internal"),
        "Couldn't open the link (error 500)."
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Open `link`, answer `getInternalLinkType` with `answer`, return the state.
fn open_with(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    link: &str,
    answer: serde_json::Value,
) -> Option<DeepLinkState> {
    driver.session.deep_link = None;
    let extra = driver.request_deep_link_info(link).unwrap().unwrap();
    let mut answer = answer;
    answer["@extra"] = json!(extra.as_extra());
    feed(driver, seq, sink, answer);
    driver.session.deep_link.clone()
}

fn info_action(state: Option<DeepLinkState>) -> DeepLinkAction {
    match state {
        Some(DeepLinkState::Info {
            action: Some(action),
            ..
        }) => action,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn typed_links_route_through_get_internal_link_type() {
    use crate::deep_link_types::{DeepLinkUi, SettingsTarget};
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    // Sticker set: searchStickerSet, then the preview dialog's set id.
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "https://t.me/addstickers/Cats",
        json!({"@type":"internalLinkTypeStickerSet","sticker_set_name":"Cats","expect_custom_emoji":false}),
    );
    let action = info_action(state);
    assert_eq!(
        action,
        DeepLinkAction::StickerSet {
            name: "Cats".into()
        }
    );
    let extra = driver.resolve_deep_link(action).unwrap().unwrap();
    assert_eq!(sent_request(&recorder, "searchStickerSet")["name"], "Cats");
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"stickerSet","@extra":extra.as_extra(),"id":"777","title":"Cats","name":"Cats",
            "is_installed":false,"stickers":[]}),
    );
    assert_eq!(
        driver.session.deep_link,
        Some(DeepLinkState::Ui(DeepLinkUi::StickerSet { set_id: 777 }))
    );

    // Comment / thread / timestamp: message link info, then the chat.
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "https://t.me/news/5?comment=7&t=30",
        json!({"@type":"internalLinkTypeMessage","url":"https://t.me/news/5?comment=7&t=30"}),
    );
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("searchPublicChat")),
        "parameters TDLib must parse are not routed locally"
    );
    let action = info_action(state);
    let extra = driver.resolve_deep_link(action).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getMessageLinkInfo")["url"],
        "https://t.me/news/5?comment=7&t=30"
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"messageLinkInfo","@extra":extra.as_extra(),"is_public":true,"chat_id":-1001,
            "message":{"@type":"message","id":3145728},"media_timestamp":30,
            "topic_id":{"@type":"messageTopicThread","message_thread_id":2097152}}),
    );
    let action = info_action(driver.session.deep_link.clone());
    assert_eq!(
        action,
        DeepLinkAction::OpenChatById {
            chat_id: -1001,
            message_id: 3145728,
            media_timestamp: Some(30),
            thread_id: Some(2097152),
        }
    );
    let extra = driver.resolve_deep_link(action.clone()).unwrap().unwrap();
    assert_eq!(sent_request(&recorder, "getChat")["chat_id"], -1001);
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"chat","@extra":extra.as_extra(),"id":-1001,"title":"News",
            "type":{"@type":"chatTypeSupergroup","supergroup_id":1001,"is_channel":false},"unread_count":0}),
    );
    assert!(matches!(
        driver.session.deep_link,
        Some(DeepLinkState::ChatReady { chat_id, .. }) if chat_id.0 == -1001
    ));

    // A message in a chat the account cannot see is explained, not opened.
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "https://t.me/c/1/2?thread=3",
        json!({"@type":"internalLinkTypeMessage","url":"https://t.me/c/1/2?thread=3"}),
    );
    let extra = driver
        .resolve_deep_link(info_action(state))
        .unwrap()
        .unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"messageLinkInfo","@extra":extra.as_extra(),"chat_id":0}),
    );
    assert!(matches!(
        driver.session.deep_link,
        Some(DeepLinkState::ShowText(_))
    ));

    // Boost link: getChatBoostLinkInfo names the channel, then getChat.
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "tg://boost?channel=1001",
        json!({"@type":"internalLinkTypeChatBoost","url":"tg://boost?channel=1001"}),
    );
    let action = info_action(state);
    assert_eq!(
        action,
        DeepLinkAction::BoostLink {
            url: "tg://boost?channel=1001".into()
        }
    );
    let extra = driver.resolve_deep_link(action).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getChatBoostLinkInfo")["url"],
        "tg://boost?channel=1001"
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"chatBoostLinkInfo","@extra":extra.as_extra(),"is_public":false,"chat_id":-1001}),
    );
    assert_eq!(
        info_action(driver.session.deep_link.clone()),
        DeepLinkAction::OpenChannelBoost { chat_id: -1001 }
    );
    // A link that names no channel is explained, not opened.
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "tg://boost?channel=9",
        json!({"@type":"internalLinkTypeChatBoost","url":"tg://boost?channel=9"}),
    );
    let extra = driver
        .resolve_deep_link(info_action(state))
        .unwrap()
        .unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"chatBoostLinkInfo","@extra":extra.as_extra(),"is_public":false,"chat_id":0}),
    );
    assert!(matches!(
        driver.session.deep_link,
        Some(DeepLinkState::ShowText(_))
    ));

    // +phone: searchUserByPhoneNumber, then createPrivateChat with a draft.
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "https://t.me/+15550001?text=hi",
        json!({"@type":"internalLinkTypeUserPhoneNumber","phone_number":"15550001","draft_text":"hi","open_profile":false}),
    );
    let action = info_action(state);
    let extra = driver.resolve_deep_link(action).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "searchUserByPhoneNumber")["phone_number"],
        "15550001"
    );
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"user","@extra":extra.as_extra(),"id":4242,"first_name":"Ann"}),
    );
    assert_eq!(
        info_action(driver.session.deep_link.clone()),
        DeepLinkAction::OpenUserDraft {
            user_id: 4242,
            draft: "hi".into()
        }
    );

    // Proxy hands the original link to the proxy UI; no request follows.
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "tg://proxy?server=x&port=1&secret=ab",
        json!({"@type":"internalLinkTypeProxy","proxy":{}}),
    );
    assert_eq!(
        state,
        Some(DeepLinkState::Ui(DeepLinkUi::Proxy {
            link: "tg://proxy?server=x&port=1&secret=ab".into()
        }))
    );

    // Share draft and settings need no request either.
    assert_eq!(
        open_with(
            &mut driver,
            &seq,
            &sink,
            "https://t.me/share/url?url=https://a.b&text=look",
            json!({"@type":"internalLinkTypeMessageDraft","text":{"@type":"formattedText","text":"https://a.b\nlook","entities":[]},"contains_link":true}),
        ),
        Some(DeepLinkState::Ui(DeepLinkUi::Share {
            text: "https://a.b\nlook".into()
        }))
    );
    assert_eq!(
        open_with(
            &mut driver,
            &seq,
            &sink,
            "tg://bg?slug=sky",
            json!({"@type":"internalLinkTypeBackground","background_name":"sky"}),
        ),
        Some(DeepLinkState::Ui(DeepLinkUi::Background {
            name: "sky".into()
        }))
    );
    assert_eq!(
        open_with(
            &mut driver,
            &seq,
            &sink,
            "tg://addlist?slug=x",
            json!({"@type":"internalLinkTypeChatFolderInvite","invite_link":"https://t.me/addlist/x"}),
        ),
        Some(DeepLinkState::Ui(DeepLinkUi::FolderInvite {
            link: "https://t.me/addlist/x".into()
        }))
    );
    assert_eq!(
        open_with(
            &mut driver,
            &seq,
            &sink,
            "tg://settings/devices",
            json!({"@type":"internalLinkTypeSettings","section":{"@type":"settingsSectionDevices"}}),
        ),
        Some(DeepLinkState::Ui(DeepLinkUi::Settings(
            SettingsTarget::Devices
        )))
    );

    // Unsupported targets say so; unknown ones ask getDeepLinkInfo.
    for (link, answer) in [
        (
            "tg://invoice?slug=x",
            json!({"@type":"internalLinkTypeInvoice","invoice_name":"x"}),
        ),
        (
            "tg://settings/themes",
            json!({"@type":"internalLinkTypeTheme","theme_name":"x"}),
        ),
    ] {
        assert!(
            matches!(
                open_with(&mut driver, &seq, &sink, link, answer),
                Some(DeepLinkState::ShowText(_))
            ),
            "{link}"
        );
    }
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "tg://nope",
        json!({"@type":"internalLinkTypeUnknownDeepLink","link":"tg://nope"}),
    );
    assert_eq!(
        state,
        Some(DeepLinkState::Unknown {
            link: "tg://nope".into()
        })
    );
    driver.request_deep_link_text("tg://nope").unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "getDeepLinkInfo")["link"],
        "tg://nope"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn typed_link_failures_use_clear_wording() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let state = open_with(
        &mut driver,
        &seq,
        &sink,
        "https://t.me/+1555000",
        json!({"@type":"internalLinkTypeUserPhoneNumber","phone_number":"1555000","draft_text":"","open_profile":false}),
    );
    let extra = driver
        .resolve_deep_link(info_action(state))
        .unwrap()
        .unwrap();
    feed(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":"PHONE_NOT_OCCUPIED"}),
    );
    assert_eq!(
        driver.session.deep_link,
        Some(DeepLinkState::ShowText(
            "The phone number +1555000 is not on Telegram yet.".into()
        ))
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bot_links_resolve_the_bot_with_search_public_chat() {
    use crate::bot_invite::{Invite, Scope};
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    let game = DeepLinkAction::ShareGame {
        domain: "chessbot".into(),
        game_short_name: "chess".into(),
    };
    driver.resolve_deep_link(game.clone()).unwrap().unwrap();
    assert_eq!(
        sent_request(&recorder, "searchPublicChat")["username"],
        "chessbot"
    );
    assert!(matches!(
        driver.session.deep_link,
        Some(DeepLinkState::ResolvingChat { ref action, .. }) if *action == game
    ));

    driver
        .resolve_deep_link(DeepLinkAction::AddBot {
            domain: "helper".into(),
            invite: Invite {
                scope: Scope::GroupAdmin,
                ..Default::default()
            },
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        sent_request(&recorder, "searchPublicChat")["username"],
        "helper"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

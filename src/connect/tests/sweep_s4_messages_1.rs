//! Sweep fixes (s4-messages-1): a refused send or an answer that is not the
//! asked one leaves the stage of the thing that asked (recorded JSON only).
use super::sweep_s3_payments_1::Flaky;
use crate::emoji::EmojiSetTab;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{RequestPurpose, StickersPurpose};
use crate::telegram::envelope::{CallbackQueryAnswer, MessageActions};
use serde_json::{Value, json};

fn private_chat(t: &mut Flaky, id: i64) {
    t.feed(
        json!({"@type":"updateNewChat","chat":{"id":id,"title":"Ann",
        "type":{"@type":"chatTypePrivate","user_id":id},"unread_count":0,
        "permissions":{"@type":"chatPermissions","can_send_basic_messages":true}}}),
    );
}

fn own_message(chat: i64, id: i64, effect: i64) -> Value {
    json!({"@type":"updateNewMessage","message":{"id":id,"chat_id":chat,
        "is_outgoing":true,"date":1,"effect_id":effect.to_string(),
        "content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}})
}

#[test]
fn a_closechat_that_never_left_does_not_stay_pending() {
    let mut t = Flaky::new();
    private_chat(&mut t, 21);
    t.driver.select_chat(ChatId(21)).unwrap();
    t.sender.refuse(&["closeChat"]);
    let pending = t.driver.session.requests.pending.len();
    assert!(t.driver.close_open_chat().is_err());
    assert_eq!(t.driver.session.requests.pending.len(), pending);
}

#[test]
fn an_emoji_list_failure_stays_on_its_tab_until_that_list_is_asked_again() {
    let mut t = Flaky::new();
    t.driver.open_emoji_sets().unwrap();
    // Trending is opened before the installed list answers.
    t.driver
        .select_emoji_set_tab(EmojiSetTab::Trending)
        .unwrap();
    t.fail("getInstalledStickerSets", "boom");
    let lists = &t.driver.session.stickers.emoji.lists;
    assert!(lists.installed.error().is_some());
    assert!(lists.trending.is_loading());
    // The trending answer (or any other request) does not clear it.
    t.answer(
        "getTrendingStickerSets",
        json!({"@type":"trendingStickerSets",
        "total_count":0,"sets":[],"is_premium":false}),
    );
    let lists = &t.driver.session.stickers.emoji.lists;
    assert!(lists.installed.error().is_some(), "{:?}", lists.installed);
    assert!(lists.trending.error().is_none());
    // Back on Installed it asks again, which ends the failure.
    t.driver
        .select_emoji_set_tab(EmojiSetTab::Installed)
        .unwrap();
    assert!(t.driver.session.stickers.emoji.lists.installed.is_loading());
}

#[test]
fn a_refused_view_of_trending_sets_keeps_their_unread_dots() {
    let mut t = Flaky::new();
    t.driver
        .session
        .stickers
        .stickers
        .trending
        .push(crate::telegram::envelope::StickerSetInfo {
            id: 5,
            title: "Set".into(),
            name: "set".into(),
            size: 1,
            is_installed: false,
            is_official: false,
            is_owned: false,
            covers: Vec::new(),
            cover_files: Vec::new(),
            is_viewed: false,
        });
    t.sender.refuse(&["viewTrendingStickerSets"]);
    assert!(t.driver.view_shown_trending_stickers().is_err());
    assert!(!t.driver.session.stickers.stickers.trending[0].is_viewed);
    t.sender.refuse(&[]);
    assert!(t.driver.view_shown_trending_stickers().unwrap().is_some());
    assert!(t.driver.session.stickers.stickers.trending[0].is_viewed);
}

#[test]
fn selection_delete_for_everyone_waits_for_every_message_to_allow_it() {
    let mut t = Flaky::new();
    private_chat(&mut t, 21);
    t.feed(own_message(21, 100, 0));
    t.feed(own_message(21, 101, 0));
    let chat = ChatId(21);
    let ids = [MessageId(100), MessageId(101)];
    t.driver.delete_selected(chat, &ids, true).unwrap();
    assert_eq!(t.sender.last("deleteMessages").unwrap()["revoke"], true);
    // TDLib says the older one is past its window.
    let actions = |all| MessageActions {
        can_be_deleted_only_for_self: true,
        can_be_deleted_for_all_users: all,
        ..MessageActions::default()
    };
    let menu = &mut t.driver.session.messages.menu_actions;
    menu.set(chat, ids[0], actions(false));
    menu.set(chat, ids[1], actions(true));
    t.driver.delete_selected(chat, &ids, true).unwrap();
    assert_eq!(t.sender.last("deleteMessages").unwrap()["revoke"], false);
}

#[test]
fn suggestions_waiting_for_the_sets_stop_waiting_when_the_sets_fail() {
    use crate::sticker_suggest::StickerSuggestMode;
    let mut t = Flaky::new();
    t.driver.session.settings.media_prefs.sticker_suggest_mode = StickerSuggestMode::InstalledOnly;
    t.driver.update_sticker_suggestions("😀").unwrap();
    assert!(t.driver.session.stickers.stickers.suggest_waiting_for_sets);
    t.fail("getInstalledStickerSets", "boom");
    let stickers = &t.driver.session.stickers.stickers;
    assert!(!stickers.suggest_waiting_for_sets);
    assert!(stickers.suggest_for.is_none());
    assert!(t.driver.composer_suggestions_idle("plain text"));
}

#[test]
fn two_callback_answers_between_polls_both_reach_the_poll() {
    let mut t = Flaky::new();
    let answer = |text: &str| CallbackQueryAnswer {
        text: text.into(),
        show_alert: false,
        url: String::new(),
    };
    let session = &mut t.driver.session;
    session.note_callback_answer(Some(ChatId(1)), answer("first"));
    session.note_callback_answer(Some(ChatId(2)), answer("second"));
    let texts: Vec<_> = session
        .bots
        .callback_answers
        .iter()
        .map(|pressed| (pressed.chat, pressed.answer.text.as_str()))
        .collect();
    assert_eq!(
        texts,
        [(Some(ChatId(1)), "first"), (Some(ChatId(2)), "second")]
    );
}

#[test]
fn a_sticker_set_edit_answer_ends_only_its_own_edit_and_every_toast_shows() {
    let mut t = Flaky::new();
    let session = &mut t.driver.session;
    session.stickers.sticker_set_view = Some(crate::state::StickerSetView::loading(77));
    let rename = |session: &mut crate::state::Session| {
        let id = session.request(
            RequestPurpose::Stickers(StickersPurpose::Creator(
                crate::state::StickerCreatorPurpose::AddExistingSticker {
                    set_id: 77,
                    emoji: false,
                },
            )),
            None,
        );
        session.begin_sticker_set_edit(77, id);
        id
    };
    let first = rename(session);
    let second = rename(session);
    t.feed(json!({"@type":"ok","@extra":first.as_extra()}));
    let view = t.driver.session.stickers.sticker_set_view.as_ref().unwrap();
    assert_eq!(view.edits.in_flight, [second]);
    // An answer of an edit made under an earlier box ends none of these.
    t.feed(json!({"@type":"ok","@extra":RequestId(9_999).as_extra()}));
    let view = t.driver.session.stickers.sticker_set_view.as_ref().unwrap();
    assert!(view.edits.busy());
    t.feed(json!({"@type":"ok","@extra":second.as_extra()}));
    let session = &mut t.driver.session;
    assert!(
        !session
            .stickers
            .sticker_set_view
            .as_ref()
            .unwrap()
            .edits
            .busy()
    );
    // Both answers' toasts are kept, oldest first.
    assert!(session.take_sticker_set_edit_note().is_some());
    assert!(session.take_sticker_set_edit_note().is_some());
    assert!(session.take_sticker_set_edit_note().is_none());
}

#[test]
fn a_mask_list_asked_before_a_change_does_not_land_after_it() {
    let mut t = Flaky::new();
    let session = &mut t.driver.session;
    let list = session.request(StickersPurpose::GetInstalledMaskSets.into(), None);
    let change = RequestPurpose::Stickers(StickersPurpose::ChangeMaskSet {
        set_id: 3,
        installed: false,
        archived: false,
    });
    session.finish_mask_mutation(Some(change));
    assert!(
        !session
            .requests
            .has_purpose(StickersPurpose::GetInstalledMaskSets.into())
    );
    t.feed(
        json!({"@type":"stickerSets","@extra":list.as_extra(),"sets":[
        {"@type":"stickerSetInfo","id":"3","size":1,"is_installed":true}]}),
    );
    let masks = &t.driver.session.stickers.masks;
    assert!(!masks.installed_loaded);
    assert!(masks.installed.is_empty());
}

#[test]
fn effects_are_looked_up_for_the_messages_of_every_shown_chat() {
    let mut t = Flaky::new();
    private_chat(&mut t, 21);
    private_chat(&mut t, 22);
    t.driver.select_chat(ChatId(21)).unwrap();
    t.driver.open_chat_window(ChatId(22)).unwrap();
    t.feed(own_message(21, 100, 11));
    t.feed(own_message(22, 200, 22));
    let mut ids = t.driver.session.message_effect_ids_in_view();
    ids.sort_unstable();
    assert_eq!(ids, [11, 22]);
}

#[test]
fn an_attachment_menu_answer_names_its_bot() {
    let mut t = Flaky::new();
    t.driver
        .toggle_bot_in_attachment_menu(43, true, false)
        .unwrap();
    t.driver
        .toggle_bot_in_attachment_menu(44, false, false)
        .unwrap();
    let asked = t.sender.all("toggleBotIsAddedToAttachmentMenu");
    assert_eq!(asked.len(), 2);
    // The second bot's answer lands first, and fails.
    t.feed(json!({"@type":"error","@extra":asked[1]["@extra"],"code":400,"message":"no"}));
    t.feed(json!({"@type":"ok","@extra":asked[0]["@extra"]}));
    let toggles: Vec<_> = t
        .driver
        .session
        .bots
        .web_apps
        .attachment_menu_toggled
        .iter()
        .map(|toggle| (toggle.bot_user_id, toggle.result.is_ok()))
        .collect();
    assert_eq!(toggles, [(44, false), (43, true)]);
}

#[test]
fn typing_moves_with_the_topic_and_the_cancel_goes_where_the_action_went() {
    let mut t = Flaky::new();
    t.feed(json!({"@type":"updateNewChat","chat":{"id":-100_7,"title":"Forum",
        "type":{"@type":"chatTypeSupergroup","supergroup_id":107,"is_channel":false},
        "unread_count":0,"permissions":{"@type":"chatPermissions","can_send_basic_messages":true}}}));
    t.driver.select_chat(ChatId(-100_7)).unwrap();
    t.driver.session.open_topic = Some(5);
    t.driver.sync_outgoing_typing("hi", false, 1_000).unwrap();
    // Another topic of the same chat, inside the 4 s interval.
    t.driver.session.open_topic = Some(6);
    t.driver.sync_outgoing_typing("hi", false, 1_500).unwrap();
    let actions: Vec<(String, i64)> = t
        .sender
        .all("sendChatAction")
        .iter()
        .map(|action| {
            (
                action["action"]["@type"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                action["topic_id"]["forum_topic_id"].as_i64().unwrap_or(0),
            )
        })
        .collect();
    assert_eq!(
        actions,
        [
            ("chatActionTyping".into(), 5),
            ("chatActionCancel".into(), 5),
            ("chatActionTyping".into(), 6),
        ]
    );
}

#[test]
fn a_stop_pressed_again_after_a_failure_leaves_the_failed_stage() {
    use crate::state::BotStopStage;
    let mut t = Flaky::new();
    t.feed(
        json!({"@type":"updatePendingMessage","chat_id":7,"forum_topic_id":0,
        "draft_id":"12","can_stop":true,"keep_on_stop":true,
        "content":{"@type":"messageText","text":{"text":"…","entities":[]}}}),
    );
    t.driver.stop_pending_bot_message(ChatId(7), 0, 12).unwrap();
    t.fail("stopPendingMessage", "boom");
    let stage = |t: &Flaky| t.driver.session.messages.pending_bot_messages[&(7, 0)].stop;
    assert_eq!(stage(&t), BotStopStage::Failed);
    t.driver.stop_pending_bot_message(ChatId(7), 0, 12).unwrap();
    assert_eq!(stage(&t), BotStopStage::Running);
}

#[test]
fn the_reply_bar_copy_shows_an_edit_and_the_oldest_copy_is_the_one_dropped() {
    let mut t = Flaky::new();
    private_chat(&mut t, 21);
    t.driver
        .fetch_composer_reply_target(ChatId(21), MessageId(500))
        .unwrap();
    t.answer(
        "getMessage",
        json!({"@type":"message","id":500,"chat_id":21,"is_outgoing":false,"date":1,
            "content":{"@type":"messageText","text":{"@type":"formattedText","text":"old","entities":[]}}}),
    );
    let shown = |t: &Flaky| {
        t.driver
            .session
            .reply_target(ChatId(21), MessageId(500))
            .map(crate::state::effective_preview)
    };
    assert_eq!(shown(&t).as_deref(), Some("old"));
    t.feed(json!({"@type":"updateMessageContent","chat_id":21,"message_id":500,
        "new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"new","entities":[]}}}));
    assert_eq!(shown(&t).as_deref(), Some("new"));

    // Past the cap the oldest settled copy goes, whatever its id.
    let session = &mut t.driver.session;
    for id in (1..=40).rev() {
        session.begin_composer_reply_fetch(ChatId(21), MessageId(id));
        session.reject_composer_reply_target(
            ChatId(21),
            MessageId(id),
            &crate::telegram::envelope::TdError::from_code(500),
        );
    }
    let fetch = &session.messages.composer_reply_fetch;
    assert!(fetch.len() <= 32);
    assert!(fetch.contains_key(&(21, 1)), "the newest stays");
    assert!(
        !fetch.contains_key(&(21, 40)),
        "the oldest of the failed goes"
    );
    assert!(!fetch.contains_key(&(21, 500)), "so does the oldest copy");
}

#[test]
fn each_create_gets_its_own_start_number_and_a_reset_keeps_the_count() {
    let mut t = Flaky::new();
    t.feed(
        json!({"@type":"updateUser","user":{"id":900,"first_name":"Manager",
        "type":{"@type":"userTypeBot","can_be_edited":false}}}),
    );
    let starts = |t: &Flaky| t.driver.session.bots.owned.create.starts;
    let before = starts(&t);
    if t.driver
        .create_managed_bot(900, "A", "a_bot", false)
        .is_err()
    {
        // The manager bot is not set up in this fixture: nothing to count.
        return;
    }
    let first = starts(&t);
    assert_eq!(first, before + 1);
    t.driver
        .create_managed_bot(900, "B", "b_bot", false)
        .unwrap();
    assert_eq!(starts(&t), first + 1);
    t.driver.reset_create_bot();
    assert_eq!(starts(&t), first + 1);
}

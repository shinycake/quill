//! Sweep fixes (r2-03): a refused send or an answer for another asker
//! leaves the stage of what asked (recorded JSON only).
use super::sweep_s3_payments_1::Flaky;
use crate::ids::ChatId;
use crate::state::{SuggestFor, UsernameCheck};
use serde_json::{Value, json};

fn private_chat(t: &mut Flaky, id: i64) {
    t.feed(
        json!({"@type":"updateNewChat","chat":{"id":id,"title":"Ann",
        "type":{"@type":"chatTypePrivate","user_id":id},"unread_count":0,
        "permissions":{"@type":"chatPermissions","can_send_basic_messages":true}}}),
    );
}

fn message(chat: i64, id: i64) -> Value {
    json!({"@type":"message","id":id,"chat_id":chat,"is_outgoing":false,"date":id,
        "content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}})
}

#[test]
fn an_installed_sets_answer_keeps_the_emoji_that_was_already_asked() {
    let mut t = Flaky::new();
    t.driver.session.stickers.stickers.suggest_for = SuggestFor::Asked("😀".into());
    t.driver.open_media_panel().unwrap();
    t.answer(
        "getInstalledStickerSets",
        json!({"@type":"stickerSets","total_count":0,"sets":[]}),
    );
    assert_eq!(
        t.driver.session.stickers.stickers.suggest_for,
        SuggestFor::Asked("😀".into())
    );
}

#[test]
fn a_send_in_one_chat_leaves_another_chats_waiting_draft_save() {
    use crate::composer::DraftTopic;
    let mut t = Flaky::new();
    private_chat(&mut t, 21);
    private_chat(&mut t, 22);
    t.driver
        .note_composer_draft_in(
            ChatId(22),
            DraftTopic::Chat,
            "typed in a window",
            None,
            1_000,
            true,
        )
        .unwrap();
    assert!(t.driver.pending_draft.armed().is_some());
    t.driver
        .clear_draft_after_agent_send(ChatId(21), DraftTopic::Chat)
        .unwrap();
    let held = t.driver.pending_draft.armed();
    assert_eq!(held.map(|h| h.draft().chat_id), Some(ChatId(22)));
    assert!(
        t.driver
            .session
            .draft_is_dirty(ChatId(22), DraftTopic::Chat)
    );
}

#[test]
fn a_refused_collections_send_keeps_the_stale_mark() {
    use crate::gift_manage::CollectionsFetch;
    use crate::telegram::envelope::MessageSender;
    let mut t = Flaky::new();
    let hub = &mut t.driver.session.payments.hub;
    hub.gifts_owner = Some(MessageSender::User { user_id: 7 });
    t.driver.session.payments.gifts_manage.collections_fetch = CollectionsFetch::Stale;
    t.sender.refuse(&["getGiftCollections"]);
    assert!(t.driver.refresh_gift_collections_if_stale().is_err());
    assert_eq!(
        t.driver.session.payments.gifts_manage.collections_fetch,
        CollectionsFetch::Stale
    );
    t.sender.refuse(&[]);
    t.driver.refresh_gift_collections_if_stale().unwrap();
    assert_eq!(t.sender.count("getGiftCollections"), 1);
    assert_eq!(
        t.driver.session.payments.gifts_manage.collections_fetch,
        CollectionsFetch::Loading
    );
}

#[test]
fn closing_the_gifts_dialog_leaves_nothing_to_refetch() {
    use crate::telegram::envelope::MessageSender;
    let mut t = Flaky::new();
    t.driver
        .open_received_gifts(MessageSender::User { user_id: 7 })
        .unwrap();
    t.answer(
        "getReceivedGifts",
        json!({"@type":"receivedGifts","total_count":0,"gifts":[],"next_offset":""}),
    );
    t.driver.session.payments.hub.gifts_op.mark_stale();
    t.driver.session.payments.hub.close_gifts();
    let asked = t.sender.count("getReceivedGifts");
    assert_eq!(t.driver.refresh_received_gifts_if_stale().unwrap(), None);
    assert_eq!(t.sender.count("getReceivedGifts"), asked);
    assert!(t.driver.session.payments.hub.gifts_owner.is_none());
}

#[test]
fn a_refused_link_check_leaves_the_check_in_flight_answerable() {
    let mut t = Flaky::new();
    t.feed(json!({"@type":"updateNewChat","chat":{"id":13,"title":"C","type":{"@type":"chatTypeSupergroup","supergroup_id":113,"is_channel":false},"unread_count":0}}));
    t.feed(
        json!({"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":113,
        "status":{"@type":"chatMemberStatusCreator","is_member":true}}}),
    );
    t.driver
        .check_group_username(ChatId(13), "quillchat")
        .unwrap();
    t.sender.refuse(&["checkChatUsername"]);
    assert!(
        t.driver
            .check_group_username(ChatId(13), "quillchats")
            .is_err()
    );
    t.sender.refuse(&[]);
    t.answer(
        "checkChatUsername",
        json!({"@type":"checkChatUsernameResultUsernameOccupied"}),
    );
    assert!(matches!(
        t.driver.session.groups.group_type.check,
        UsernameCheck::Verdict(_)
    ));
}

#[test]
fn a_failed_older_page_is_not_asked_again_until_retry_or_scroll_away() {
    let mut t = Flaky::new();
    private_chat(&mut t, 7);
    t.driver.select_chat(ChatId(7)).unwrap();
    t.answer(
        "getChatHistory",
        json!({"@type":"messages","total_count":1,"messages":[message(7, 100)]}),
    );
    t.driver.fetch_history().unwrap().expect("older page");
    assert_eq!(t.sender.count("getChatHistory"), 2);
    t.fail("getChatHistory", "boom");
    let history = &t.driver.session.histories[&7];
    assert!(history.load_failed() && !history.messages.is_empty());
    // The row-0 paint asks again: nothing goes out.
    assert_eq!(t.driver.fetch_history().unwrap(), None);
    assert_eq!(t.sender.count("getChatHistory"), 2);
    // Scrolled away from the edge, then back: it asks again.
    if let Some(history) = t.driver.session.histories.get_mut(&7) {
        history.clear_load_failed();
    }
    t.driver.fetch_history().unwrap().expect("asked again");
    assert_eq!(t.sender.count("getChatHistory"), 3);
}

#[test]
fn money_history_is_asked_when_an_earlier_money_send_is_refused() {
    let mut t = Flaky::new();
    t.driver.session.ton_wallet.can_use = true;
    t.sender.refuse(&["getApplicationConfig"]);
    assert!(t.driver.open_money_page().is_err());
    assert!(t.sender.count("getTonWalletTransactions") >= 1);
    assert!(t.driver.session.ton_wallet.money.history.is_loading());
}

#[test]
fn a_refused_undo_keeps_the_pending_live_story_stars() {
    let mut h = super::live_story::watching();
    h.ingest(&super::live_story::LIVE_CALL.replace("JOINED", "true"));
    h.driver.add_live_story_reaction(3, 10_000).unwrap();
    h.recorder.refuse(Some("removePendingLiveStoryReactions"));
    assert!(h.driver.undo_live_story_reaction().is_err());
    let pending = h
        .driver
        .session
        .stories
        .live
        .as_ref()
        .and_then(|live| live.pending_reaction);
    assert!(pending.is_some(), "the Stars are still pending");
    h.recorder.refuse(None);
    h.driver.undo_live_story_reaction().unwrap();
    assert!(
        h.driver
            .session
            .stories
            .live
            .as_ref()
            .is_some_and(|live| live.pending_reaction.is_none())
    );
}

#[test]
fn a_refused_playlist_change_loads_the_list_again() {
    use crate::ids::FileId;
    let mut t = Flaky::new();
    let me = t.driver.session.my_user_id.unwrap_or_default();
    let track = |id: i32| crate::telegram::envelope::ProfileAudio {
        file_id: FileId(id),
        title: format!("t{id}"),
        performer: String::new(),
        file_name: String::new(),
        duration: 10,
    };
    t.driver
        .session
        .media
        .playlists
        .entry(me)
        .or_default()
        .set_ready(vec![track(1), track(2)]);
    t.sender.refuse(&["removeProfileAudio"]);
    assert!(t.driver.remove_profile_audio_track(FileId(1)).is_err());
    assert_eq!(t.driver.session.take_stale_playlists(), vec![me]);
    t.sender.refuse(&["setProfileAudioPosition"]);
    t.driver
        .session
        .media
        .playlists
        .entry(me)
        .or_default()
        .set_ready(vec![track(1), track(2)]);
    assert!(t.driver.move_profile_audio_track(0, 1).is_err());
    assert_eq!(t.driver.session.take_stale_playlists(), vec![me]);
}

#[test]
fn a_failed_owner_save_is_retried_by_the_next_ingest_without_a_new_claim() {
    use crate::connect::types::OwnerClaim;
    use crate::settings::AccountPaths;
    let app = std::env::temp_dir().join(format!("quill-r2-03-{}", std::process::id()));
    let mut t = Flaky::new();
    let folder = AccountPaths::for_root(&app, &t.driver.session.account).root;
    t.driver.paths.root = folder.clone();
    // The folder's place is taken by a file: the owner file cannot be saved.
    std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
    std::fs::write(&folder, b"in the way").unwrap();
    t.driver.claim_account_owner();
    assert_eq!(t.driver.owner_claim, OwnerClaim::Unsaved(7));
    std::fs::remove_file(&folder).unwrap();
    t.driver.claim_account_owner();
    assert_eq!(t.driver.owner_claim, OwnerClaim::Saved(7));
    assert!(folder.join("owner.json").exists());
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn a_commit_that_did_not_go_out_keeps_the_stars_pending_with_a_new_window() {
    use crate::paid_reaction::ACCUMULATE_MS;
    let mut h = super::live_story::watching();
    h.ingest(&super::live_story::LIVE_CALL.replace("JOINED", "true"));
    h.driver.add_live_story_reaction(2, 1_000).unwrap();
    h.recorder.refuse(Some("commitPendingLiveStoryReactions"));
    let due = 1_000 + ACCUMULATE_MS;
    assert!(h.driver.live_story_reaction_tick(due));
    let live = h.driver.session.stories.live.as_ref().unwrap();
    assert_eq!(live.pending_reaction.map(|p| p.stars), Some(2));
    assert_eq!(
        live.pending_reaction.map(|p| p.commit_at_ms),
        Some(due + ACCUMULATE_MS)
    );
    assert!(
        live.effects.is_empty(),
        "no burst for Stars that did not go out"
    );
    // Not asked again until the new window is over.
    assert!(!h.driver.live_story_reaction_tick(due + 1));
    h.recorder.refuse(None);
    assert!(h.driver.live_story_reaction_tick(due + ACCUMULATE_MS));
    assert_eq!(h.sent("commitPendingLiveStoryReactions"), 1);
}

#[test]
fn a_failed_data_storage_save_keeps_the_dirty_mark_for_the_next_ingest() {
    let dir = std::env::temp_dir().join(format!("quill-r2-03-ds-{}", std::process::id()));
    let mut t = Flaky::new();
    let good = t.driver.paths.clone();
    // The settings folder's place is taken by a file: the write fails.
    let blocked = dir.join("blocked");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&blocked, b"in the way").unwrap();
    t.driver.paths.root = blocked;
    t.driver.session.settings.data_storage_dirty = true;
    t.feed(json!({"@type":"updateOption","name":"my_id","value":{"@type":"optionValueInteger","value":7}}));
    assert!(t.driver.session.settings.data_storage_dirty);
    t.driver.paths = good;
    t.feed(json!({"@type":"updateOption","name":"my_id","value":{"@type":"optionValueInteger","value":7}}));
    assert!(!t.driver.session.settings.data_storage_dirty);
    let _ = std::fs::remove_dir_all(&dir);
}

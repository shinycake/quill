//! State reducer tests: the main profile tab.
use super::common::*;
use super::*;
use crate::profile_tab::ProfileTab;

fn private_chat(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>) {
    apply_json(
        session,
        seq,
        sink,
        r#"{"@type":"updateNewChat","chat":{"id":6,"title":"alice","type":{"@type":"chatTypePrivate","user_id":78},"unread_count":0}}"#,
    );
}

#[test]
fn update_user_full_info_stores_the_main_tab_and_opens_the_gallery_on_it() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    private_chat(&mut session, &seq, &sink);
    assert_eq!(
        session.main_profile_gallery_tab(ChatId(6)),
        SharedMediaTab::Media
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUserFullInfo","user_id":78,"user_full_info":{"@type":"userFullInfo","bio":null,"main_profile_tab":{"@type":"profileTabMusic"},"bot_info":null}}"#,
    );
    let info = session.user_full_info(78).expect("cached");
    assert_eq!(info.extras.main_profile_tab, Some(ProfileTab::Music));
    assert_eq!(
        session.main_profile_gallery_tab(ChatId(6)),
        SharedMediaTab::Music
    );
    let tab = session
        .media
        .shared_media
        .open_for_tab(ChatId(6), session.main_profile_gallery_tab(ChatId(6)));
    assert_eq!(tab, SharedMediaTab::Music);
    assert_eq!(session.media.shared_media.active_tab, SharedMediaTab::Music);
}

#[test]
fn tabs_without_a_gallery_fall_back_to_media() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    private_chat(&mut session, &seq, &sink);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateUserFullInfo","user_id":78,"user_full_info":{"@type":"userFullInfo","bio":null,"main_profile_tab":{"@type":"profileTabGifts"},"bot_info":null}}"#,
    );
    assert_eq!(
        session.user_full_info(78).unwrap().extras.main_profile_tab,
        Some(ProfileTab::Gifts)
    );
    assert_eq!(
        session.main_profile_gallery_tab(ChatId(6)),
        SharedMediaTab::Media
    );
}

#[test]
fn reopening_the_same_chat_keeps_the_current_tab() {
    let mut state = SharedMediaState::default();
    state.open_for_tab(ChatId(6), SharedMediaTab::Files);
    state.select_tab(SharedMediaTab::Links);
    assert_eq!(
        state.open_for_tab(ChatId(6), SharedMediaTab::Files),
        SharedMediaTab::Links
    );
}

#[test]
fn refused_main_tab_change_surfaces_as_a_notice() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::SetMainProfileTab, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"X"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.chats_state.chat_action_error.as_deref(),
        Some("could not save the change (error 400)")
    );
}

//! State reducer tests: B10 profile and contact panels.
use super::common::*;
use super::*;

fn photo_json(id: i64, thumb: i32, full: i32) -> String {
    format!(
        r#"{{"@type":"chatPhoto","id":{id},"added_date":1700000000,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb_file},"width":160,"height":160,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full_file},"width":800,"height":800,"progressive_sizes":[]}}]}}"#,
        thumb_file = media_file_json(thumb, "\"\"", false),
        full_file = media_file_json(full, "\"\"", false),
    )
}

#[test]
fn groups_in_common_answer_lands_by_pending_user() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(
        RequestPurpose::GetProfileChats(ProfileChatsKind::GroupsInCommon),
        31,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":2,"chat_ids":[-1001,-1002]}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session.profile_chat_list(ProfileChatsKind::GroupsInCommon, 31),
        &[-1001, -1002]
    );
    assert!(
        session
            .profile_chat_list(ProfileChatsKind::GroupsInCommon, 32)
            .is_empty()
    );
}

#[test]
fn similar_chats_and_personal_candidates_use_their_own_keys() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let similar = session.request(
        RequestPurpose::GetProfileChats(ProfileChatsKind::SimilarChats),
        Some(ChatId(-1005)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[-1006]}}"#,
            similar.0
        ),
    );
    let personal = session.request(
        RequestPurpose::GetProfileChats(ProfileChatsKind::SuitablePersonalChats),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chats","@extra":"{}","total_count":1,"chat_ids":[-1007]}}"#,
            personal.0
        ),
    );
    assert_eq!(
        session.profile_chat_list(ProfileChatsKind::SimilarChats, -1005),
        &[-1006]
    );
    assert_eq!(
        session.profile_chat_list(ProfileChatsKind::SuitablePersonalChats, 0),
        &[-1007]
    );
}

#[test]
fn refused_list_fetch_is_kept_as_failed_for_retry() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(
        RequestPurpose::GetProfileChats(ProfileChatsKind::GroupsInCommon),
        31,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"USER_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session
            .users_state
            .profile_chat_lists
            .get(&(ProfileChatsKind::GroupsInCommon, 31)),
        Some(ProfileChatsFetch::Failed(_))
    ));
}

#[test]
fn profile_photos_answer_caches_gallery_and_files() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(RequestPurpose::GetUserProfilePhotos, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatPhotos","@extra":"{}","total_count":2,"photos":[{},{}]}}"#,
            extra.0,
            photo_json(901, 11, 12),
            photo_json(902, 13, 14),
        ),
    );
    let Some(ProfilePhotosFetch::Loaded {
        total_count,
        photos,
    }) = session.users_state.user_profile_photos.get(&31)
    else {
        panic!("gallery not loaded");
    };
    assert_eq!(*total_count, 2);
    assert_eq!(photos.len(), 2);
    assert_eq!(photos[0].id, 901);
    assert_eq!(photos[0].thumb_file_id, 11);
    assert_eq!(photos[0].full_file_id, 12);
    assert_eq!(photos[0].width, 800);
    assert!(session.media.files.contains_key(&12));
    assert!(session.media.files.contains_key(&13));
}

#[test]
fn set_main_photo_ok_drops_the_own_gallery() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.my_user_id = Some(777);
    for user in [777, 31] {
        session.users_state.user_profile_photos.insert(
            user,
            ProfilePhotosFetch::Loaded {
                total_count: 0,
                photos: Vec::new(),
            },
        );
    }
    let extra = session.request(RequestPurpose::SetProfilePhoto, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(!session.users_state.user_profile_photos.contains_key(&777));
    assert!(session.users_state.user_profile_photos.contains_key(&31));
}

#[test]
fn user_full_info_keeps_personal_channel_and_note() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":null,"birthdate":{{"@type":"birthdate","day":9,"month":10,"year":0}},"personal_chat_id":-1001234,"note":{{"@type":"formattedText","text":"met at the fair","entities":[]}},"bot_info":null}}"#,
            extra.0
        ),
    );
    let info = session.user_full_info(31).expect("full info cached");
    assert_eq!(info.extras.personal_chat_id, -1001234);
    assert_eq!(info.extras.note, "met at the fair");
    assert_eq!(
        info.extras.birthdate.map(|b| (b.day, b.month)),
        Some((9, 10))
    );
}

#[test]
fn refused_profile_edits_surface_as_a_notice() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for purpose in [
        RequestPurpose::SetBirthdate,
        RequestPurpose::SetPersonalChat,
        RequestPurpose::SetUserNote,
    ] {
        session.chats_state.chat_action_error = None;
        let extra = session.request(purpose, None);
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
}

#[test]
fn user_full_info_keeps_unofficial_flag_and_personal_photo_first_in_gallery() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","uses_unofficial_app":true,"personal_photo":{},"bio":null,"bot_info":null}}"#,
            extra.0,
            photo_json(9500, 951, 952)
        ),
    );
    let info = session.user_full_info(31).expect("full info cached");
    assert!(info.extras.uses_unofficial_app);
    assert_eq!(
        info.extras.personal_photo.as_ref().map(|p| p.id),
        Some(9500)
    );
    let extra = session.request_for_user(RequestPurpose::GetUserProfilePhotos, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatPhotos","@extra":"{}","total_count":1,"photos":[{}]}}"#,
            extra.0,
            photo_json(9001, 911, 912)
        ),
    );
    let (gallery, personal) = session.profile_gallery(31).expect("gallery loaded");
    assert_eq!(personal, Some(9500));
    assert_eq!(
        gallery.iter().map(|p| p.id).collect::<Vec<_>>(),
        [9500, 9001]
    );
    // The personal photo's files are cached for display.
    assert!(session.media.files.contains_key(&952));
}

#[test]
fn refused_report_and_personal_photo_surface_as_notices() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (purpose, expected) in [
        (
            RequestPurpose::ReportChatPhoto,
            "could not send the report (error 400)",
        ),
        (
            RequestPurpose::SetUserPersonalPhoto,
            "could not save the change (error 400)",
        ),
    ] {
        session.chats_state.chat_action_error = None;
        let extra = session.request(purpose, None);
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
            Some(expected)
        );
    }
}

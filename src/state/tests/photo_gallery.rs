//! State reducer tests: the profile photo viewer's galleries for groups,
//! channels and users (full info photo, photo history pages, actions).
use super::common::*;
use super::*;

const GROUP: i64 = 81;

fn photo_json(id: i64, thumb: i32, full: i32) -> String {
    format!(
        r#"{{"@type":"chatPhoto","id":"{id}","added_date":1700000000,"sizes":[{{"@type":"photoSize","type":"m","photo":{thumb_file},"width":160,"height":160,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{full_file},"width":640,"height":640,"progressive_sizes":[]}}]}}"#,
        thumb_file = media_file_json(thumb, "\"\"", false),
        full_file = media_file_json(full, "\"\"", false),
    )
}

fn change_json(message_id: i64, photo_id: i64, file: i32) -> String {
    format!(
        r#"{{"@type":"message","id":{message_id},"chat_id":{GROUP},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageChatChangePhoto","photo":{}}}}}"#,
        photo_json(photo_id, file, file + 1)
    )
}

/// A joined supergroup (a channel when `is_channel`) whose full info
/// carries photo 503 as its current photo.
fn seed_group(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, status: &str) {
    seed_slow_mode_group(
        session,
        seq,
        sink,
        GROUP,
        Some(status),
        &format!(r#""photo":{}"#, photo_json(503, 31, 32)),
        false,
    );
}

fn apply_page(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    total: i32,
    next: i64,
    changes: &[String],
) {
    let extra = session.request(RequestPurpose::GetChatPhotoHistory, Some(ChatId(GROUP)));
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":{total},"next_from_message_id":{next},"messages":[{}]}}"#,
            extra.0,
            changes.join(",")
        ),
    );
}

fn ids(gallery: &PhotoGallery) -> Vec<(i64, GalleryPhotoKind)> {
    gallery
        .photos
        .iter()
        .map(|photo| (photo.photo.id, photo.kind))
        .collect()
}

#[test]
fn full_info_photo_opens_the_gallery_before_the_history() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink, MEMBER_STATUS);
    let gallery = session
        .photo_gallery(PhotoOwner::Chat(ChatId(GROUP)))
        .expect("the current photo is enough");
    assert_eq!(ids(&gallery), [(503, GalleryPhotoKind::Main)]);
    assert_eq!(gallery.total, 1);
    // Its sizes are known files, ready to download.
    assert!(session.media.files.contains_key(&32));
}

#[test]
fn photo_history_pages_follow_the_current_photo() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink, MEMBER_STATUS);
    // The newest change set the current photo: it is not repeated.
    apply_page(
        &mut session,
        &seq,
        &sink,
        3,
        20,
        &[change_json(30, 503, 41), change_json(20, 502, 43)],
    );
    let gallery = session
        .photo_gallery(PhotoOwner::Chat(ChatId(GROUP)))
        .expect("loaded");
    assert_eq!(
        ids(&gallery),
        [
            (503, GalleryPhotoKind::Main),
            (502, GalleryPhotoKind::Earlier)
        ]
    );
    assert_eq!(gallery.total, 3);
    assert!(gallery.has_more);
    assert_eq!(
        gallery.photos.get(1).and_then(|p| p.message_id),
        Some(MessageId(20))
    );

    // The next page is asked for and appended.
    let Some(ChatPhotosFetch::Loaded { loading_more, .. }) =
        session.users_state.chat_photo_history.get_mut(&GROUP)
    else {
        panic!("history loaded");
    };
    *loading_more = true;
    apply_page(&mut session, &seq, &sink, 3, 0, &[change_json(10, 501, 45)]);
    let gallery = session
        .photo_gallery(PhotoOwner::Chat(ChatId(GROUP)))
        .expect("loaded");
    assert_eq!(
        ids(&gallery).iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [503, 502, 501]
    );
    assert!(!gallery.has_more);
}

#[test]
fn a_current_photo_without_its_change_is_an_extra_entry() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink, MEMBER_STATUS);
    apply_page(&mut session, &seq, &sink, 1, 0, &[change_json(20, 502, 43)]);
    let gallery = session
        .photo_gallery(PhotoOwner::Chat(ChatId(GROUP)))
        .expect("loaded");
    assert_eq!(
        ids(&gallery),
        [
            (503, GalleryPhotoKind::Main),
            (502, GalleryPhotoKind::Earlier)
        ]
    );
    assert_eq!(gallery.total, 2);
}

#[test]
fn a_deleted_change_leaves_the_gallery() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink, MEMBER_STATUS);
    apply_page(
        &mut session,
        &seq,
        &sink,
        2,
        0,
        &[change_json(30, 503, 41), change_json(20, 502, 43)],
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateDeleteMessages","chat_id":{GROUP},"message_ids":[20],"is_permanent":true,"from_cache":false}}"#
        ),
    );
    let gallery = session
        .photo_gallery(PhotoOwner::Chat(ChatId(GROUP)))
        .expect("loaded");
    assert_eq!(ids(&gallery), [(503, GalleryPhotoKind::Main)]);
    assert_eq!(gallery.total, 1);
}

#[test]
fn a_new_chat_photo_drops_what_the_viewer_knew() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink, MEMBER_STATUS);
    apply_page(&mut session, &seq, &sink, 1, 0, &[change_json(30, 503, 41)]);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateChatPhoto","chat_id":{GROUP},"photo":{{"@type":"chatPhotoInfo","small":{},"big":{},"has_animation":false,"is_personal":false}}}}"#,
            media_file_json(61, "\"\"", false),
            media_file_json(62, "\"\"", false),
        ),
    );
    assert!(!session.users_state.chat_current_photos.contains_key(&GROUP));
    assert!(!session.users_state.chat_photo_history.contains_key(&GROUP));
}

#[test]
fn members_report_and_editors_manage_group_photos() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink, MEMBER_STATUS);
    apply_page(
        &mut session,
        &seq,
        &sink,
        2,
        0,
        &[change_json(30, 503, 41), change_json(20, 502, 43)],
    );
    let gallery = session
        .photo_gallery(PhotoOwner::Chat(ChatId(GROUP)))
        .expect("loaded");
    let (Some(current), Some(earlier)) = (gallery.photos.first(), gallery.photos.get(1)) else {
        panic!("two photos");
    };
    // A member without the right: report only; an earlier photo's message
    // is offered to the delete box, where TDLib's properties decide.
    let member_current = session.photo_actions(&gallery, current);
    assert_eq!(member_current.delete, None);
    assert!(!member_current.set_main && member_current.report);
    assert_eq!(
        session.photo_actions(&gallery, earlier).delete,
        Some(PhotoDelete::Message {
            chat_id: ChatId(GROUP),
            message_id: MessageId(20),
            is_outgoing: false,
        })
    );

    // The creator may delete the current photo and set an earlier one.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":{GROUP},"is_forum":false,"status":{{"@type":"chatMemberStatusCreator","is_anonymous":false,"is_member":true}}}}}}"#
        ),
    );
    let creator_current = session.photo_actions(&gallery, current);
    assert_eq!(
        creator_current.delete,
        Some(PhotoDelete::ChatPhoto {
            chat_id: ChatId(GROUP)
        })
    );
    assert!(!creator_current.set_main && !creator_current.report);
    let creator_earlier = session.photo_actions(&gallery, earlier);
    assert!(creator_earlier.set_main && !creator_earlier.report);
}

#[test]
fn own_photos_delete_and_set_main_but_never_report() {
    let (mut session, _sink) = session();
    session.my_user_id = Some(777);
    let photo = |id| ProfilePhoto {
        id,
        added_date: 0,
        thumb_file_id: 1,
        full_file_id: 2,
        width: 640,
        height: 640,
        animation: None,
    };
    let list = [photo(1), photo(2)];
    let own = user_gallery(777, &list, 2, Some(1), None, None);
    let (Some(main), Some(older)) = (own.photos.first(), own.photos.get(1)) else {
        panic!("two photos");
    };
    let main_actions = session.photo_actions(&own, main);
    assert_eq!(main_actions.delete, Some(PhotoDelete::Own { photo_id: 1 }));
    assert!(!main_actions.set_main && !main_actions.report);
    assert!(session.photo_actions(&own, older).set_main);

    let other = user_gallery(31, &list, 2, Some(1), Some(&photo(9)), None);
    let (Some(personal), Some(main)) = (other.photos.first(), other.photos.get(1)) else {
        panic!("three photos");
    };
    // The photo you set for a contact is not theirs to be reported.
    assert!(!session.photo_actions(&other, personal).report);
    let actions = session.photo_actions(&other, main);
    assert!(actions.report && actions.delete.is_none() && !actions.set_main);
}

#[test]
fn user_photos_page_and_end_with_the_public_photo() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","photo":{},"public_photo":{},"bio":null,"bot_info":null}}"#,
            extra.0,
            photo_json(9001, 911, 912),
            photo_json(9900, 991, 992)
        ),
    );
    assert!(session.media.files.contains_key(&992));
    let extra = session.request_for_user(RequestPurpose::GetUserProfilePhotos, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatPhotos","@extra":"{}","total_count":2,"photos":[{}]}}"#,
            extra.0,
            photo_json(9001, 911, 912)
        ),
    );
    let gallery = session.photo_gallery(PhotoOwner::User(31)).expect("loaded");
    // One more listed photo to load, then the public photo: 3 in all.
    assert_eq!(gallery.total, 3);
    assert!(gallery.has_more);
    assert_eq!(ids(&gallery), [(9001, GalleryPhotoKind::Main)]);

    let Some(ProfilePhotosFetch::Loaded { loading_more, .. }) =
        session.users_state.user_profile_photos.get_mut(&31)
    else {
        panic!("loaded");
    };
    *loading_more = true;
    let extra = session.request_for_user(RequestPurpose::GetUserProfilePhotos, 31);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"chatPhotos","@extra":"{}","total_count":2,"photos":[{}]}}"#,
            extra.0,
            photo_json(9002, 913, 914)
        ),
    );
    let gallery = session.photo_gallery(PhotoOwner::User(31)).expect("loaded");
    assert_eq!(
        ids(&gallery),
        [
            (9001, GalleryPhotoKind::Main),
            (9002, GalleryPhotoKind::Earlier),
            (9900, GalleryPhotoKind::Public)
        ]
    );
    assert_eq!(gallery.total, 3);
    assert!(!gallery.has_more);
}

#[test]
fn a_failed_later_page_keeps_the_loaded_photos() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink, MEMBER_STATUS);
    apply_page(
        &mut session,
        &seq,
        &sink,
        5,
        20,
        &[change_json(30, 503, 41)],
    );
    if let Some(ChatPhotosFetch::Loaded { loading_more, .. }) =
        session.users_state.chat_photo_history.get_mut(&GROUP)
    {
        *loading_more = true;
    }
    let extra = session.request(RequestPurpose::GetChatPhotoHistory, Some(ChatId(GROUP)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        session.users_state.chat_photo_history.get(&GROUP),
        Some(ChatPhotosFetch::Loaded {
            loading_more: false,
            ..
        })
    ));
}

//! State reducer tests: video profile photos. The open info panel's cover
//! clip (and its download) follows the panel, the shown photo and the
//! autoplay settings; gallery photos keep their clips.
use super::common::*;
use super::*;
use crate::telegram::envelope::ChatPhotoAnimation;

const GROUP: i64 = 82;
const USER: i64 = 8201;

fn photo_json(id: i64, file: i32, clip: Option<i32>) -> String {
    let animation = clip.map_or_else(String::new, |clip| {
        format!(
            r#","animation":{{"@type":"animatedChatPhoto","length":640,"file":{},"main_frame_timestamp":2.0}}"#,
            media_file_json(clip, "", false)
        )
    });
    format!(
        r#"{{"@type":"chatPhoto","id":"{id}","added_date":1700000000,"sizes":[{{"@type":"photoSize","type":"m","photo":{},"width":160,"height":160,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{},"width":640,"height":640,"progressive_sizes":[]}}]{animation}}}"#,
        media_file_json(file, "", false),
        media_file_json(file + 1, "", false),
    )
}

fn clip(file: i32) -> ChatPhotoAnimation {
    ChatPhotoAnimation {
        file_id: FileId(file),
        start_ms: 2000,
        small_file_id: None,
    }
}

fn seed_group(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>) {
    seed_slow_mode_group(
        session,
        seq,
        sink,
        GROUP,
        Some(MEMBER_STATUS),
        &format!(r#""photo":{}"#, photo_json(601, 61, Some(69))),
        false,
    );
}

#[test]
fn the_cover_clip_follows_the_open_panel() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink);
    assert_eq!(session.info_cover_animation(), None);
    assert!(session.info_cover_file_ids_to_download().is_empty());

    session.users_state.open_info_panel = Some(InfoPanelTarget::Supergroup(GROUP));
    assert_eq!(
        session.info_cover_animation(),
        Some((PhotoOwner::Chat(ChatId(GROUP)), clip(69)))
    );
    // The clip is a known file, downloaded while the panel is open.
    assert!(session.media.files.contains_key(&69));
    assert_eq!(session.info_cover_file_ids_to_download(), [FileId(69)]);

    // Closing the panel stops the clip: nothing plays or downloads.
    session.users_state.open_info_panel = None;
    assert_eq!(session.info_cover_animation(), None);
    assert!(session.info_cover_file_ids_to_download().is_empty());
}

#[test]
fn autoplay_off_or_data_saver_keeps_the_still() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink);
    session.users_state.open_info_panel = Some(InfoPanelTarget::Supergroup(GROUP));
    session.settings.media_prefs.autoplay_gifs = false;
    assert_eq!(session.info_cover_animation(), None);
    assert!(session.info_cover_file_ids_to_download().is_empty());
    session.settings.media_prefs.autoplay_gifs = true;
    session.settings.media_prefs.data_saver = true;
    assert_eq!(session.info_cover_animation(), None);
}

#[test]
fn earlier_chat_photos_keep_their_clips_in_the_gallery() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    seed_group(&mut session, &seq, &sink);
    let extra = session.request(RequestPurpose::GetChatPhotoHistory, Some(ChatId(GROUP)));
    let change = format!(
        r#"{{"@type":"message","id":20,"chat_id":{GROUP},"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageChatChangePhoto","photo":{}}}}}"#,
        photo_json(600, 63, Some(68))
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":2,"next_from_message_id":0,"messages":[{change}]}}"#,
            extra.0
        ),
    );
    let gallery = session
        .photo_gallery(PhotoOwner::Chat(ChatId(GROUP)))
        .expect("loaded");
    let clips: Vec<_> = gallery
        .photos
        .iter()
        .map(|photo| (photo.photo.id, photo.photo.animation))
        .collect();
    assert_eq!(clips, [(601, Some(clip(69))), (600, Some(clip(68)))]);
    assert!(session.media.files.contains_key(&68));
}

fn apply_user_full_info(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    fields: &str,
) {
    let extra = session.request_for_user(RequestPurpose::GetUserFullInfo, USER);
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","bio":{{"@type":"formattedText","text":"","entities":[]}}{fields}}}"#,
            extra.0
        ),
    );
}

#[test]
fn a_users_cover_plays_the_shown_photo() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_user_full_info(
        &mut session,
        &seq,
        &sink,
        &format!(r#","photo":{}"#, photo_json(701, 71, Some(79))),
    );
    session.users_state.open_info_panel = Some(InfoPanelTarget::User(USER));
    assert_eq!(
        session.info_cover_animation(),
        Some((PhotoOwner::User(USER), clip(79)))
    );
    assert!(session.media.files.contains_key(&79));

    // A photo you set for the contact is the one shown: a still one shows
    // no clip.
    apply_user_full_info(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#","photo":{},"personal_photo":{}"#,
            photo_json(701, 71, Some(79)),
            photo_json(702, 73, None)
        ),
    );
    assert_eq!(session.info_cover_animation(), None);

    // Saved Messages never plays one.
    apply_user_full_info(
        &mut session,
        &seq,
        &sink,
        &format!(r#","photo":{}"#, photo_json(701, 71, Some(79))),
    );
    session.my_user_id = Some(USER);
    assert_eq!(session.info_cover_animation(), None);
}

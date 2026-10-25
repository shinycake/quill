//! State reducer tests: video userpics beside messages and in the chat
//! list. A Premium user's video photo plays its small clip once the full
//! info names it; everyone else, and every still photo, keeps the still.
use super::common::*;
use super::*;

const USER: i64 = 8301;

fn update_user(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, photo: &str) {
    update_user_premium(session, seq, sink, photo, true);
}

fn update_user_premium(
    session: &mut Session,
    seq: &AtomicU64,
    sink: &Arc<MemorySink>,
    photo: &str,
    premium: bool,
) {
    apply_json(
        session,
        seq,
        sink,
        &format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{USER},"first_name":"Noa","last_name":"","is_premium":{premium},"profile_photo":{photo},"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
    );
}

fn profile_photo(id: i64, video: bool) -> String {
    format!(
        r#"{{"@type":"profilePhoto","id":"{id}","small":{},"big":{},"has_animation":{video},"is_personal":false}}"#,
        media_file_json(81, "", false),
        media_file_json(82, "", false),
    )
}

/// A `chatPhoto` with a large clip and, when `small`, the 160 px one.
fn chat_photo(id: i64, small: bool) -> String {
    let small = if small {
        format!(
            r#","small_animation":{{"@type":"animatedChatPhoto","length":160,"file":{},"main_frame_timestamp":0}}"#,
            media_file_json(89, "", false)
        )
    } else {
        String::new()
    };
    format!(
        r#"{{"@type":"chatPhoto","id":"{id}","added_date":1,"sizes":[{{"@type":"photoSize","type":"a","photo":{},"width":160,"height":160,"progressive_sizes":[]}}],"animation":{{"@type":"animatedChatPhoto","length":640,"file":{},"main_frame_timestamp":1.0}}{small}}}"#,
        media_file_json(83, "", false),
        media_file_json(88, "", false),
    )
}

fn full_info(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, fields: &str) {
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
fn a_premium_users_video_photo_plays_its_small_clip() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    update_user(&mut session, &seq, &sink, &profile_photo(901, true));
    let user = session.user(USER).expect("known");
    assert_eq!((user.photo_id, user.photo_has_animation), (901, true));
    assert_eq!(session.userpic_video(USER), UserpicVideo::Unknown);
    assert!(session.take_userpic_full_info_ask(USER));
    assert!(!session.take_userpic_full_info_ask(USER));

    full_info(
        &mut session,
        &seq,
        &sink,
        &format!(r#","photo":{}"#, chat_photo(901, true)),
    );
    assert_eq!(session.userpic_video(USER), UserpicVideo::Clip(FileId(89)));
    assert!(session.media.files.contains_key(&89));
    assert!(!session.take_userpic_full_info_ask(USER));

    full_info(
        &mut session,
        &seq,
        &sink,
        &format!(r#","photo":{}"#, chat_photo(901, false)),
    );
    assert_eq!(session.userpic_video(USER), UserpicVideo::Clip(FileId(88)));
}

#[test]
fn a_new_photo_asks_for_the_full_info_again_once() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    update_user(&mut session, &seq, &sink, &profile_photo(901, true));
    full_info(
        &mut session,
        &seq,
        &sink,
        &format!(r#","photo":{}"#, chat_photo(901, true)),
    );
    assert!(!session.take_userpic_full_info_ask(USER));
    update_user(&mut session, &seq, &sink, &profile_photo(902, true));
    assert_eq!(session.userpic_video(USER), UserpicVideo::Unknown);
    assert!(session.take_userpic_full_info_ask(USER));
    full_info(&mut session, &seq, &sink, "");
    assert_eq!(session.userpic_video(USER), UserpicVideo::Unknown);
    assert!(!session.take_userpic_full_info_ask(USER));
}

#[test]
fn stills_stay_still() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    update_user_premium(&mut session, &seq, &sink, &profile_photo(901, true), false);
    full_info(
        &mut session,
        &seq,
        &sink,
        &format!(r#","photo":{}"#, chat_photo(901, true)),
    );
    assert_eq!(session.userpic_video(USER), UserpicVideo::Still);
    assert!(!session.take_userpic_full_info_ask(USER));
    update_user(&mut session, &seq, &sink, &profile_photo(903, false));
    assert_eq!(session.userpic_video(USER), UserpicVideo::Still);
    update_user(&mut session, &seq, &sink, "null");
    assert_eq!(session.userpic_video(USER), UserpicVideo::Still);
    assert_eq!(session.userpic_video(USER + 1), UserpicVideo::Still);
}

#[test]
fn autoplay_off_or_data_saver_keeps_the_still() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    update_user(&mut session, &seq, &sink, &profile_photo(901, true));
    full_info(
        &mut session,
        &seq,
        &sink,
        &format!(r#","photo":{}"#, chat_photo(901, true)),
    );
    session.settings.media_prefs.autoplay_gifs = false;
    assert_eq!(session.userpic_video(USER), UserpicVideo::Still);
    session.settings.media_prefs.autoplay_gifs = true;
    session.settings.media_prefs.data_saver = true;
    assert_eq!(session.userpic_video(USER), UserpicVideo::Still);
    assert!(!session.take_userpic_full_info_ask(USER));
}

#[test]
fn a_personal_photo_is_the_one_matched() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    update_user(&mut session, &seq, &sink, &profile_photo(905, true));
    full_info(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#","photo":{},"personal_photo":{}"#,
            chat_photo(901, false),
            chat_photo(905, true)
        ),
    );
    assert_eq!(session.userpic_video(USER), UserpicVideo::Clip(FileId(89)));
}

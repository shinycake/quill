//! Driver tests: opening a private chat fetches the user's full info, whose
//! `has_restricted_voice_and_video_note_messages` refuses voice and video
//! messages (tdesktop `VoiceMessagesForbidden`).
use super::paid_messages::{Fixture, cleanup, extra_of, fixture, ingest, open_private, requests};
use crate::ids::ChatId;
use crate::send_rights::{SendKind, SendRights};

fn rights(f: &Fixture, chat_id: i64) -> SendRights {
    let session = &f.1.session;
    let chat = session.chats.get(&chat_id).expect("chat");
    SendRights::for_chat(session, chat, 0)
}

#[test]
fn opening_a_private_chat_reads_whether_the_user_accepts_voice_messages() {
    let mut f = fixture();
    open_private(&mut f, 600, 601, true, false);
    f.1.fetch_open_chat_extras(ChatId(600)).unwrap();
    let asks = requests(&f, "getUserFullInfo");
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0]["user_id"], 601);
    // Still in flight: a second open does not ask again.
    f.1.maybe_fetch_open_user_full_info().unwrap();
    assert_eq!(requests(&f, "getUserFullInfo").len(), 1);
    assert!(rights(&f, 600).allows(SendKind::VoiceMessages));

    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"userFullInfo","@extra":"{}","has_restricted_voice_and_video_note_messages":true,"bio":null}}"#,
            extra_of(&asks[0])
        ),
    );
    let r = rights(&f, 600);
    assert!(!r.allows(SendKind::VoiceMessages));
    assert!(!r.allows(SendKind::VideoMessages));
    assert!(r.allows(SendKind::Message));
    assert_eq!(
        r.denial(SendKind::VoiceMessages, &|_| String::new())
            .as_deref(),
        Some("Ann doesn't accept voice messages.")
    );
    // Cached: no refetch on the next open.
    f.1.maybe_fetch_open_user_full_info().unwrap();
    assert_eq!(requests(&f, "getUserFullInfo").len(), 1);

    // The user lifts the restriction.
    ingest(
        &mut f,
        r#"{"@type":"updateUserFullInfo","user_id":601,"user_full_info":{"@type":"userFullInfo","has_restricted_voice_and_video_note_messages":false}}"#,
    );
    assert!(rights(&f, 600).allows(SendKind::VoiceMessages));
    cleanup(f);
}

#[test]
fn bots_and_saved_messages_skip_the_user_fetch() {
    let mut f = fixture();
    open_private(&mut f, 610, 611, false, true);
    f.1.maybe_fetch_open_user_full_info().unwrap();
    assert!(requests(&f, "getUserFullInfo").is_empty());
    f.1.session.my_user_id = Some(621);
    open_private(&mut f, 620, 621, false, false);
    f.1.maybe_fetch_open_user_full_info().unwrap();
    assert!(requests(&f, "getUserFullInfo").is_empty());
    cleanup(f);
}

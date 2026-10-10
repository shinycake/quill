//! One test per service action: recorded TDLib JSON in, Telegram
//! Desktop's wording (and the links) out.
use crate::service_text::{PinTarget, ServiceCtx, ServiceLink, ServiceNames, ServiceText, render};
use crate::telegram::envelope::{
    MessageContent, MessageSender, ParsedFile, ServiceAction, parse_content,
};
use serde_json::Value;

const ME: i64 = 9;
const DANA: i64 = 1;
const OMAR: i64 = 2;
const LEA: i64 = 3;
const CHAT: i64 = -1001;

struct Names;

impl ServiceNames for Names {
    fn user_name(&self, id: i64) -> Option<String> {
        match id {
            DANA => Some("Dana Cole".into()),
            OMAR => Some("Omar Haddad".into()),
            LEA => Some("Lea Stern".into()),
            ME => Some("Idan Birman".into()),
            _ => None,
        }
    }
    fn user_short_name(&self, id: i64) -> Option<String> {
        self.user_name(id)
            .map(|n| n.split(' ').next().unwrap_or("").to_string())
    }
    fn chat_name(&self, id: i64) -> Option<String> {
        match id {
            CHAT => Some("Design Club".into()),
            -2002 => Some("Launch Channel".into()),
            _ => None,
        }
    }
    fn pinned(&self, id: i64) -> PinTarget {
        match id {
            100 => PinTarget::Text("Lunch at noon, bring snacks!".into()),
            101 => PinTarget::Text("Short note".into()),
            102 => PinTarget::Media("a photo".into()),
            _ => PinTarget::Loading,
        }
    }
    fn game_title(&self, id: i64) -> Option<String> {
        (id == 300).then(|| "Lumberjack".to_string())
    }
}

#[derive(Clone, Copy)]
struct Opts {
    actor: Option<MessageSender>,
    outgoing: bool,
    channel: bool,
}

const BY_DANA: Opts = Opts {
    actor: Some(MessageSender::User { user_id: DANA }),
    outgoing: false,
    channel: false,
};
const BY_ME: Opts = Opts {
    actor: Some(MessageSender::User { user_id: ME }),
    outgoing: true,
    channel: false,
};
const IN_CHANNEL: Opts = Opts {
    actor: None,
    outgoing: false,
    channel: true,
};

fn parse(json: &str) -> (MessageContent, Vec<ParsedFile>) {
    let value: Value = serde_json::from_str(json).expect("valid json");
    parse_content(Some(&value))
}

fn run(json: &str, opts: Opts) -> ServiceText {
    let (content, _) = parse(json);
    let ctx = ServiceCtx {
        actor: opts.actor,
        is_outgoing: opts.outgoing,
        my_id: Some(ME),
        is_channel: opts.channel,
        is_secret: false,
        chat_id: CHAT,
        now: 1_800_000_000,
        names: &Names,
    };
    render(&content, &ctx).unwrap_or_else(|| panic!("not a service message: {json}"))
}

fn say(json: &str, opts: Opts) -> String {
    run(json, opts).plain()
}

macro_rules! wording {
    ($name:ident, $opts:expr, $json:expr, $expected:expr) => {
        #[test]
        fn $name() {
            assert_eq!(say($json, $opts), $expected);
        }
    };
}

mod chats;
mod content;
mod payments;

#[test]
fn video_chat_scheduled_says_today_tomorrow_or_the_date() {
    let at = |offset: i64| {
        let start = 1_800_000_000 + offset;
        say(
            &format!(
                r#"{{"@type":"messageVideoChatScheduled","group_call_id":5,"start_date":{start}}}"#
            ),
            BY_DANA,
        )
    };
    assert!(at(60).starts_with("Dana Cole scheduled a video chat for today at "));
    assert!(at(86_400 + 60).starts_with("Dana Cole scheduled a video chat for tomorrow at "));
    let later = at(9 * 86_400);
    assert!(later.starts_with("Dana Cole scheduled a video chat for "));
    assert!(!later.contains("today") && !later.contains("tomorrow"));
    assert!(later.contains(" at "));
    let channel = say(
        r#"{"@type":"messageVideoChatScheduled","group_call_id":5,"start_date":1800000060}"#,
        IN_CHANNEL,
    );
    assert!(channel.starts_with("Live stream scheduled for today at "));
}

#[test]
fn actor_and_target_names_are_links_to_their_profiles() {
    let text = run(
        r#"{"@type":"messageChatAddMembers","member_user_ids":[2,3]}"#,
        BY_DANA,
    );
    let (plain, links) = text.spans();
    assert_eq!(plain, "Dana Cole added Omar Haddad and Lea Stern");
    let linked: Vec<(&str, &ServiceLink)> = links
        .iter()
        .map(|(range, link)| (&plain[range.clone()], link))
        .collect();
    assert_eq!(
        linked,
        vec![
            (
                "Dana Cole",
                &ServiceLink::Sender(MessageSender::User { user_id: DANA })
            ),
            (
                "Omar Haddad",
                &ServiceLink::Sender(MessageSender::User { user_id: OMAR })
            ),
            (
                "Lea Stern",
                &ServiceLink::Sender(MessageSender::User { user_id: LEA })
            ),
        ]
    );
}

#[test]
fn pinned_excerpt_links_to_the_pinned_message() {
    let text = run(r#"{"@type":"messagePinMessage","message_id":100}"#, BY_DANA);
    let (plain, links) = text.spans();
    let (range, link) = links.last().expect("a link for the excerpt");
    assert_eq!(&plain[range.clone()], "Lunch at noon, b\u{2026}");
    assert_eq!(link, &ServiceLink::Message(100));
}

#[test]
fn game_score_title_links_to_the_game_message() {
    let text = run(
        r#"{"@type":"messageGameScore","game_message_id":300,"game_id":"5","score":9}"#,
        BY_DANA,
    );
    let (_, links) = text.spans();
    assert_eq!(links.last().unwrap().1, ServiceLink::Message(300));
}

#[test]
fn unknown_users_read_as_someone_instead_of_a_blank() {
    assert_eq!(
        say(
            r#"{"@type":"messageChatDeleteMember","user_id":77}"#,
            BY_DANA
        ),
        "Dana Cole removed Someone"
    );
}

#[test]
fn bot_website_domain_is_an_external_link() {
    let text = run(
        r#"{"@type":"messageBotWriteAccessAllowed","reason":{"@type":"botWriteAccessAllowReasonConnectedWebsite","domain_name":"example.org"}}"#,
        BY_ME,
    );
    let (_, links) = text.spans();
    assert_eq!(links[0].1, ServiceLink::Url("http://example.org".into()));
}

#[test]
fn chat_photo_change_keeps_the_new_photo_for_the_thumbnail() {
    let json = r#"{"@type":"messageChatChangePhoto","photo":{"@type":"chatPhoto","id":"5","added_date":1,"minithumbnail":{"@type":"minithumbnail","width":8,"height":8,"data":"AQID"},"sizes":[{"@type":"photoSize","type":"a","photo":{"@type":"file","id":77,"size":900,"expected_size":900,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"x","unique_id":"y","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":900}},"width":160,"height":160}]}}"#;
    let (content, files) = parse(json);
    let MessageContent::Action(action) = content else {
        panic!("a service action");
    };
    let ServiceAction::ChatPhoto { photo: Some(photo) } = *action else {
        panic!("a chat photo with sizes");
    };
    assert_eq!(photo.sizes.len(), 1);
    assert_eq!(photo.sizes[0].file_id.0, 77);
    assert!(photo.minithumbnail.is_some());
    assert_eq!(files.len(), 1);
}

#[test]
fn ordinary_content_is_not_a_service_message() {
    let (content, _) = parse(r#"{"@type":"messageText","text":{"text":"hi","entities":[]}}"#);
    let ctx = ServiceCtx {
        actor: None,
        is_outgoing: false,
        my_id: None,
        is_channel: false,
        is_secret: false,
        chat_id: CHAT,
        now: 0,
        names: &Names,
    };
    assert!(render(&content, &ctx).is_none());
}

#[test]
fn unknown_constructors_stay_unsupported() {
    let (content, _) = parse(r#"{"@type":"messageFromTheFuture"}"#);
    assert!(matches!(content, MessageContent::Unsupported { .. }));
    let (content, _) = parse(r#"{"@type":"messageUnsupported"}"#);
    assert!(matches!(content, MessageContent::Unsupported { .. }));
}

#[test]
fn nameless_preview_never_says_loading() {
    let (content, _) = parse(r#"{"@type":"messagePinMessage","message_id":5}"#);
    assert_eq!(content.preview(), "Someone pinned a message");
}

#[test]
fn birthday_card_table_hides_an_unset_year() {
    use crate::service_text::birthday_table;
    assert_eq!(
        birthday_table(6, 10, 1990),
        vec![
            ("Day", "6".to_string()),
            ("Month", "October".to_string()),
            ("Year", "1990".to_string())
        ]
    );
    assert_eq!(birthday_table(6, 10, 0).len(), 2);
}

#[test]
fn birthday_card_prefills_the_form_or_nothing() {
    use crate::service_text::birthday_form_parts;
    assert_eq!(birthday_form_parts(6, 10, 1990), Some((6, 10, Some(1990))));
    assert_eq!(birthday_form_parts(29, 2, 0), Some((29, 2, None)));
    assert_eq!(birthday_form_parts(0, 10, 0), None);
    assert_eq!(birthday_form_parts(6, 13, 0), None);
}

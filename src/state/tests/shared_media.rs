//! State reducer tests: shared_media.
use super::common::*;
use super::*;

#[test]
fn shared_media_empty_ready_failed_and_stale_drop() {
    // Slice media-shared-gallery: per-tab fetch state machine through
    // the real `foundChatMessages` / error reducer paths.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let chat = ChatId(11);
    let tab = session.shared_media.open_for(chat);
    assert_eq!(tab, SharedMediaTab::Media);
    // Empty answer → Empty, never Ready.
    let generation = session.shared_media.begin_fetch(SharedMediaTab::Media);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Media,
            generation,
        },
        Some(chat),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":0,"next_from_message_id":0,"messages":[]}}"#,
            extra.0
        ),
    );
    let media = &session.shared_media.tabs[SharedMediaTab::Media.index()];
    assert_eq!(media.status, SharedMediaTabStatus::Empty);
    assert!(media.items.is_empty());
    // Ready answer → Ready with items.
    let generation = session.shared_media.begin_fetch(SharedMediaTab::Files);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Files,
            generation,
        },
        Some(chat),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":201,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"report.pdf","mime_type":"application/pdf","document":{{"@type":"file","id":901,"size":1,"expected_size":1,"local":{{"@type":"localFile","path":"","is_downloading_completed":false,"is_downloading_active":false}},"remote":{{"@type":"remoteFile","id":"x"}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}]}}"#,
            extra.0
        ),
    );
    let files = &session.shared_media.tabs[SharedMediaTab::Files.index()];
    assert_eq!(files.status, SharedMediaTabStatus::Ready);
    assert_eq!(files.items.len(), 1);
    assert_eq!(files.items[0].label, "report.pdf");
    // Stale generation → dropped, tab keeps its old state.
    let generation = session.shared_media.begin_fetch(SharedMediaTab::Music);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Music,
            generation,
        },
        Some(chat),
    );
    session.shared_media.close();
    session.shared_media.open_for(ChatId(12));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[]}}"#,
            extra.0
        ),
    );
    let music = &session.shared_media.tabs[SharedMediaTab::Music.index()];
    assert_eq!(music.status, SharedMediaTabStatus::Idle);
    // Error → Failed with the server text.
    let generation = session.shared_media.begin_fetch(SharedMediaTab::Links);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Links,
            generation,
        },
        Some(ChatId(12)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_INVALID"}}"#,
            extra.0
        ),
    );
    let links = &session.shared_media.tabs[SharedMediaTab::Links.index()];
    assert_eq!(links.status, SharedMediaTabStatus::Failed);
    assert!(!links.error.is_empty());
}

#[test]
fn shared_media_tgx_copy_is_verbatim() {
    // Slice media-shared-gallery: the per-tab empty-state copy matches
    // TGX's strings.xml (chat + channel variants).
    assert_eq!(SharedMediaTab::Media.empty_title(), "No media to show");
    assert_eq!(
        SharedMediaTab::Media.empty_hint(false),
        "Share photos and videos in this chat and\naccess them on any of your devices."
    );
    assert_eq!(
        SharedMediaTab::Media.empty_hint(true),
        "Published photos and videos\nwill be shown here."
    );
    assert_eq!(
        SharedMediaTab::Gifs.empty_hint(false),
        "Share GIFs in this chat and\naccess them on any device you have."
    );
    assert_eq!(
        SharedMediaTab::Files.empty_hint(true),
        "Published documents and files\nwill be shown here."
    );
    for tab in SharedMediaTab::ALL {
        assert!(!tab.label().is_empty());
        assert!(!tab.glyph().is_empty());
        assert!(tab.filter_constructor().starts_with("searchMessagesFilter"));
    }
}

#[test]
fn open_chat_closes_shared_media_gallery() {
    // Slice media-shared-gallery: gallery open for chat A, switching to
    // chat B closes it (B1 — otherwise the panel shows A's title beside
    // B's conversation and row jumps resolve the message id against B);
    // the same-chat path leaves it open.
    let (mut session, _sink) = session();
    session.shared_media.open_for(ChatId(11));
    assert!(session.shared_media.open);
    session.open_chat(ChatId(12));
    assert!(!session.shared_media.open);
    assert_eq!(session.shared_media.chat_id, None);
    session.shared_media.open_for(ChatId(12));
    session.open_chat(ChatId(12));
    assert!(session.shared_media.open);
    assert_eq!(session.shared_media.chat_id, Some(ChatId(12)));
}

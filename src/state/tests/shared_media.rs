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
    let tab = session.media.shared_media.open_for(chat);
    assert_eq!(tab, SharedMediaTab::Media);
    // Empty answer → Empty, never Ready.
    let generation = session
        .media
        .shared_media
        .begin_fetch(SharedMediaTab::Media);
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
    let media = &session.media.shared_media.tabs[SharedMediaTab::Media.index()];
    assert_eq!(media.status, SharedMediaTabStatus::Empty);
    assert!(media.items.is_empty());
    // Ready answer → Ready with items.
    let generation = session
        .media
        .shared_media
        .begin_fetch(SharedMediaTab::Files);
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
    let files = &session.media.shared_media.tabs[SharedMediaTab::Files.index()];
    assert_eq!(files.status, SharedMediaTabStatus::Ready);
    assert_eq!(files.items.len(), 1);
    assert_eq!(files.items[0].label, "report.pdf");
    // Stale generation → dropped, tab keeps its old state.
    let generation = session
        .media
        .shared_media
        .begin_fetch(SharedMediaTab::Music);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Music,
            generation,
        },
        Some(chat),
    );
    session.media.shared_media.close();
    session.media.shared_media.open_for(ChatId(12));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[]}}"#,
            extra.0
        ),
    );
    let music = &session.media.shared_media.tabs[SharedMediaTab::Music.index()];
    assert_eq!(music.status, SharedMediaTabStatus::Idle);
    // Error → Failed with the server text.
    let generation = session
        .media
        .shared_media
        .begin_fetch(SharedMediaTab::Links);
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
    let links = &session.media.shared_media.tabs[SharedMediaTab::Links.index()];
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
    session.media.shared_media.open_for(ChatId(11));
    assert!(session.media.shared_media.open);
    session.open_chat(ChatId(12));
    assert!(!session.media.shared_media.open);
    assert_eq!(session.media.shared_media.chat_id, None);
    session.media.shared_media.open_for(ChatId(12));
    session.open_chat(ChatId(12));
    assert!(session.media.shared_media.open);
    assert_eq!(session.media.shared_media.chat_id, Some(ChatId(12)));
}

fn found_documents(extra: RequestId, total: i32, next_from: i64, ids: &[i64]) -> String {
    let messages: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"id":{id},"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"f{id}.pdf","mime_type":"application/pdf","document":{{"@type":"file","id":{file},"size":1,"expected_size":1,"local":{{"@type":"localFile","path":"","is_downloading_completed":false,"is_downloading_active":false}},"remote":{{"@type":"remoteFile","id":"x"}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}"#,
                file = 1000 + id
            )
        })
        .collect();
    format!(
        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":{total},"next_from_message_id":{next_from},"messages":[{}]}}"#,
        extra.0,
        messages.join(",")
    )
}

#[test]
fn shared_media_pages_older_messages_for_the_viewer() {
    // The viewer over a Shared Media list asks for the next older page
    // with `from_message_id = next_from_message_id` and appends it.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let chat = ChatId(11);
    let tab = SharedMediaTab::Media;
    session.media.shared_media.open_for(chat);
    let generation = session.media.shared_media.begin_fetch(tab);
    let extra = session.request(
        RequestPurpose::GetSharedMedia { tab, generation },
        Some(chat),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &found_documents(extra, 4, 150, &[200, 150]),
    );
    let state = &session.media.shared_media.tabs[tab.index()];
    assert_eq!(state.status, SharedMediaTabStatus::Ready);
    assert_eq!(state.next_from, MessageId(150));
    assert!(state.can_load_more());
    assert!(
        state.items.iter().all(|item| item.message.is_some()),
        "the viewable tabs keep the messages"
    );

    let (more_generation, from) = session.media.shared_media.begin_fetch_more(tab).unwrap();
    assert_eq!(from, MessageId(150));
    assert_eq!(more_generation, generation, "paging keeps the generation");
    assert!(
        session.media.shared_media.begin_fetch_more(tab).is_none(),
        "one page in flight at a time"
    );
    let extra = session.request(
        RequestPurpose::Media(MediaPurpose::GetSharedMediaMore {
            tab,
            generation: more_generation,
        }),
        Some(chat),
    );
    // The page repeats message 150 (TDLib includes `from_message_id`).
    apply_json(
        &mut session,
        &seq,
        &sink,
        &found_documents(extra, 4, 0, &[150, 120, 100]),
    );
    let state = &session.media.shared_media.tabs[tab.index()];
    let ids: Vec<i64> = state.items.iter().map(|item| item.message_id.0).collect();
    assert_eq!(ids, vec![200, 150, 120, 100]);
    assert_eq!(state.total_count, 4);
    assert!(!state.loading_more);
    assert!(!state.can_load_more(), "oldest message reached");
    assert!(session.media.shared_media.begin_fetch_more(tab).is_none());
}

#[test]
fn shared_media_older_page_failure_allows_a_retry_and_stale_pages_drop() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let chat = ChatId(11);
    let tab = SharedMediaTab::Media;
    session.media.shared_media.open_for(chat);
    let generation = session.media.shared_media.begin_fetch(tab);
    let extra = session.request(
        RequestPurpose::GetSharedMedia { tab, generation },
        Some(chat),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &found_documents(extra, 9, 150, &[200, 150]),
    );
    let (g, _) = session.media.shared_media.begin_fetch_more(tab).unwrap();
    let extra = session.request(
        RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { tab, generation: g }),
        Some(chat),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"TIMEOUT"}}"#,
            extra.0
        ),
    );
    let state = &session.media.shared_media.tabs[tab.index()];
    assert_eq!(state.status, SharedMediaTabStatus::Ready, "list stays");
    assert_eq!(state.items.len(), 2);
    assert!(
        state.can_load_more(),
        "the failed page can be requested again"
    );

    // A page that lands after the gallery closed is dropped.
    let (g, _) = session.media.shared_media.begin_fetch_more(tab).unwrap();
    let extra = session.request(
        RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { tab, generation: g }),
        Some(chat),
    );
    session.media.shared_media.close();
    session.media.shared_media.open_for(chat);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &found_documents(extra, 9, 0, &[120]),
    );
    assert!(
        session.media.shared_media.tabs[tab.index()]
            .items
            .is_empty()
    );
}

#[test]
fn the_calendar_opens_for_the_active_tab() {
    let (mut session, _sink) = session();
    session.media.shared_media.open_for(ChatId(7));
    let calendar = session
        .open_shared_media_calendar(1_791_460_800)
        .cloned()
        .expect("media tab has a calendar");
    assert_eq!(calendar.chat_id, ChatId(7));
    assert_eq!(
        calendar.media,
        crate::search_filters::SearchMediaKind::Media
    );
    assert_eq!(calendar.shared_tab, Some(SharedMediaTab::Media));
    session.media.shared_media.select_tab(SharedMediaTab::Gifs);
    assert!(session.open_shared_media_calendar(1_791_460_800).is_none());
}

#[test]
fn a_date_jump_page_starts_at_the_picked_day() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let chat = ChatId(11);
    session.media.shared_media.open_for(chat);
    let day = crate::search_filters::day_number(2026, 10, 9);
    let generation = session
        .media
        .shared_media
        .begin_fetch_at_day(SharedMediaTab::Media, day);
    let extra = session.request(
        RequestPurpose::GetSharedMedia {
            tab: SharedMediaTab::Media,
            generation,
        },
        Some(chat),
    );
    let noon = |d: i64| (day + d) * 86_400 + 43_200;
    let message = |id: i64, date: i64| {
        format!(
            r#"{{"id":{id},"chat_id":11,"date":{date},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m","entities":[]}}}}}}"#
        )
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":3,"next_from_message_id":0,"messages":[{},{},{}]}}"#,
            extra.0,
            message(30, noon(1)),
            message(20, noon(0)),
            message(10, noon(-1)),
        ),
    );
    let tab = &session.media.shared_media.tabs[SharedMediaTab::Media.index()];
    let ids: Vec<i64> = tab.items.iter().map(|i| i.message_id.0).collect();
    assert_eq!(ids, vec![20, 10]);
    assert_eq!(tab.anchor_day, Some(day));
}

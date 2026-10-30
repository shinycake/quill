//! State reducer tests: the gallery's profile tabs (Stories, Gifts,
//! Playlist) and the new message tabs, through the real answer paths.
use super::common::*;
use super::*;

fn open_user_gallery(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>) {
    apply_json(
        session,
        seq,
        sink,
        r#"{"@type":"updateNewChat","chat":{"id":6,"title":"alice","type":{"@type":"chatTypePrivate","user_id":78},"unread_count":0}}"#,
    );
    session.media.shared_media.open_for(ChatId(6));
}

fn tab(session: &Session, tab: SharedMediaTab) -> &SharedMediaTabState {
    &session.media.shared_media.tabs[tab.index()]
}

#[test]
fn saved_tracks_fill_the_users_playlist_and_page() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    open_user_gallery(&mut session, &seq, &sink);
    let extra = session.request_for_user(
        RequestPurpose::Media(MediaPurpose::GetProfileAudios { append: false }),
        78,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"audios","@extra":"{}","total_count":3,"audios":[{{"@type":"audio","duration":214,"title":"Night Drive","performer":"Ada","file_name":"n.mp3","mime_type":"audio/mpeg"}},{{"@type":"audio","duration":0,"title":"","performer":"","file_name":"b.ogg","mime_type":"audio/ogg"}}]}}"#,
            extra.0
        ),
    );
    let playlist = &session.media.playlists[&78];
    assert!(playlist.loaded);
    assert_eq!(playlist.tracks.len(), 2);
    assert_eq!(track_name(&playlist.tracks[0]), "Ada — Night Drive");
    assert_eq!(track_name(&playlist.tracks[1]), "b.ogg");
    assert!(playlist.has_more());
    // The gallery tabs are not touched: the playlist is not a tab.
    assert!(
        session
            .shared_media_visible_tabs()
            .iter()
            .all(|tab| tab.lists_messages()
                || matches!(tab, SharedMediaTab::Stories | SharedMediaTab::Gifts))
    );
    // The next page appends and ends the list.
    let extra = session.request_for_user(
        RequestPurpose::Media(MediaPurpose::GetProfileAudios { append: true }),
        78,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"audios","@extra":"{}","total_count":3,"audios":[{{"@type":"audio","duration":9,"title":"Last","performer":"Bo","file_name":"","mime_type":""}}]}}"#,
            extra.0
        ),
    );
    let playlist = &session.media.playlists[&78];
    assert_eq!(playlist.tracks.len(), 3);
    assert!(!playlist.has_more());
}

#[test]
fn a_failed_playlist_request_is_loaded_and_empty() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    open_user_gallery(&mut session, &seq, &sink);
    let extra = session.request_for_user(
        RequestPurpose::Media(MediaPurpose::GetProfileAudios { append: false }),
        78,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"USER_PRIVACY"}}"#,
            extra.0
        ),
    );
    let playlist = &session.media.playlists[&78];
    assert!(playlist.loaded && playlist.tracks.is_empty() && playlist.error.is_some());
}

#[test]
fn received_gifts_fill_the_gifts_tab_without_touching_the_gifts_dialog() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    open_user_gallery(&mut session, &seq, &sink);
    session
        .media
        .shared_media
        .begin_profile_fetch(SharedMediaTab::Gifts);
    let extra = session.request(
        RequestPurpose::Media(MediaPurpose::GetProfileGifts { append: false }),
        Some(ChatId(6)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"receivedGifts","@extra":"{}","total_count":2,"next_offset":"","gifts":[{{"received_gift_id":"g1","text":{{"text":"Happy birthday!","entities":[]}},"is_saved":true,"date":1700000000,"sell_star_count":85,"gift":{{"@type":"sentGiftRegular","gift":{{"@type":"gift","id":"1","star_count":100}}}}}},{{"received_gift_id":"g2","date":1700000500,"gift":{{"@type":"sentGiftUpgraded","gift":{{"@type":"upgradedGift","title":"Plush Pepe","number":123}}}}}}]}}"#,
            extra.0
        ),
    );
    let gifts = tab(&session, SharedMediaTab::Gifts);
    assert_eq!(gifts.status, SharedMediaTabStatus::Ready);
    assert_eq!(gifts.items[0].label, "100 Stars — Happy birthday!");
    assert_eq!(gifts.items[1].label, "Plush Pepe #123");
    assert_eq!(gifts.items[0].action, SharedMediaAction::Gift);
    assert!(matches!(gifts.items[0].visual, SharedMediaVisual::Gift(_)));
    assert_eq!(gifts.items[0].date, 1_700_000_000);
    assert!(!gifts.can_load_more());
    assert_eq!(session.media.chat_media_counts[&6][&COUNT_KEY_GIFTS], 2);
    // The Gifts dialog's own list is untouched.
    assert!(session.payments.hub.gifts.is_empty());
}

#[test]
fn chat_page_stories_fill_the_stories_tab_and_failures_hide_it() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    open_user_gallery(&mut session, &seq, &sink);
    session
        .media
        .shared_media
        .begin_profile_fetch(SharedMediaTab::Stories);
    let extra = session.request(
        RequestPurpose::GetChatPostedToChatPageStories,
        Some(ChatId(6)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stories","@extra":"{}","total_count":3,"stories":[{{"@type":"story","id":5,"poster_chat_id":6,"date":1700000000,"content":{{"@type":"storyContentUnsupported"}}}},{{"@type":"story","id":4,"poster_chat_id":6,"date":1690000000,"content":{{"@type":"storyContentUnsupported"}}}}]}}"#,
            extra.0
        ),
    );
    let stories = tab(&session, SharedMediaTab::Stories);
    assert_eq!(stories.status, SharedMediaTabStatus::Ready);
    assert_eq!(stories.items.len(), 2);
    assert_eq!(stories.items[0].action, SharedMediaAction::Story(5));
    assert_eq!(stories.total_count, 3);
    assert!(stories.can_load_more());
    assert_eq!(session.media.chat_media_counts[&6][&COUNT_KEY_STORIES], 3);
    assert!(
        session
            .shared_media_visible_tabs()
            .contains(&SharedMediaTab::Stories)
    );

    // A failed refetch keeps the loaded stories; a failed first fetch
    // shows the error and hides the tab.
    session
        .media
        .shared_media
        .begin_profile_fetch(SharedMediaTab::Gifts);
    let extra = session.request(
        RequestPurpose::Media(MediaPurpose::GetProfileGifts { append: false }),
        Some(ChatId(6)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"USER_PRIVACY"}}"#,
            extra.0
        ),
    );
    let gifts = tab(&session, SharedMediaTab::Gifts);
    assert_eq!(gifts.status, SharedMediaTabStatus::Failed);
    assert!(!gifts.error.is_empty());
    assert_eq!(session.media.chat_media_counts[&6][&COUNT_KEY_GIFTS], 0);
    assert!(
        !session
            .shared_media_visible_tabs()
            .contains(&SharedMediaTab::Gifts)
    );
}

#[test]
fn round_videos_and_polls_get_their_own_row_labels() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    open_user_gallery(&mut session, &seq, &sink);
    for (tab_kind, messages, labels) in [
        (
            SharedMediaTab::Voice,
            r#"{"id":70,"chat_id":6,"is_outgoing":false,"content":{"@type":"messageVideoNote","video_note":{"@type":"videoNote","duration":6,"waveform":"","length":240},"is_viewed":false,"is_secret":false}},{"id":71,"chat_id":6,"is_outgoing":false,"content":{"@type":"messageVoiceNote","voice_note":{"@type":"voiceNote","duration":3,"waveform":"","mime_type":"audio/ogg"},"caption":{"@type":"formattedText","text":"","entities":[]},"is_listened":true}}"#,
            vec!["Video message", "Voice message"],
        ),
        (
            SharedMediaTab::Polls,
            r#"{"id":60,"chat_id":6,"is_outgoing":false,"content":{"@type":"messagePoll","poll":{"@type":"poll","id":9001,"question":{"@type":"formattedText","text":"Lunch?","entities":[]},"options":[{"@type":"pollOption","id":"a","text":{"@type":"formattedText","text":"Sushi","entities":[]},"voter_count":1,"vote_percentage":100,"is_chosen":false}],"total_voter_count":1,"is_anonymous":true,"allows_multiple_answers":false,"allows_revoting":true,"is_closed":false,"type":{"@type":"pollTypeRegular"}},"description":{"@type":"formattedText","text":"","entities":[]},"can_add_option":false}}"#,
            vec!["Lunch?"],
        ),
    ] {
        let generation = session.media.shared_media.begin_fetch(tab_kind);
        let extra = session.request(
            RequestPurpose::GetSharedMedia {
                tab: tab_kind,
                generation,
            },
            Some(ChatId(6)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":{},"next_from_message_id":0,"messages":[{messages}]}}"#,
                extra.0,
                labels.len()
            ),
        );
        let state = tab(&session, tab_kind);
        let got: Vec<&str> = state.items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(got, labels);
    }
}

#[test]
fn gallery_message_counts_include_polls_and_round_videos() {
    // Keys 7 and 8 are `EXTRA_COUNT_FILTERS`; they land next to the info
    // panel's counts and drive the Polls and Voice tabs.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    open_user_gallery(&mut session, &seq, &sink);
    // The entry the counts land in, as `fetch_chat_media_counts` opens it.
    session
        .media
        .chat_media_counts
        .insert(6, Default::default());
    for (key, count) in [(0u8, 5), (1, 0), (7, 2), (8, 4)] {
        let extra = session.request(
            RequestPurpose::Users(UsersPurpose::GetChatMessageCount { filter: key }),
            Some(ChatId(6)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"count","@extra":"{}","count":{count}}}"#,
                extra.0
            ),
        );
    }
    let tabs = session.shared_media_visible_tabs();
    assert_eq!(
        tabs,
        vec![
            SharedMediaTab::Media,
            SharedMediaTab::Polls,
            SharedMediaTab::Voice
        ]
    );
}

#[test]
fn story_thumbnails_are_queued_once_for_download() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    open_user_gallery(&mut session, &seq, &sink);
    let extra = session.request(
        RequestPurpose::GetChatPostedToChatPageStories,
        Some(ChatId(6)),
    );
    let file = |id: i32| {
        format!(
            r#"{{"@type":"file","id":{id},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"is_downloading_active":false,"is_downloading_completed":false}},"remote":{{"@type":"remoteFile","id":"r{id}","unique_id":"u{id}"}}}}"#
        )
    };
    let size = |id: i32, w: i32| {
        format!(
            r#"{{"@type":"photoSize","type":"x","photo":{},"width":{w},"height":{w},"progressive_sizes":[]}}"#,
            file(id)
        )
    };
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stories","@extra":"{}","total_count":1,"stories":[{{"@type":"story","id":5,"poster_chat_id":6,"date":1700000000,"content":{{"@type":"storyContentPhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{},{}]}}}}}}]}}"#,
            extra.0,
            size(61, 90),
            size(62, 800)
        ),
    );
    let stories = tab(&session, SharedMediaTab::Stories);
    // The size nearest a grid tile comes first.
    match &stories.items[0].visual {
        SharedMediaVisual::Story { thumbs, .. } => {
            assert_eq!(thumbs.first().map(|id| id.0), Some(61))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(session.take_profile_thumbs(), vec![FileId(61)]);
    assert!(session.take_profile_thumbs().is_empty());
}

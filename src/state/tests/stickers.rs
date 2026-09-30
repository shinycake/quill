//! State reducer tests: stickers.
use super::common::*;
use super::*;

#[test]
fn s8_sticker_backend_purpose_gated_dispatch() {
    let (mut with_purpose, sink) = session();
    let seq = AtomicU64::new(0);

    // Trending sets land under GetTrendingStickerSets.
    let extra = with_purpose.request(RequestPurpose::GetTrendingStickerSets, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"trendingStickerSets","total_count":1,"is_premium":true,"sets":[{{"@type":"stickerSetInfo","id":"77","title":"Demo","name":"DemoStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":false,"is_archived":false,"is_official":true,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":false,"size":2,"covers":[]}}],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(with_purpose.stickers.trending.len(), 1);
    assert_eq!(with_purpose.stickers.trending[0].id, 77);
    assert!(with_purpose.stickers.trending_is_premium);

    // A stray trendingStickerSets (no matching purpose) is ignored.
    let (mut without_purpose, sink2) = session();
    let seq2 = AtomicU64::new(0);
    apply_json(
        &mut without_purpose,
        &seq2,
        &sink2,
        r#"{"@type":"trendingStickerSets","total_count":1,"is_premium":false,"sets":[]}"#,
    );
    assert!(without_purpose.stickers.trending.is_empty());

    // Favorites arrive as the bare `stickers` type under
    // GetFavoriteStickers.
    let extra = with_purpose.request(RequestPurpose::GetFavoriteStickers, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stickers","stickers":[{{"@type":"sticker","id":"9001","set_id":"77","width":512,"height":512,"emoji":"😀","format":{{"@type":"stickerFormatWebp"}},"full_type":{{"@type":"stickerFullTypeRegular","premium_animation":null}},"thumbnail":null,"sticker":null}}],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(with_purpose.stickers.favorites.len(), 1);
    assert_eq!(with_purpose.stickers.favorites[0].emoji, "😀");

    // searchStickerSets answers with `stickerSets` under
    // SearchStickerSets (not the installed-sets slot).
    let extra = with_purpose.request(RequestPurpose::SearchStickerSets, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"stickerSets","total_count":1,"sets":[{{"@type":"stickerSetInfo","id":"78","title":"Cats","name":"CatsStickers","thumbnail":null,"thumbnail_outline":null,"is_owned":false,"is_installed":false,"is_archived":false,"is_official":false,"sticker_type":{{"@type":"stickerTypeRegular"}},"needs_repainting":false,"is_allowed_as_chat_emoji_status":false,"is_viewed":true,"size":5,"covers":[]}}],"@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(with_purpose.stickers.found_sets.len(), 1);
    assert_eq!(with_purpose.stickers.found_sets[0].id, 78);
    assert!(with_purpose.stickers.sets.is_empty());

    // A removeFavoriteSticker `ok` clears the favorites cache.
    let extra = with_purpose.request(RequestPurpose::RemoveFavoriteSticker, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(with_purpose.stickers.favorites.is_empty());

    // A changeStickerSet `ok` drops the installed-sets cache so the
    // panel refetches the authoritative list.
    with_purpose.stickers.sets = vec![with_purpose.stickers.trending[0].clone()];
    let extra = with_purpose.request(RequestPurpose::ChangeStickerSet, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(with_purpose.stickers.sets.is_empty());
}

#[test]
fn s9_gif_backend_purpose_gated_dispatch() {
    let (mut with_purpose, sink) = session();
    let seq = AtomicU64::new(0);

    // First search page lands in `GifPanel` under GetGifSearchResults
    // (not the composer's inline_query slot).
    let extra = with_purpose.request(
        RequestPurpose::GetGifSearchResults { first_page: true },
        None,
    );
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","inline_query_id":7,"button":null,"results":[{{"@type":"inlineQueryResultAnimation","id":"g1","title":"cat","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"c.gif","mime_type":"image/gif","animation":{{"@type":"file","id":101,"size":12,"expected_size":12,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":false,"uploaded_size":0}}}},"thumbnail":null,"has_stickers":false}}}}],"next_offset":"50","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert_eq!(with_purpose.gifs.search_results.len(), 1);
    assert_eq!(with_purpose.gifs.search_results[0].file_id.0, 101);
    assert_eq!(with_purpose.gifs.search_next_offset, "50");
    assert!(with_purpose.inline_query.is_none());

    // Second page appends new entries (deduped by file id) and
    // refreshes the offset.
    let extra = with_purpose.request(
        RequestPurpose::GetGifSearchResults { first_page: false },
        None,
    );
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"inlineQueryResults","inline_query_id":7,"button":null,"results":[{{"@type":"inlineQueryResultAnimation","id":"g1","title":"cat","animation":{{"@type":"animation","duration":2,"width":240,"height":140,"file_name":"c.gif","mime_type":"image/gif","animation":{{"@type":"file","id":101}},"thumbnail":null,"has_stickers":false}}}},{{"@type":"inlineQueryResultAnimation","id":"g2","title":"dog","animation":{{"@type":"animation","duration":3,"width":200,"height":200,"file_name":"d.gif","mime_type":"image/gif","animation":{{"@type":"file","id":102}},"thumbnail":null,"has_stickers":false}}}}],"next_offset":"","@extra":"{}"}}"#,
            extra.0
        ),
    );
    // g1 (file 101) was already on page one — deduped; g2 (file 102)
    // appends; the offset advances to "" (exhausted).
    assert_eq!(with_purpose.gifs.search_results.len(), 2);
    assert_eq!(with_purpose.gifs.search_results[1].file_id.0, 102);
    assert_eq!(with_purpose.gifs.search_next_offset, "");

    // A stray inlineQueryResults (no matching purpose) is ignored.
    let (mut without_purpose, sink2) = session();
    let seq2 = AtomicU64::new(0);
    apply_json(
        &mut without_purpose,
        &seq2,
        &sink2,
        r#"{"@type":"inlineQueryResults","inline_query_id":7,"button":null,"results":[],"next_offset":"9"}"#,
    );
    assert!(without_purpose.gifs.search_results.is_empty());

    // An addSavedAnimation `ok` clears the saved-GIF cache so it
    // refetches the server-confirmed list.
    with_purpose.gifs.animations = with_purpose.gifs.search_results.clone();
    let extra = with_purpose.request(RequestPurpose::AddSavedAnimation, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(with_purpose.gifs.animations.is_empty());

    // A removeSavedAnimation `ok` invalidates the same way.
    with_purpose.gifs.animations = with_purpose.gifs.search_results.clone();
    let extra = with_purpose.request(RequestPurpose::RemoveSavedAnimation, None);
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(with_purpose.gifs.animations.is_empty());

    // updateAnimationSearchParameters stores provider + emojis.
    apply_json(
        &mut with_purpose,
        &seq,
        &sink,
        r#"{"@type":"updateAnimationSearchParameters","provider":"Tenor","emojis":["😀","🎉"]}"#,
    );
    assert_eq!(with_purpose.gifs.search_provider, "Tenor");
    assert_eq!(with_purpose.gifs.provider_emojis, vec!["😀", "🎉"]);
}

#[test]
fn sound_list_refetch_prunes_file_ids() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.sound_file_ids.insert(91, 7); // dropped from the list
    session.sound_file_ids.insert(92, 8); // still saved
    let extra = session.request(RequestPurpose::GetSavedNotificationSounds, None);
    let sound_file = r#"{"@type":"file","id":92,"size":12,"expected_size":12,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":12}}"#;
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"notificationSounds","notification_sounds":[{{"@type":"notificationSound","id":8,"duration":2,"date":0,"title":"Chime","data":"","sound":{}}}],"@extra":"{}"}}"#,
            sound_file, extra.0
        ),
    );
    assert!(session.saved_sounds_loaded);
    assert!(!session.sound_file_ids.contains_key(&91));
    assert_eq!(session.sound_file_ids.get(&92), Some(&8));
}

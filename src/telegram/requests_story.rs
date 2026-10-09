//! Phase 9.7 story-page request builders: story albums (`storyAlbum`
//! family), chat archive, and chat-page pinned stories (TDLib 1.8.67).
//! Split out of `requests.rs` per the file-size directive — `requests.rs`
//! keeps only the irreducible core.

use crate::ids::{ChatId, RequestId};
use serde_json::json;

/// Phase 9.7: `getChatStoryAlbums` (TDLib 1.8.67, `schema/td_api.tl:13850`)
/// — `getChatStoryAlbums chat_id:int53 = StoryAlbums;` ("Returns the list
/// of story albums owned by the given chat"). Response is `storyAlbums`.
pub fn get_chat_story_albums(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatStoryAlbums",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0
    })
    .to_string()
}

/// Phase 9.7: `getStoryAlbumStories` (TDLib 1.8.67, `schema/td_api.tl:13857`)
/// — `getStoryAlbumStories chat_id:int53 story_album_id:int32 offset:int32
/// limit:int32 = Stories;` ("Returns the list of stories added to the
/// given story album. For optimal performance, the number of returned
/// stories is chosen by TDLib"). `offset` 0 starts from the first album
/// story. Response is `stories`.
pub fn get_story_album_stories(
    extra: RequestId,
    chat_id: ChatId,
    story_album_id: i32,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getStoryAlbumStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_album_id": story_album_id,
        "offset": offset,
        "limit": limit
    })
    .to_string()
}

/// Phase 9.7: `createStoryAlbum` (TDLib 1.8.67, `schema/td_api.tl:13863`)
/// — `createStoryAlbum story_poster_chat_id:int53 name:string
/// story_ids:vector<int32> = StoryAlbum;` ("Creates an album of stories;
/// requires can_edit_stories administrator right for supergroup and
/// channel chats"). Name is 1-12 characters per the schema comment.
/// Response is the new `storyAlbum`.
pub fn create_story_album(
    extra: RequestId,
    story_poster_chat_id: ChatId,
    name: &str,
    story_ids: &[i32],
) -> String {
    json!({
        "@type": "createStoryAlbum",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": story_poster_chat_id.0,
        "name": name,
        "story_ids": story_ids
    })
    .to_string()
}

/// Phase 9.7: `reorderStoryAlbums` (TDLib 1.8.67, `schema/td_api.tl:13868`)
/// — `reorderStoryAlbums chat_id:int53 story_album_ids:vector<int32> =
/// Ok;` ("Changes order of story albums."). Response is `ok`.
pub fn reorder_story_albums(extra: RequestId, chat_id: ChatId, story_album_ids: &[i32]) -> String {
    json!({
        "@type": "reorderStoryAlbums",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_album_ids": story_album_ids
    })
    .to_string()
}

/// Phase 9.7: `deleteStoryAlbum` (TDLib 1.8.67, `schema/td_api.tl:13873`)
/// — `deleteStoryAlbum chat_id:int53 story_album_id:int32 = Ok;`
/// ("Deletes a story album."). Response is `ok`.
pub fn delete_story_album(extra: RequestId, chat_id: ChatId, story_album_id: i32) -> String {
    json!({
        "@type": "deleteStoryAlbum",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_album_id": story_album_id
    })
    .to_string()
}

/// Phase 9.7: `setStoryAlbumName` (TDLib 1.8.67, `schema/td_api.tl:13879`)
/// — `setStoryAlbumName chat_id:int53 story_album_id:int32 name:string =
/// StoryAlbum;` ("Changes name of an album of stories. ... Returns the
/// changed album"). Name is 1-12 characters per the schema comment.
pub fn set_story_album_name(
    extra: RequestId,
    chat_id: ChatId,
    story_album_id: i32,
    name: &str,
) -> String {
    json!({
        "@type": "setStoryAlbumName",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_album_id": story_album_id,
        "name": name
    })
    .to_string()
}

/// Phase 9.7: `addStoryAlbumStories` (TDLib 1.8.67, `schema/td_api.tl:13887`)
/// — `addStoryAlbumStories chat_id:int53 story_album_id:int32
/// story_ids:vector<int32> = StoryAlbum;` ("Adds stories to the beginning
/// of a previously created story album. ... Returns the changed album").
pub fn add_story_album_stories(
    extra: RequestId,
    chat_id: ChatId,
    story_album_id: i32,
    story_ids: &[i32],
) -> String {
    json!({
        "@type": "addStoryAlbumStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_album_id": story_album_id,
        "story_ids": story_ids
    })
    .to_string()
}

/// Phase 9.7: `removeStoryAlbumStories` (TDLib 1.8.67,
/// `schema/td_api.tl:13894`) — `removeStoryAlbumStories chat_id:int53
/// story_album_id:int32 story_ids:vector<int32> = StoryAlbum;` ("Removes
/// stories from an album. ... Returns the changed album").
pub fn remove_story_album_stories(
    extra: RequestId,
    chat_id: ChatId,
    story_album_id: i32,
    story_ids: &[i32],
) -> String {
    json!({
        "@type": "removeStoryAlbumStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_album_id": story_album_id,
        "story_ids": story_ids
    })
    .to_string()
}

/// Phase 9.7: `reorderStoryAlbumStories` (TDLib 1.8.67,
/// `schema/td_api.tl:13901`) — `reorderStoryAlbumStories chat_id:int53
/// story_album_id:int32 story_ids:vector<int32> = StoryAlbum;` ("Changes
/// order of stories in an album. ... Returns the changed album"). The
/// listed stories move to the beginning; the rest keep their order.
pub fn reorder_story_album_stories(
    extra: RequestId,
    chat_id: ChatId,
    story_album_id: i32,
    story_ids: &[i32],
) -> String {
    json!({
        "@type": "reorderStoryAlbumStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_album_id": story_album_id,
        "story_ids": story_ids
    })
    .to_string()
}

/// Phase 9.7: `getChatArchivedStories` (TDLib 1.8.67,
/// `schema/td_api.tl:13784`) — `getChatArchivedStories chat_id:int53
/// from_story_id:int32 limit:int32 = Stories;` ("The maximum number of
/// stories to be returned. For optimal performance, the number of
/// returned stories is chosen by TDLib and can be smaller than the
/// specified limit"). `from_story_id` 0 starts from the newest archived
/// story; later pages pass the smallest id loaded so far. Response is
/// `stories`.
pub fn get_chat_archived_stories(
    extra: RequestId,
    chat_id: ChatId,
    from_story_id: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getChatArchivedStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "from_story_id": from_story_id,
        "limit": limit
    })
    .to_string()
}

/// Phase 9.7: `getChatPostedToChatPageStories` (TDLib 1.8.67,
/// `schema/td_api.tl:13776`) — `getChatPostedToChatPageStories
/// chat_id:int53 from_story_id:int32 limit:int32 = Stories;` Response is
/// `stories`; with `from_story_id == 0` it also carries
/// `pinned_story_ids` (schema comment at `td_api.tl:6747`).
pub fn get_chat_posted_to_chat_page_stories(
    extra: RequestId,
    chat_id: ChatId,
    from_story_id: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getChatPostedToChatPageStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "from_story_id": from_story_id,
        "limit": limit
    })
    .to_string()
}

/// Phase 9.7: `setChatPinnedStories` (TDLib 1.8.67, `schema/td_api.tl:13789`)
/// — `setChatPinnedStories chat_id:int53 story_ids:vector<int32> = Ok;`
/// ("Changes the list of pinned stories on a chat page; requires
/// can_edit_stories administrator right in the chat. ... All stories must
/// be posted to the chat page first. There can be up to
/// getOption(\"pinned_story_count_max\") pinned stories on a chat page").
/// Response is `ok`.
pub fn set_chat_pinned_stories(extra: RequestId, chat_id: ChatId, story_ids: &[i32]) -> String {
    json!({
        "@type": "setChatPinnedStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_ids": story_ids
    })
    .to_string()
}

/// B14: `getCloseFriends = Users;` (schema `td_api.tl:14939`, "Returns all
/// close friends of the current user").
pub fn get_close_friends(extra: RequestId) -> String {
    json!({"@type": "getCloseFriends", "@extra": extra.as_extra()}).to_string()
}

/// B14: `setCloseFriends user_ids:vector<int53> = Ok;` (schema
/// `td_api.tl:14936`) — replaces the whole list.
pub fn set_close_friends(extra: RequestId, user_ids: &[i64]) -> String {
    json!({
        "@type": "setCloseFriends",
        "@extra": extra.as_extra(),
        "user_ids": user_ids
    })
    .to_string()
}

/// B14: `setChatActiveStoriesList chat_id:int53 story_list:StoryList = Ok;`
/// (schema `td_api.tl:14159`) — `archive` moves the peer's stories to the
/// hidden list (tdesktop "Hide stories"), otherwise back to the main one.
pub fn set_chat_active_stories_list(extra: RequestId, chat_id: ChatId, archive: bool) -> String {
    json!({
        "@type": "setChatActiveStoriesList",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "story_list": {
            "@type": if archive { "storyListArchive" } else { "storyListMain" }
        }
    })
    .to_string()
}

/// B14: `toggleStoryIsPostedToChatPage story_poster_chat_id:int53
/// story_id:int32 is_posted_to_chat_page:Bool = Ok;` (schema
/// `td_api.tl:14143`).
pub fn toggle_story_is_posted_to_chat_page(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    posted: bool,
) -> String {
    json!({
        "@type": "toggleStoryIsPostedToChatPage",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "is_posted_to_chat_page": posted
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b14_story_request_shapes() {
        let v: serde_json::Value = serde_json::from_str(&get_close_friends(RequestId(1))).unwrap();
        assert_eq!(v["@type"], "getCloseFriends");
        let v: serde_json::Value =
            serde_json::from_str(&set_close_friends(RequestId(2), &[5, 9])).unwrap();
        assert_eq!(v["@type"], "setCloseFriends");
        assert_eq!(v["user_ids"], serde_json::json!([5, 9]));
        let v: serde_json::Value = serde_json::from_str(&set_chat_active_stories_list(
            RequestId(3),
            ChatId(11),
            true,
        ))
        .unwrap();
        assert_eq!(v["story_list"]["@type"], "storyListArchive");
        let v: serde_json::Value = serde_json::from_str(&set_chat_active_stories_list(
            RequestId(3),
            ChatId(11),
            false,
        ))
        .unwrap();
        assert_eq!(v["story_list"]["@type"], "storyListMain");
        let v: serde_json::Value = serde_json::from_str(&toggle_story_is_posted_to_chat_page(
            RequestId(4),
            ChatId(11),
            7,
            true,
        ))
        .unwrap();
        assert_eq!(v["@type"], "toggleStoryIsPostedToChatPage");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 7);
        assert_eq!(v["is_posted_to_chat_page"], true);
    }

    #[test]
    fn story_album_and_archive_request_shapes() {
        // Phase 9.7: every album/archive/pinned constructor pins its schema
        // shape (td_api.tl line numbers in the doc comments above).
        let v: serde_json::Value =
            serde_json::from_str(&get_chat_story_albums(RequestId(80), ChatId(11))).unwrap();
        assert_eq!(v["@type"], "getChatStoryAlbums");
        assert_eq!(v["chat_id"], 11);

        let v: serde_json::Value = serde_json::from_str(&get_story_album_stories(
            RequestId(81),
            ChatId(11),
            3,
            0,
            50,
        ))
        .unwrap();
        assert_eq!(v["@type"], "getStoryAlbumStories");
        assert_eq!(v["story_album_id"], 3);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], 50);

        let v: serde_json::Value = serde_json::from_str(&create_story_album(
            RequestId(82),
            ChatId(11),
            "Trip",
            &[5, 6],
        ))
        .unwrap();
        assert_eq!(v["@type"], "createStoryAlbum");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["name"], "Trip");
        assert_eq!(v["story_ids"], serde_json::json!([5, 6]));

        let v: serde_json::Value =
            serde_json::from_str(&reorder_story_albums(RequestId(83), ChatId(11), &[3, 2]))
                .unwrap();
        assert_eq!(v["@type"], "reorderStoryAlbums");
        assert_eq!(v["story_album_ids"], serde_json::json!([3, 2]));

        let v: serde_json::Value =
            serde_json::from_str(&delete_story_album(RequestId(84), ChatId(11), 3)).unwrap();
        assert_eq!(v["@type"], "deleteStoryAlbum");
        assert_eq!(v["story_album_id"], 3);

        let v: serde_json::Value =
            serde_json::from_str(&set_story_album_name(RequestId(85), ChatId(11), 3, "Trips"))
                .unwrap();
        assert_eq!(v["@type"], "setStoryAlbumName");
        assert_eq!(v["name"], "Trips");

        let v: serde_json::Value =
            serde_json::from_str(&add_story_album_stories(RequestId(86), ChatId(11), 3, &[7]))
                .unwrap();
        assert_eq!(v["@type"], "addStoryAlbumStories");
        assert_eq!(v["story_ids"], serde_json::json!([7]));

        let v: serde_json::Value = serde_json::from_str(&remove_story_album_stories(
            RequestId(87),
            ChatId(11),
            3,
            &[6],
        ))
        .unwrap();
        assert_eq!(v["@type"], "removeStoryAlbumStories");
        assert_eq!(v["story_ids"], serde_json::json!([6]));

        let v: serde_json::Value = serde_json::from_str(&reorder_story_album_stories(
            RequestId(88),
            ChatId(11),
            3,
            &[7, 5],
        ))
        .unwrap();
        assert_eq!(v["@type"], "reorderStoryAlbumStories");
        assert_eq!(v["story_ids"], serde_json::json!([7, 5]));

        let v: serde_json::Value =
            serde_json::from_str(&get_chat_archived_stories(RequestId(89), ChatId(11), 0, 50))
                .unwrap();
        assert_eq!(v["@type"], "getChatArchivedStories");
        assert_eq!(v["from_story_id"], 0);

        let v: serde_json::Value = serde_json::from_str(&get_chat_posted_to_chat_page_stories(
            RequestId(90),
            ChatId(11),
            0,
            50,
        ))
        .unwrap();
        assert_eq!(v["@type"], "getChatPostedToChatPageStories");
        assert_eq!(v["from_story_id"], 0);

        let v: serde_json::Value =
            serde_json::from_str(&set_chat_pinned_stories(RequestId(91), ChatId(11), &[5]))
                .unwrap();
        assert_eq!(v["@type"], "setChatPinnedStories");
        assert_eq!(v["story_ids"], serde_json::json!([5]));
    }
}

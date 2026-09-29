//! Phase 9.7 story-page state: per-chat album lists, chat-page stories,
//! archive pagination, and the honest mutation-op state (`StoryPageOp`).
//! Split out of `state.rs` per the file-size directive — `state.rs` keeps
//! the `Session` fields, the `RequestPurpose` variants, the reducer match
//! arms, and the `Session` methods.

use crate::state::RequestPurpose;

/// Phase 9.7: one chat's `getChatPostedToChatPageStories` state — the
/// loaded story ids (paginated, accumulated), the pinned story ids (from
/// the first page's `pinned_story_ids`, or from a `setChatPinnedStories`
/// `ok`), and the server-side total.
#[derive(Debug, Clone, Default)]
pub struct ChatPageStories {
    pub story_ids: Vec<i32>,
    pub pinned_story_ids: Vec<i32>,
    pub total_count: i32,
}

/// Phase 9.7: one chat's `getChatArchivedStories` state — accumulated
/// story ids, the server total, and the cursor for the next page (the
/// smallest loaded story id; `None` before the first page).
#[derive(Debug, Clone, Default)]
pub struct ArchivedStories {
    pub story_ids: Vec<i32>,
    pub total_count: i32,
    pub next_from_story_id: Option<i32>,
}

/// Phase 9.7: honest album/pin mutation states on the story page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryPageOpState {
    /// A read is in flight (`getChatStoryAlbums`,
    /// `getStoryAlbumStories`, `getChatArchivedStories`,
    /// `getChatPostedToChatPageStories`).
    Checking,
    /// A mutation is in flight (create/reorder/delete/rename album,
    /// add/remove/reorder album stories, set pinned stories).
    Sending,
    Succeeded,
    Failed(String),
}

/// Phase 9.7: the latest story-page mutation (`createStoryAlbum`,
/// `setChatPinnedStories`, …) — `label` names the operation, `state` the
/// honest TDLib round-trip state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryPageOp {
    pub label: String,
    pub state: StoryPageOpState,
}

/// Phase 9.7: the status-line label for a story-page request purpose —
/// shared by the driver's `begin_story_page_op` and the reducer's
/// succeed/fail/clear so both sides name the same op.
pub fn story_page_op_label(purpose: RequestPurpose) -> String {
    match purpose {
        RequestPurpose::GetChatStoryAlbums => "loading albums".to_string(),
        RequestPurpose::GetStoryAlbumStories => "loading album stories".to_string(),
        RequestPurpose::CreateStoryAlbum => "creating album".to_string(),
        RequestPurpose::ReorderStoryAlbums => "reordering albums".to_string(),
        RequestPurpose::DeleteStoryAlbum => "deleting album".to_string(),
        RequestPurpose::SetStoryAlbumName => "renaming album".to_string(),
        RequestPurpose::AddStoryAlbumStories => "adding stories to album".to_string(),
        RequestPurpose::RemoveStoryAlbumStories => "removing stories from album".to_string(),
        RequestPurpose::ReorderStoryAlbumStories => "reordering album stories".to_string(),
        RequestPurpose::GetChatArchivedStories => "loading archived stories".to_string(),
        RequestPurpose::GetChatPostedToChatPageStories => "loading chat page stories".to_string(),
        RequestPurpose::SetChatPinnedStories => "pinning stories".to_string(),
        _ => "story request".to_string(),
    }
}

/// Phase 9.7: parse a comma/whitespace-separated story-id list from a
/// text input (the story page's "add stories" / "new album" inputs).
/// Non-numeric tokens are dropped; ids dedupe, order kept.
pub fn parse_story_id_list(text: &str) -> Vec<i32> {
    let mut ids = Vec::new();
    for token in text.split(|c: char| c == ',' || c.is_whitespace()) {
        if let Ok(id) = token.parse::<i32>()
            && id > 0
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticSink, MemorySink};
    use crate::ids::{AccountKey, ChatId};
    use crate::state::Session;
    use crate::telegram::client::copy_and_parse;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    fn session() -> (Session, Arc<MemorySink>) {
        let sink = Arc::new(MemorySink::new());
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        (Session::new(AccountKey::primary(), dyn_sink), sink)
    }

    fn apply_json(session: &mut Session, seq: &AtomicU64, sink: &Arc<MemorySink>, json: &str) {
        let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
        let owned = copy_and_parse(json, seq, &dyn_sink).unwrap();
        session.apply(owned);
    }

    #[test]
    fn story_page_albums_replace_and_stray_ignored() {
        // Phase 9.7: a `storyAlbums` answer with the matching
        // `GetChatStoryAlbums` purpose replaces the chat's album list; a
        // stray `storyAlbums` (no pending request) never flips the UI.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let albums = |extra: &str| {
            format!(
                r#"{{"@type":"storyAlbums","@extra":"{extra}","albums":[{{"@type":"storyAlbum","id":1,"name":"Travel"}},{{"@type":"storyAlbum","id":2,"name":"Food"}}]}}"#
            )
        };
        apply_json(&mut session, &seq, &sink, &albums("no-such-extra"));
        assert!(!session.story_albums.contains_key(&11));
        let extra = session.request(RequestPurpose::GetChatStoryAlbums, Some(ChatId(11)));
        apply_json(&mut session, &seq, &sink, &albums(&extra.0.to_string()));
        let names: Vec<&str> = session.story_albums[&11]
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["Travel", "Food"]);
        assert!(session.story_page_op.is_none());
    }

    #[test]
    fn story_page_album_stories_accumulate_and_dedupe() {
        // Phase 9.7: `getStoryAlbumStories` pages accumulate per album and
        // dedupe; the loading op clears when a page lands.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let stories = |extra: &str, ids: &[i32]| {
            let list: Vec<String> = ids
                .iter()
                .map(|id| format!(r#"{{"@type":"story","id":{id},"poster_chat_id":11}}"#))
                .collect();
            format!(
                r#"{{"@type":"stories","@extra":"{extra}","total_count":3,"stories":[{}]}}"#,
                list.join(",")
            )
        };
        let fetch = |session: &mut Session, ids: &[i32]| {
            let extra = session.request_for_story_album(
                RequestPurpose::GetStoryAlbumStories,
                ChatId(11),
                None,
                Some(7),
            );
            session
                .begin_story_page_check(story_page_op_label(RequestPurpose::GetStoryAlbumStories));
            (extra, ids.to_vec())
        };
        let (extra, _) = fetch(&mut session, &[301, 302]);
        assert!(matches!(
            session.story_page_op.as_ref().map(|op| &op.state),
            Some(StoryPageOpState::Checking)
        ));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &stories(&extra.0.to_string(), &[301, 302]),
        );
        let (extra, _) = fetch(&mut session, &[302, 303]);
        apply_json(
            &mut session,
            &seq,
            &sink,
            &stories(&extra.0.to_string(), &[302, 303]),
        );
        assert_eq!(session.story_album_stories[&(11, 7)], vec![301, 302, 303]);
        assert!(session.story_page_op.is_none());
    }

    #[test]
    fn story_page_chat_page_pinned_first_page_only() {
        // Phase 9.7: the first chat-page payload sets `pinned_story_ids`
        // (schema 1.8.67:6747 — populated only by the first page); a later
        // page without pins preserves them and appends ids.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let page = |extra: &str, pinned: Option<&str>, ids: &[i32]| {
            let list: Vec<String> = ids
                .iter()
                .map(|id| format!(r#"{{"@type":"story","id":{id},"poster_chat_id":11}}"#))
                .collect();
            let pins = pinned
                .map(|p| format!(r#""pinned_story_ids":[{p}],"#))
                .unwrap_or_default();
            format!(
                r#"{{"@type":"stories","@extra":"{extra}","total_count":3,{pins}"stories":[{}]}}"#,
                list.join(",")
            )
        };
        let extra = session.request(
            RequestPurpose::GetChatPostedToChatPageStories,
            Some(ChatId(11)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &page(&extra.0.to_string(), Some("301"), &[301, 302]),
        );
        let extra = session.request(
            RequestPurpose::GetChatPostedToChatPageStories,
            Some(ChatId(11)),
        );
        apply_json(
            &mut session,
            &seq,
            &sink,
            &page(&extra.0.to_string(), None, &[303]),
        );
        let state = &session.chat_page_stories[&11];
        assert_eq!(state.pinned_story_ids, vec![301]);
        assert_eq!(state.story_ids, vec![301, 302, 303]);
    }

    #[test]
    fn story_page_archive_next_cursor() {
        // Phase 9.7: the archive accumulates ids in server order (newest
        // first) and the next page starts from the smallest loaded id
        // (`from_story_id` returns stories below it).
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let extra = session.request(RequestPurpose::GetChatArchivedStories, Some(ChatId(11)));
        session.begin_story_page_check(story_page_op_label(RequestPurpose::GetChatArchivedStories));
        assert!(matches!(
            session.story_page_op.as_ref().map(|op| &op.state),
            Some(StoryPageOpState::Checking)
        ));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"stories","@extra":"{}","total_count":5,"stories":[{{"@type":"story","id":202,"poster_chat_id":11}},{{"@type":"story","id":201,"poster_chat_id":11}}]}}"#,
                extra.0
            ),
        );
        let archived = &session.archived_stories[&11];
        assert_eq!(archived.story_ids, vec![202, 201]);
        assert_eq!(archived.next_from_story_id, Some(201));
        assert!(session.story_page_op.is_none());
    }

    #[test]
    fn story_page_mutation_op_lifecycle() {
        // Phase 9.7: a mutation shows Sending while in flight, Succeeded on
        // the `storyAlbum` answer, Failed on a TDLib error.
        let (mut session, sink) = session();
        let seq = AtomicU64::new(1);
        let extra = session.request(RequestPurpose::CreateStoryAlbum, Some(ChatId(11)));
        session.begin_story_page_op(story_page_op_label(RequestPurpose::CreateStoryAlbum));
        assert!(matches!(
            session.story_page_op.as_ref().map(|op| &op.state),
            Some(StoryPageOpState::Sending)
        ));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"storyAlbum","@extra":"{}","id":9,"name":"New"}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.story_page_op.as_ref().map(|op| &op.state),
            Some(StoryPageOpState::Succeeded)
        ));
        assert_eq!(session.story_albums[&11][0].name, "New");
        let extra = session.request(RequestPurpose::DeleteStoryAlbum, Some(ChatId(11)));
        session.begin_story_page_op(story_page_op_label(RequestPurpose::DeleteStoryAlbum));
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":400,"message":"ALBUM_NOT_FOUND"}}"#,
                extra.0
            ),
        );
        assert!(matches!(
            session.story_page_op.as_ref().map(|op| &op.state),
            Some(StoryPageOpState::Failed(_))
        ));
    }

    #[test]
    fn parse_story_id_list_cases() {
        // Phase 9.7: the story-page id inputs — commas/whitespace split,
        // junk dropped, order kept, deduped.
        assert_eq!(parse_story_id_list("301, 302,303"), vec![301, 302, 303]);
        assert_eq!(parse_story_id_list("  7\n8\t9 "), vec![7, 8, 9]);
        assert_eq!(parse_story_id_list("1,abc,2,,0,-3,1"), vec![1, 2]);
        assert!(parse_story_id_list("").is_empty());
    }
}

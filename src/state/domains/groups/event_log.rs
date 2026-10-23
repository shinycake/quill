//! Applies the `getChatEventLog` answer: a first page replaces the cache,
//! an older page appends, a poll answer merges the newest events.
use crate::state::*;
use crate::telegram::envelope::ParsedChatEvent;

impl Session {
    /// A first page (cursor 0) replaces the cache; an older page appends,
    /// deduped by event id, keeping reverse-chronological order
    /// (decreasing event id, schema 1.8.67 line 15252). A full page sets
    /// `has_more`; a short page exhausts the log. A poll answer joins the
    /// loaded page and leaves `has_more` alone.
    pub(super) fn apply_chat_events(
        &mut self,
        events: Vec<ParsedChatEvent>,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::Groups(GroupsPurpose::PollChatEventLog)
            && let Some(chat_id) = pending.chat_id
        {
            // The live poll: new events join the loaded page; with
            // no loaded page (a filter change cleared it) the
            // answer is stale and dropped.
            if let Some(ChatEventLogFetch::Loaded(page)) = self.groups.event_logs.get(&chat_id.0) {
                let mut merged = page.events.clone();
                crate::state::paging::append_new_by_id(&mut merged, events, |event| event.id);
                merged.sort_by_key(|event| std::cmp::Reverse(event.id));
                let has_more = page.has_more;
                self.groups.event_logs.insert(
                    chat_id.0,
                    ChatEventLogFetch::Loaded(ChatEventLogPage {
                        events: merged,
                        has_more,
                    }),
                );
            }
        } else if let Some(pending) = pending
            && let RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id }) =
                pending.purpose
            && let Some(chat_id) = pending.chat_id
        {
            let has_more = events.len() as i32 >= CHAT_EVENT_LOG_PAGE_SIZE;
            let mut merged = match (from_event_id, self.groups.event_logs.get(&chat_id.0)) {
                (0, _) => events,
                (_, Some(ChatEventLogFetch::Loaded(page))) => {
                    let mut merged = page.events.clone();
                    crate::state::paging::append_new_by_id(&mut merged, events, |event| event.id);
                    merged
                }
                _ => events,
            };
            merged.sort_by_key(|event| std::cmp::Reverse(event.id));
            self.groups.event_logs.insert(
                chat_id.0,
                ChatEventLogFetch::Loaded(ChatEventLogPage {
                    events: merged,
                    has_more,
                }),
            );
        }
    }
}

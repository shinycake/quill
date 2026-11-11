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
        // Only the requests the log waits for: a page or poll asked under
        // an earlier query or filter is dropped.
        let Some((pending, chat_id)) = pending.and_then(|p| Some((p, p.chat_id?))) else {
            return;
        };
        let Some(ask) = self.groups.take_event_log_wait(chat_id.0, pending.id) else {
            return;
        };
        match ask {
            EventLogAsk::Poll => {
                // The live poll: new events join the loaded page.
                if let Some(ChatEventLogFetch::Loaded(loaded)) =
                    self.groups.event_log_fetch_mut(chat_id.0)
                {
                    let mut merged = loaded.page.events.clone();
                    crate::state::paging::append_new_by_id(&mut merged, events, |event| event.id);
                    merged.sort_by_key(|event| std::cmp::Reverse(event.id));
                    loaded.page.events = merged;
                }
            }
            EventLogAsk::Older => {
                if let Some(ChatEventLogFetch::Loaded(loaded)) =
                    self.groups.event_log_fetch_mut(chat_id.0)
                {
                    let has_more = events.len() as i32 >= CHAT_EVENT_LOG_PAGE_SIZE;
                    let mut merged = loaded.page.events.clone();
                    crate::state::paging::append_new_by_id(&mut merged, events, |event| event.id);
                    merged.sort_by_key(|event| std::cmp::Reverse(event.id));
                    loaded.page = ChatEventLogPage {
                        events: merged,
                        has_more,
                    };
                }
            }
            EventLogAsk::First => {
                let has_more = events.len() as i32 >= CHAT_EVENT_LOG_PAGE_SIZE;
                let mut events = events;
                events.sort_by_key(|event| std::cmp::Reverse(event.id));
                self.groups.set_event_log_fetch(
                    chat_id.0,
                    ChatEventLogFetch::Loaded(LoadedEventLog::new(ChatEventLogPage {
                        events,
                        has_more,
                    })),
                );
            }
        }
    }
}

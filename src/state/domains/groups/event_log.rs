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
        // Only the request the log's stage holds: a page or poll asked under
        // an earlier query or filter finds another stage and is dropped.
        let Some(pending) = pending else {
            return;
        };
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let (chat, id) = (chat_id.0, pending.id);
        match pending.purpose {
            RequestPurpose::Groups(GroupsPurpose::PollChatEventLog) => {
                // New events join the loaded page; with no loaded page (a
                // filter change cleared it) the answer is stale.
                if let Some(page) = self.groups.loaded_event_log_mut(chat)
                    && page.poll == (LogPoll::Polling { request: id })
                {
                    page.poll = LogPoll::Idle;
                    merge_events(&mut page.events, events);
                }
            }
            RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id: 0 }) => {
                if let Some(ChatEventLogFetch::Loading { request }) =
                    self.groups.event_log_fetch(chat)
                    && *request == id
                {
                    let older = older_after(&events);
                    let mut events = events;
                    events.sort_by_key(|event| std::cmp::Reverse(event.id));
                    self.groups.set_event_log_fetch(
                        chat,
                        ChatEventLogFetch::Loaded(ChatEventLogPage {
                            events,
                            older,
                            poll: LogPoll::Idle,
                        }),
                    );
                }
            }
            RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { .. }) => {
                if let Some(page) = self.groups.loaded_event_log_mut(chat)
                    && page.older == (OlderEvents::Loading { request: id })
                {
                    page.older = older_after(&events);
                    merge_events(&mut page.events, events);
                }
            }
            _ => {}
        }
    }

    /// The older page of `chat`'s loaded log was refused or never sent: the
    /// log keeps what it has and draws Retry. Nothing else changes.
    pub(crate) fn fail_older_events(&mut self, chat: i64, line: String) {
        if let Some(page) = self.groups.loaded_event_log_mut(chat) {
            page.older = OlderEvents::Failed(line);
        }
    }

    /// The `getChatEventLog` request `pending` failed. The stage that holds
    /// its id settles: the first page fails the log (`first_line`), an older
    /// page keeps the loaded log with Retry (`older_line`), a poll changes
    /// nothing shown (the next tick asks again). Any other request is
    /// dropped.
    pub(super) fn fail_event_log_request(
        &mut self,
        pending: &PendingRequest,
        first_line: impl FnOnce() -> String,
        older_line: impl FnOnce() -> String,
    ) {
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let (chat, id) = (chat_id.0, pending.id);
        match pending.purpose {
            RequestPurpose::Groups(GroupsPurpose::PollChatEventLog) => {
                if let Some(page) = self.groups.loaded_event_log_mut(chat)
                    && page.poll == (LogPoll::Polling { request: id })
                {
                    page.poll = LogPoll::Idle;
                }
            }
            RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { .. }) => {
                if let Some(ChatEventLogFetch::Loading { request }) =
                    self.groups.event_log_fetch(chat)
                {
                    if *request == id {
                        self.groups
                            .set_event_log_fetch(chat, ChatEventLogFetch::Failed(first_line()));
                    }
                } else if let Some(page) = self.groups.loaded_event_log_mut(chat)
                    && page.older == (OlderEvents::Loading { request: id })
                {
                    page.older = OlderEvents::Failed(older_line());
                }
            }
            _ => {}
        }
    }
}

/// Whether an older page is worth asking after `events` came back: a full
/// page says yes, a short one exhausts the log.
fn older_after(events: &[ParsedChatEvent]) -> OlderEvents {
    if events.len() as i32 >= CHAT_EVENT_LOG_PAGE_SIZE {
        OlderEvents::Available
    } else {
        OlderEvents::Exhausted
    }
}

/// `events` join `loaded`, deduped by event id, newest first.
fn merge_events(loaded: &mut Vec<ParsedChatEvent>, events: Vec<ParsedChatEvent>) {
    crate::state::paging::append_new_by_id(loaded, events, |event| event.id);
    loaded.sort_by_key(|event| std::cmp::Reverse(event.id));
}

//! Connect driver: the shared-media calendar. The gallery's Calendar button
//! opens the same box as "Jump to date", filtered to the active tab
//! (`getChatMessageCalendar`); picking a day reloads the tab from that day
//! (`searchChatMessages` around the day's first message).
use super::*;
use crate::state::{day_window_offset, nearest_media_day};

/// What picking a day in the shared-media calendar did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedMediaJump {
    /// The tab reloads around the day.
    Jumped,
    /// No day with media to land on; the box stays open with a note.
    NoMedia,
    /// The calendar's chat is no longer the gallery's: the box is dropped.
    Stale,
}

impl<S: JsonSender> ConnectDriver<S> {
    /// Open the calendar for the gallery's active tab and page in the
    /// current month's days.
    pub fn open_shared_media_calendar(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let now = crate::local_time::now_unix();
        if self.session.open_shared_media_calendar(now).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.fetch_calendar_page()
    }

    /// A day was picked in the shared-media calendar: reload the tab from
    /// that day, or from the nearest day that has media. The calendar is
    /// consumed only when the jump goes ahead or can never go ahead (its
    /// chat left the gallery); a day with nothing near it leaves the box
    /// open with a note.
    pub(super) fn jump_shared_media_to_day(
        &mut self,
        tab: SharedMediaTab,
        day_number: i64,
    ) -> Result<SharedMediaJump, ConnectSendError> {
        let Some(calendar) = self.session.search.history_calendar.as_ref() else {
            return Ok(SharedMediaJump::Stale);
        };
        let chat_id = calendar.chat_id;
        if self.session.media.shared_media.chat_id != Some(chat_id) {
            self.session.search.history_calendar = None;
            return Ok(SharedMediaJump::Stale);
        }
        let Some((target, day)) = nearest_media_day(&calendar.days, day_number) else {
            self.session.search.date_jump_note =
                Some(crate::lang::tr(crate::lang::keys::Q_MISC_NO_MEDIA_TO_JUMP_TO).to_string());
            return Ok(SharedMediaJump::NoMedia);
        };
        let (from, offset) = (
            day.message_id,
            day_window_offset(day.total_count, SHARED_MEDIA_PAGE_SIZE),
        );
        let Some(filter) = tab.filter_constructor() else {
            self.session.search.history_calendar = None;
            return Ok(SharedMediaJump::Stale);
        };
        self.session.search.history_calendar = None;
        let generation = self
            .session
            .media
            .shared_media
            .begin_fetch_at_day(tab, target);
        let extra = self.session.request(
            RequestPurpose::GetSharedMedia { tab, generation },
            Some(chat_id),
        );
        let filter = search_messages_filter_json(filter);
        match self.sender.send_json(&search_chat_messages(
            extra,
            chat_id,
            &TopicId::None,
            "",
            from,
            offset,
            SHARED_MEDIA_PAGE_SIZE,
            Some(filter),
        )) {
            Ok(()) => Ok(SharedMediaJump::Jumped),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.media.shared_media.fail(
                    chat_id,
                    tab,
                    generation,
                    "Could not send the shared-media request.".to_string(),
                );
                Err(err)
            }
        }
    }
}

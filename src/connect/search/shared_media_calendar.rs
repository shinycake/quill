//! Connect driver: the shared-media calendar. The gallery's Calendar button
//! opens the same box as "Jump to date", filtered to the active tab
//! (`getChatMessageCalendar`); picking a day reloads the tab from that day
//! (`searchChatMessages` around the day's first message).
use super::*;
use crate::state::{day_window_offset, nearest_media_day};

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
    /// that day, or from the nearest day that has media.
    pub(super) fn jump_shared_media_to_day(
        &mut self,
        tab: SharedMediaTab,
        day_number: i64,
    ) -> Result<(), ConnectSendError> {
        let Some(calendar) = self.session.search.history_calendar.take() else {
            return Ok(());
        };
        let Some((target, day)) = nearest_media_day(&calendar.days, day_number) else {
            return Ok(());
        };
        let (from, offset) = (
            day.message_id,
            day_window_offset(day.total_count, SHARED_MEDIA_PAGE_SIZE),
        );
        let chat_id = calendar.chat_id;
        if self.session.media.shared_media.chat_id != Some(chat_id) {
            return Ok(());
        }
        let generation = self
            .session
            .media
            .shared_media
            .begin_fetch_at_day(tab, target);
        let extra = self.session.request(
            RequestPurpose::GetSharedMedia { tab, generation },
            Some(chat_id),
        );
        let filter = search_messages_filter_json(tab.filter_constructor());
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
            Ok(()) => Ok(()),
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

//! A click on the picture beside a poll option (tdesktop `MakePollThumbnail`
//! handlers): a photo, clip or GIF opens in the media viewer, a link asks
//! before it opens (`OpenPollOptionLinkBox`), a place opens on the map and a
//! sticker shows its pack.

use super::super::app::QuillApp;
use super::super::entity_links::OpenLinkConfirm;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::link_policy::{OpenDecision, open_decision};
use quill::media_viewer::poll_option_viewer_item;
use quill::telegram::envelope::{MessageContent, PollOptionMedia};
use quill::text::LinkTarget;

impl QuillApp {
    /// The media of option `index` of the poll in `message_id`, as the
    /// session has it now.
    fn poll_option_media(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        index: usize,
    ) -> Option<PollOptionMedia> {
        let message = self
            .session()?
            .histories
            .get(&chat_id.0)?
            .messages
            .get(&message_id.0)?;
        match &message.content {
            MessageContent::Poll(poll) => poll.poll.options.get(index)?.media.clone(),
            _ => None,
        }
    }

    pub(in crate::ui) fn open_poll_option_media(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(media) = self.poll_option_media(chat_id, message_id, index) else {
            return;
        };
        match &media {
            PollOptionMedia::Photo(_)
            | PollOptionMedia::Video(_)
            | PollOptionMedia::Animation(_) => {
                if let Some(item) = poll_option_viewer_item(chat_id, message_id, &media) {
                    self.open_poll_option_viewer(item, cx);
                }
            }
            PollOptionMedia::Sticker(sticker) => {
                if sticker.set_id != 0 {
                    self.view_message_sticker_set(sticker.set_id, cx);
                }
            }
            PollOptionMedia::Link { url, .. } => self.confirm_poll_option_link(url, cx),
            PollOptionMedia::Location(place)
            | PollOptionMedia::Venue {
                location: place, ..
            } => {
                self.open_link(&place.open_street_map_url(), cx);
            }
        }
    }

    /// tdesktop always asks before opening an option's link: the address,
    /// then "Open" or "Cancel". The address is checked like any link
    /// (look-alike letters are marked).
    fn confirm_poll_option_link(&mut self, url: &str, cx: &mut Context<Self>) {
        let link = LinkTarget::Url {
            url: url.to_string(),
            label: None,
        };
        let (url, shown, suspicious) = match open_decision(&link) {
            OpenDecision::Confirm {
                url,
                shown,
                suspicious,
            } => (url, shown, suspicious),
            OpenDecision::Open => (url.to_string(), url.to_string(), Vec::new()),
        };
        self.message_ui.open_link_confirm = Some(OpenLinkConfirm {
            url,
            shown,
            suspicious,
            from_poll_option: true,
        });
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "option_media_tests.rs"]
mod tests;

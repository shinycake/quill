//! The gallery's profile-style tabs (tdesktop `info/profile/tabs`): the
//! Photos / Videos split, "load more", and what a click on a Stories,
//! Gifts or Playlist row does.

use super::app::QuillApp;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::*;
use quill::ids::MessageId;
use quill::state::{SharedMediaAction, SharedMediaTab};
use quill::telegram::envelope::{ChatKind, MessageSender};

impl QuillApp {
    /// "Split" / "Merge" in the gallery header while a media tab is shown
    /// (tdesktop's Expand / Collapse action on the media tab).
    pub(super) fn shared_media_split_button(
        &self,
        active: SharedMediaTab,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !matches!(
            active,
            SharedMediaTab::Media | SharedMediaTab::Photos | SharedMediaTab::Videos
        ) {
            return None;
        }
        let split = self.session()?.media.shared_media.media_split;
        let (label, tip) = if split {
            ("Merge", "Show photos and videos together")
        } else {
            ("Split", "Show photos and videos separately")
        };
        Some(
            Button::new("shared-media-split")
                .ghost()
                .label(label)
                .tooltip(tip)
                .accessibility_label(tip)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_shared_media_split_ui(!split, cx);
                }))
                .into_any_element(),
        )
    }

    pub(super) fn set_shared_media_split_ui(&mut self, split: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.set_shared_media_split(split).is_err() {
                self.connection.status_note = "could not load that tab".into();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let counts = session
                .media
                .shared_media
                .chat_id
                .and_then(|id| session.media.chat_media_counts.get(&id.0))
                .cloned();
            session
                .media
                .shared_media
                .set_media_split(split, counts.as_ref());
        }
        cx.notify();
    }

    /// The next page of a gallery tab (live only).
    pub(super) fn load_more_shared_media_ui(
        &mut self,
        tab: SharedMediaTab,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut()
            && live.driver.fetch_more_shared_media(tab).is_err()
        {
            self.connection.status_note = "could not load more".into();
        }
        cx.notify();
    }

    /// A click on a gallery row. Messages open the viewer or jump to the
    /// message; stories open the story viewer over the loaded stories;
    /// gifts open the received-gifts list of the chat; saved tracks are
    /// read-only.
    pub(super) fn activate_shared_media_item(
        &mut self,
        action: SharedMediaAction,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        match action {
            SharedMediaAction::Message => {
                // Photos, videos and GIFs open the viewer over this list;
                // everything else jumps to the message.
                if !self.open_shared_media_viewer(message_id, cx) {
                    self.jump_to_shared_media_item_ui(message_id, cx);
                }
            }
            SharedMediaAction::Story(story_id) => self.open_shared_media_story(story_id, cx),
            SharedMediaAction::Gift => self.open_shared_media_gifts(cx),
            SharedMediaAction::Audio => {}
        }
    }

    fn open_shared_media_story(&mut self, story_id: i32, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(chat_id) = session.media.shared_media.chat_id else {
            return;
        };
        let keys: Vec<(i64, i32)> = session.media.shared_media.tabs
            [SharedMediaTab::Stories.index()]
        .items
        .iter()
        .filter_map(|item| match item.action {
            SharedMediaAction::Story(id) => Some((chat_id.0, id)),
            _ => None,
        })
        .collect();
        let items = quill::story_viewer::collect_found_story_items(&keys, &session.stories.stories);
        if let Some(index) = items.iter().position(|item| item.story_id == story_id) {
            self.begin_story_viewer(items, index, cx);
        }
    }

    fn open_shared_media_gifts(&mut self, cx: &mut Context<Self>) {
        let owner = self.session().and_then(|session| {
            let chat = session.chats.get(&session.media.shared_media.chat_id?.0)?;
            match chat.kind {
                ChatKind::Private { user_id } if user_id.0 > 0 => {
                    Some(MessageSender::User { user_id: user_id.0 })
                }
                ChatKind::Supergroup {
                    is_channel: true, ..
                } => Some(MessageSender::Chat { chat_id: chat.id.0 }),
                _ => None,
            }
        });
        if let Some(owner) = owner {
            self.open_gifts(owner, cx);
        }
    }
}

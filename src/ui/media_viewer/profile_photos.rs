//! What the profile photo viewer shows and offers for the photo on
//! screen: tdesktop's header ("Photo 2 of 5", "Group Photo", ...) and the
//! Delete / Set as Main / Report rules of `OverlayWidget`.
#![deny(clippy::indexing_slicing)]

use super::*;
use quill::media_viewer::{PhotoHeaderOwner, profile_photo_header};
use quill::state::{GalleryPhoto, PhotoActions, PhotoDelete, PhotoGallery, PhotoOwner};
use quill::telegram::envelope::ChatKind;

/// The header and actions of the photo on screen.
pub(in crate::ui) struct ProfilePhotoFacts {
    pub(in crate::ui) header: String,
    pub(in crate::ui) actions: PhotoActions,
    /// An earlier group or channel photo goes for everyone (basic groups;
    /// supergroups always delete for everyone).
    revoke: bool,
}

impl QuillApp {
    /// The gallery the viewer pages over and its entry on screen.
    fn viewer_gallery_photo(&self) -> Option<(PhotoGallery, GalleryPhoto)> {
        if self.viewer.state.source() != ViewerSource::Profile {
            return None;
        }
        let owner = self.viewer.extra.profile_owner?;
        let item = self.viewer.state.current()?;
        let gallery = self.session()?.photo_gallery(owner)?;
        let photo = gallery
            .photos
            .iter()
            .find(|photo| photo.photo.id == item.message_id.0)?
            .clone();
        Some((gallery, photo))
    }

    pub(in crate::ui) fn profile_photo_facts(&self) -> Option<ProfilePhotoFacts> {
        let (gallery, photo) = self.viewer_gallery_photo()?;
        let session = self.session()?;
        let header_owner = match gallery.owner {
            PhotoOwner::User(_) => PhotoHeaderOwner::User,
            PhotoOwner::Chat(chat_id) => match session.chats.get(&chat_id.0).map(|c| &c.kind) {
                Some(ChatKind::Supergroup {
                    is_channel: true, ..
                }) => PhotoHeaderOwner::Channel,
                _ => PhotoHeaderOwner::Group,
            },
        };
        let (position, total) = self.viewer.state.position().unwrap_or((1, 1));
        let header = profile_photo_header(header_owner, photo.kind, position, total);
        let mut actions = session.photo_actions(&gallery, &photo);
        let mut revoke = false;
        // An earlier photo's service message: TDLib's `messageProperties`
        // decide, as tdesktop's `HistoryItem::canDelete`; until they
        // arrive, your own message or a chat you may edit.
        if let Some(PhotoDelete::Message {
            chat_id,
            message_id,
            is_outgoing,
        }) = actions.delete
        {
            let known = session
                .messages
                .message_menu_actions
                .filter(|(c, m, _)| *c == chat_id && *m == message_id)
                .map(|(_, _, actions)| actions);
            let allowed = known.map_or(
                is_outgoing || session.chat_can_edit_information(chat_id),
                |a| a.can_be_deleted(),
            );
            revoke = known.map_or(is_outgoing, |a| a.can_be_deleted_for_all_users);
            if !allowed {
                actions.delete = None;
            }
        }
        Some(ProfilePhotoFacts {
            header,
            actions,
            revoke,
        })
    }

    /// Trash on a profile photo: the viewer closes and the delete box
    /// opens (tdesktop `OverlayWidget::deleteMedia`).
    pub(super) fn delete_viewer_profile_photo(&mut self, cx: &mut Context<Self>) {
        let Some(facts) = self.profile_photo_facts() else {
            return;
        };
        let Some(delete) = facts.actions.delete else {
            return;
        };
        self.close_media_viewer(cx);
        self.dialogs.photo_delete = Some((delete, facts.revoke));
        cx.notify();
    }

    /// What TDLib allows for an earlier group or channel photo's message,
    /// asked when the viewer lands on it.
    pub(super) fn ask_profile_photo_message_actions(&mut self) {
        let Some((_, photo)) = self.viewer_gallery_photo() else {
            return;
        };
        let Some(PhotoOwner::Chat(chat_id)) = self.viewer.extra.profile_owner else {
            return;
        };
        if let (Some(message_id), Some(live)) = (photo.message_id, self.live.as_mut()) {
            let _ = live.driver.fetch_message_menu_actions(chat_id, message_id);
        }
    }
}

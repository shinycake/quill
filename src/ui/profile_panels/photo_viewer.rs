//! The profile photo viewer for users, groups and channels: opening it
//! from an info panel's avatar, paging, "Set as Main" and "Report"
//! (tdesktop `OverlayWidget` over `UserPhotos` and
//! `SharedMediaType::ChatPhoto`).

use super::*;
use crate::ui::status_note::StatusNote;
use quill::lang::{keys, tr, tr_with};
use quill::media_viewer::{MediaViewer, MediaViewerItem, ViewerSource, profile_photo_items};
use quill::state::{ChatPhotosFetch, PhotoGallery, PhotoOwner, ProfilePhotosFetch};

/// The viewer items of a gallery, newest first.
fn gallery_items(gallery: &PhotoGallery) -> Vec<MediaViewerItem> {
    let chat_id = match gallery.owner {
        PhotoOwner::Chat(chat_id) => chat_id,
        PhotoOwner::User(_) => ChatId(0),
    };
    profile_photo_items(gallery.photos.iter().map(|photo| &photo.photo), chat_id)
}

impl QuillApp {
    /// Open the photos of `owner` in the media viewer, on the current one
    /// with the earlier ones after it. A user's list is fetched first when
    /// it is not cached; a group or channel opens once its current photo
    /// is known, and its history fills in while the viewer is open.
    pub(in crate::ui) fn open_peer_photos(&mut self, owner: PhotoOwner, cx: &mut Context<Self>) {
        if let (PhotoOwner::Chat(chat_id), Some(live)) = (owner, self.live.as_mut()) {
            if matches!(
                live.driver
                    .session
                    .users_state
                    .chat_photo_history
                    .get(&chat_id.0),
                Some(ChatPhotosFetch::Failed(_))
            ) {
                live.driver
                    .session
                    .users_state
                    .chat_photo_history
                    .remove(&chat_id.0);
            }
            let _ = live.driver.fetch_chat_photos(chat_id);
        }
        let ready = self
            .session()
            .and_then(|s| s.photo_gallery(owner))
            .is_some_and(|gallery| !gallery.photos.is_empty());
        if ready {
            self.show_photo_gallery(owner, cx);
            return;
        }
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_PROFILE_PHOTOS_NEED_A_LIVE,
            )));
            cx.notify();
            return;
        };
        let sent = match owner {
            PhotoOwner::User(user_id) => {
                // A failed earlier fetch is retried.
                if matches!(
                    live.driver
                        .session
                        .users_state
                        .user_profile_photos
                        .get(&user_id),
                    Some(ProfilePhotosFetch::Failed(_))
                ) {
                    live.driver
                        .session
                        .users_state
                        .user_profile_photos
                        .remove(&user_id);
                }
                live.driver.fetch_user_profile_photos(user_id).map(|_| ())
            }
            // Sent above.
            PhotoOwner::Chat(_) => Ok(()),
        };
        match sent {
            Ok(()) => self.dialogs.pending_profile_gallery = Some(owner),
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            ))),
        }
        cx.notify();
    }

    /// Poll-loop hook: open the gallery that was waiting for its photos,
    /// and add newly loaded pages to an open one. Returns `true` when
    /// something changed.
    pub(in crate::ui) fn pump_profile_gallery(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = self.sync_profile_photo_viewer(cx);
        let Some(owner) = self.dialogs.pending_profile_gallery else {
            return changed;
        };
        let failure = self.session().and_then(|s| match owner {
            PhotoOwner::User(user_id) => match s.users_state.user_profile_photos.get(&user_id) {
                Some(ProfilePhotosFetch::Failed(reason)) => Some(reason.clone()),
                _ => None,
            },
            PhotoOwner::Chat(chat_id) => match s.users_state.chat_photo_history.get(&chat_id.0) {
                Some(ChatPhotosFetch::Failed(reason)) => Some(reason.clone()),
                _ => None,
            },
        });
        let Some(gallery) = self.session().and_then(|s| s.photo_gallery(owner)) else {
            if let Some(reason) = failure {
                self.dialogs.pending_profile_gallery = None;
                self.connection.set_note(StatusNote::error(tr_with(
                    keys::Q_PEOPLE_COULDNT_LOAD_PROFILE_PHOTOS,
                    &[("reason", &reason)],
                )));
                changed = true;
            }
            return changed;
        };
        self.dialogs.pending_profile_gallery = None;
        if !gallery.photos.is_empty() {
            self.show_photo_gallery(owner, cx);
        } else if let Some(reason) = failure {
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_PEOPLE_COULDNT_LOAD_PROFILE_PHOTOS,
                &[("reason", &reason)],
            )));
        } else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_NO_PROFILE_PHOTOS)));
        }
        true
    }

    fn show_photo_gallery(&mut self, owner: PhotoOwner, cx: &mut Context<Self>) {
        let Some(gallery) = self.session().and_then(|s| s.photo_gallery(owner)) else {
            return;
        };
        let items = gallery_items(&gallery);
        if items.is_empty() {
            return;
        }
        self.dialogs.pending_profile_gallery = None;
        self.viewer.state = MediaViewer::open_profile(items, 0, gallery.total);
        self.viewer.extra.profile_owner = Some(owner);
        self.viewer.open_gen += 1;
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
    }

    /// Keep an open profile photo viewer in step with its gallery: append
    /// the pages that arrived, and ask for the next one near the end.
    pub(in crate::ui) fn sync_profile_photo_viewer(&mut self, cx: &mut Context<Self>) -> bool {
        if self.viewer.state.source() != ViewerSource::Profile {
            return false;
        }
        let Some(owner) = self.viewer.extra.profile_owner else {
            return false;
        };
        let gallery = self.session().and_then(|s| s.photo_gallery(owner));
        let mut changed = false;
        if let Some(gallery) = &gallery
            && (gallery.photos.len() != self.viewer.state.len()
                || self.viewer.state.position().map(|(_, total)| total) != Some(gallery.total))
        {
            changed = self
                .viewer
                .state
                .extend_photos(gallery_items(gallery), gallery.total)
                > 0;
            cx.notify();
        }
        self.download_viewer_strip_thumbs();
        if self.viewer.state.wants_more_photos()
            && gallery.is_some_and(|gallery| gallery.has_more)
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_more_photos(owner);
        }
        changed
    }

    /// "Set as Main" on an earlier photo: your own (`setProfilePhoto` with
    /// `inputChatPhotoPrevious`) or a group or channel's (`setChatPhoto`).
    /// The viewer closes, as tdesktop's does once the photo changes.
    pub(in crate::ui) fn set_viewer_photo_as_main(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.viewer.state.current().cloned() else {
            return;
        };
        let Some(owner) = self.viewer.extra.profile_owner else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_PROFILE_PHOTO_CHANGES_NEED_A,
            )));
            cx.notify();
            return;
        };
        let sent = match owner {
            PhotoOwner::User(_) => live.driver.set_profile_photo_previous(item.message_id.0),
            PhotoOwner::Chat(chat_id) => live
                .driver
                .set_chat_photo_from_earlier(chat_id, item.download_file_id.0),
        };
        match sent {
            Ok(_) => {
                self.connection.set_note(StatusNote::silent(tr(
                    keys::Q_PEOPLE_MAIN_PROFILE_PHOTO_UPDATED,
                )));
                self.close_media_viewer(cx);
            }
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            ))),
        }
        cx.notify();
    }

    /// Open the viewer's "Report" for the photo on screen: the viewer
    /// closes first so the reason list is not hidden behind it.
    pub(in crate::ui) fn report_viewer_profile_photo(&mut self, cx: &mut Context<Self>) {
        let Some(owner) = self.viewer.extra.profile_owner else {
            return;
        };
        let Some(file_id) = self
            .viewer
            .state
            .current()
            .map(|item| item.download_file_id.0)
        else {
            return;
        };
        self.close_media_viewer(cx);
        self.dialogs.profile_dialog = Some(ProfileDialog::ReportPhoto { owner, file_id });
        cx.notify();
    }

    pub(super) fn submit_photo_report(
        &mut self,
        owner: PhotoOwner,
        file_id: i32,
        reason: PhotoReportReason,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_DEMO_MODE_REPORTS_NEED_A,
            )));
            cx.notify();
            return;
        };
        match live.driver.report_photo(owner, file_id, reason.td_type()) {
            Ok(_) => self
                .connection
                .set_note(StatusNote::silent(tr(keys::Q_PEOPLE_REPORT_SENT))),
            Err(_) => self.connection.set_note(StatusNote::error(tr(
                keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
            ))),
        }
        cx.notify();
    }
}

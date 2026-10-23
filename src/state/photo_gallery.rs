//! The profile photo viewer's galleries and what may be done with each
//! photo, after tdesktop's `OverlayWidget` (media_view_overlay_widget.cpp):
//! a user's photos (`getUserProfilePhotos` with the personal photo first
//! and the public photo last, `data_user_photos`) and a group or channel's
//! photo history (`SharedMediaWithLastSlice` over
//! `searchMessagesFilterChatPhoto`, the current photo first).
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::telegram::envelope::{
    MessageContent, ParsedMessage, ParsedProfilePhoto, PhotoContent, ServiceAction,
};

/// The Telegram service notifications account: its photos are never
/// reported (tdesktop `isNotificationsUser`).
const SERVICE_NOTIFICATIONS_USER: i64 = 777_000;

/// What the viewer may do with one gallery photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhotoActions {
    pub delete: Option<PhotoDelete>,
    /// "Set as Main" (tdesktop `lng_mediaview_set_userpic`).
    pub set_main: bool,
    /// "Report" (`reportChatPhoto`).
    pub report: bool,
}

/// How a gallery photo is deleted (tdesktop `OverlayWidget::deleteMedia`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoDelete {
    /// One of your own photos: "Do you want to delete this photo?", then
    /// `deleteProfilePhoto` (TDLib clears the main or public photo the
    /// way `PeerPhoto::clear` does).
    Own { photo_id: i64 },
    /// A group or channel's current photo: the same box, then
    /// `setChatPhoto` with no photo (`messages.editChatPhoto` /
    /// `channels.editPhoto` with `inputChatPhotoEmpty`).
    ChatPhoto { chat_id: ChatId },
    /// An earlier photo of a group or channel: its
    /// `messageChatChangePhoto` goes through the delete-messages box.
    Message {
        chat_id: ChatId,
        message_id: MessageId,
        is_outgoing: bool,
    },
}

impl ProfilePhoto {
    /// The gallery entry of a `messageChatChangePhoto` photo: the grid
    /// size and the largest one, as `parse_profile_photo` picks them.
    pub fn from_photo_content(id: i64, photo: &PhotoContent) -> Option<Self> {
        let full = photo.largest_size()?;
        let thumb = photo.thumb_size().unwrap_or(full);
        Some(Self {
            id,
            added_date: 0,
            thumb_file_id: thumb.file_id.0,
            full_file_id: full.file_id.0,
            width: full.width,
            height: full.height,
        })
    }
}

/// A user's gallery from its parts: the personal photo first, the
/// loaded list (the main photo in it marked), and the public photo last
/// once every page is in. `total` counts the photos not loaded yet.
pub fn user_gallery(
    user_id: i64,
    list: &[ProfilePhoto],
    total_count: i32,
    main_id: Option<i64>,
    personal: Option<&ProfilePhoto>,
    public: Option<&ProfilePhoto>,
) -> PhotoGallery {
    let listed = |id: i64| list.iter().any(|photo| photo.id == id);
    let list_total = usize::try_from(total_count).unwrap_or(0).max(list.len());
    let has_more = list.len() < list_total;
    let main_id = main_id.or_else(|| list.first().map(|photo| photo.id));
    let mut photos = Vec::with_capacity(list.len() + 2);
    let mut total = list_total;
    if let Some(personal) = personal {
        photos.push(GalleryPhoto {
            photo: personal.clone(),
            kind: GalleryPhotoKind::Personal,
            message_id: None,
            is_outgoing: false,
        });
        if !listed(personal.id) {
            total += 1;
        }
    }
    let personal_id = personal.map(|photo| photo.id);
    photos.extend(
        list.iter()
            .filter(|photo| Some(photo.id) != personal_id)
            .map(|photo| GalleryPhoto {
                photo: photo.clone(),
                kind: if Some(photo.id) == main_id {
                    GalleryPhotoKind::Main
                } else {
                    GalleryPhotoKind::Earlier
                },
                message_id: None,
                is_outgoing: false,
            }),
    );
    if let Some(public) =
        public.filter(|public| !listed(public.id) && Some(public.id) != personal_id)
    {
        total += 1;
        if !has_more {
            photos.push(GalleryPhoto {
                photo: public.clone(),
                kind: GalleryPhotoKind::Public,
                message_id: None,
                is_outgoing: false,
            });
        }
    }
    PhotoGallery {
        owner: PhotoOwner::User(user_id),
        photos,
        total,
        has_more,
    }
}

/// A chat's gallery from its parts: the current photo first, then the
/// photo changes newest first. As in `SharedMediaWithLastSlice`, the
/// current photo is its own entry only when the newest change set a
/// different photo ("isolated"); otherwise that change is the current
/// photo. `deleted` drops changes whose message is gone.
pub fn chat_gallery(
    chat_id: ChatId,
    current: Option<&ProfilePhoto>,
    changes: &[ChatPhotoChange],
    total_count: i32,
    has_more: bool,
    deleted: impl Fn(MessageId) -> bool,
) -> PhotoGallery {
    let shown: Vec<&ChatPhotoChange> = changes
        .iter()
        .filter(|change| !deleted(change.message_id))
        .collect();
    let removed = changes.len().saturating_sub(shown.len());
    let listed_total = usize::try_from(total_count)
        .unwrap_or(0)
        .saturating_sub(removed)
        .max(shown.len());
    let isolated = current.filter(|current| {
        shown
            .first()
            .is_none_or(|newest| newest.photo.id != current.id)
    });
    let mut photos = Vec::with_capacity(shown.len() + 1);
    if let Some(current) = isolated {
        photos.push(GalleryPhoto {
            photo: current.clone(),
            kind: GalleryPhotoKind::Main,
            message_id: None,
            is_outgoing: false,
        });
    }
    let current_id = current.map(|photo| photo.id);
    photos.extend(shown.into_iter().map(|change| GalleryPhoto {
        photo: change.photo.clone(),
        kind: if Some(change.photo.id) == current_id {
            GalleryPhotoKind::Main
        } else {
            GalleryPhotoKind::Earlier
        },
        message_id: Some(change.message_id),
        is_outgoing: change.is_outgoing,
    }));
    PhotoGallery {
        owner: PhotoOwner::Chat(chat_id),
        photos,
        total: listed_total + usize::from(isolated.is_some()),
        has_more,
    }
}

/// The photo change a found message records, if it is one.
fn photo_change(message: &ParsedMessage) -> Option<ChatPhotoChange> {
    let MessageContent::Action(action) = &message.content else {
        return None;
    };
    let ServiceAction::ChatPhoto {
        photo: Some(photo),
        photo_id,
    } = action.as_ref()
    else {
        return None;
    };
    Some(ChatPhotoChange {
        message_id: message.id,
        photo: ProfilePhoto::from_photo_content(*photo_id, photo)?,
        is_outgoing: message.is_outgoing,
    })
}

impl Session {
    /// The gallery the profile photo viewer pages over for `owner`, or
    /// `None` while nothing of it can be shown yet. A chat's gallery is
    /// ready once its current photo is known or its history is in.
    pub fn photo_gallery(&self, owner: PhotoOwner) -> Option<PhotoGallery> {
        match owner {
            PhotoOwner::User(user_id) => {
                let Some(ProfilePhotosFetch::Loaded {
                    photos,
                    total_count,
                    ..
                }) = self.users_state.user_profile_photos.get(&user_id)
                else {
                    return None;
                };
                let info = self.user_full_info(user_id);
                let parsed =
                    |photo: Option<&ParsedProfilePhoto>| photo.map(ProfilePhoto::from_parsed);
                let personal = parsed(info.and_then(|info| info.extras.personal_photo.as_ref()));
                let public = parsed(info.and_then(|info| info.extras.public_photo.as_ref()));
                Some(user_gallery(
                    user_id,
                    photos,
                    *total_count,
                    info.and_then(|info| info.photo_id),
                    personal.as_ref(),
                    public.as_ref(),
                ))
            }
            PhotoOwner::Chat(chat_id) => {
                let current = self.users_state.chat_current_photos.get(&chat_id.0);
                let history = self.users_state.chat_photo_history.get(&chat_id.0);
                let (changes, total_count, has_more) = match history {
                    Some(ChatPhotosFetch::Loaded {
                        changes,
                        total_count,
                        next_from_message_id,
                        ..
                    }) => (
                        changes.as_slice(),
                        *total_count,
                        next_from_message_id.0 != 0
                            && usize::try_from(*total_count).unwrap_or(0) > changes.len(),
                    ),
                    Some(ChatPhotosFetch::Failed(_)) => (&[][..], 0, false),
                    Some(ChatPhotosFetch::Loading) | None => {
                        current.and_then(Option::as_ref)?;
                        (&[][..], 0, false)
                    }
                };
                let history_entry = self.histories.get(&chat_id.0);
                Some(chat_gallery(
                    chat_id,
                    current.and_then(Option::as_ref),
                    changes,
                    total_count,
                    has_more,
                    |message_id| history_entry.is_some_and(|h| h.is_tombstone(message_id)),
                ))
            }
        }
    }

    /// Whether the current user may change a group or channel's title,
    /// photo and description (tdesktop `canEditInformation`): channels
    /// need the right; in groups a member (tdesktop `amIn`, by the status
    /// TDLib reported for the chat or its basic group) may unless the
    /// default permissions forbid it.
    pub fn chat_can_edit_information(&self, chat_id: ChatId) -> bool {
        if self.chat_can_change_info(chat_id) {
            return true;
        }
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        let is_group = match chat.kind {
            ChatKind::BasicGroup { .. } => true,
            ChatKind::Supergroup { is_channel, .. } => !is_channel,
            _ => false,
        };
        is_group
            && matches!(
                self.group_own_status(chat_id),
                Some(
                    ChannelMemberStatus::Creator
                        | ChannelMemberStatus::Administrator
                        | ChannelMemberStatus::Member
                )
            )
            && chat.permissions.as_ref().is_some_and(|p| p.can_change_info)
    }

    /// What may be done with `photo` of `gallery` (tdesktop's viewer menu,
    /// media_view_overlay_widget.cpp `canDelete`, "Set userpic" and
    /// "Report userpic").
    pub fn photo_actions(&self, gallery: &PhotoGallery, photo: &GalleryPhoto) -> PhotoActions {
        let main = photo.kind == GalleryPhotoKind::Main;
        match gallery.owner {
            PhotoOwner::User(user_id) => {
                let is_self = self.my_user_id == Some(user_id);
                PhotoActions {
                    delete: is_self.then_some(PhotoDelete::Own {
                        photo_id: photo.photo.id,
                    }),
                    set_main: is_self && !main,
                    report: !is_self
                        && user_id != SERVICE_NOTIFICATIONS_USER
                        && photo.kind != GalleryPhotoKind::Personal,
                }
            }
            PhotoOwner::Chat(chat_id) => {
                let can_edit = self.chat_can_edit_information(chat_id);
                let delete = if main {
                    can_edit.then_some(PhotoDelete::ChatPhoto { chat_id })
                } else {
                    photo.message_id.map(|message_id| PhotoDelete::Message {
                        chat_id,
                        message_id,
                        is_outgoing: photo.is_outgoing,
                    })
                };
                PhotoActions {
                    delete,
                    set_main: can_edit && !main,
                    report: !can_edit,
                }
            }
        }
    }

    /// Keep a group or channel's current photo from its full info.
    pub(crate) fn store_chat_current_photo(
        &mut self,
        chat_id: ChatId,
        photo: Option<ParsedProfilePhoto>,
    ) {
        let photo = photo.map(|photo| {
            let entry = ProfilePhoto::from_parsed(&photo);
            for file in photo.files {
                self.upsert_file(file, false);
            }
            entry
        });
        self.users_state
            .chat_current_photos
            .insert(chat_id.0, photo);
    }

    /// The chat's photo changed: what the viewer knew of its photos is out
    /// of date.
    pub(crate) fn forget_chat_photos(&mut self, chat_id: ChatId) {
        self.users_state.chat_current_photos.remove(&chat_id.0);
        self.users_state.chat_photo_history.remove(&chat_id.0);
    }

    /// A `foundChatMessages` page of a chat's photo history. A later page
    /// (`loading_more`) extends the list.
    pub(crate) fn apply_chat_photo_history(
        &mut self,
        messages: Vec<ParsedMessage>,
        total_count: i32,
        next_from_message_id: MessageId,
        pending: &PendingRequest,
    ) {
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        for message in &messages {
            self.remember_files(&message.files);
        }
        let mut changes = match self.users_state.chat_photo_history.remove(&chat_id.0) {
            Some(ChatPhotosFetch::Loaded {
                changes,
                loading_more: true,
                ..
            }) => changes,
            _ => Vec::new(),
        };
        let known: Vec<MessageId> = changes.iter().map(|change| change.message_id).collect();
        changes.extend(
            messages
                .iter()
                .filter_map(photo_change)
                .filter(|change| !known.contains(&change.message_id)),
        );
        let empty_page = messages.is_empty();
        let next_from_message_id = if empty_page {
            MessageId(0)
        } else {
            next_from_message_id
        };
        self.users_state.chat_photo_history.insert(
            chat_id.0,
            ChatPhotosFetch::Loaded {
                total_count,
                changes,
                next_from_message_id,
                loading_more: false,
            },
        );
    }
}

#[cfg(test)]
#[path = "photo_gallery_tests.rs"]
mod tests;

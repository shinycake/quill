use super::{chat_gallery, user_gallery};
use crate::ids::{ChatId, MessageId};
use crate::state::{ChatPhotoChange, GalleryPhotoKind, ProfilePhoto};

fn photo(id: i64) -> ProfilePhoto {
    ProfilePhoto {
        id,
        added_date: 0,
        thumb_file_id: 1,
        full_file_id: 2,
        width: 100,
        height: 100,
    }
}

fn change(message_id: i64, photo_id: i64) -> ChatPhotoChange {
    ChatPhotoChange {
        message_id: MessageId(message_id),
        photo: photo(photo_id),
        is_outgoing: false,
    }
}

fn kinds(gallery: &crate::state::PhotoGallery) -> Vec<(i64, GalleryPhotoKind)> {
    gallery.photos.iter().map(|p| (p.photo.id, p.kind)).collect()
}

#[test]
fn a_chat_without_a_photo_lists_only_its_history() {
    let changes = [change(20, 2), change(10, 1)];
    let gallery = chat_gallery(ChatId(5), None, &changes, 2, false, |_| false);
    assert_eq!(
        kinds(&gallery),
        [(2, GalleryPhotoKind::Earlier), (1, GalleryPhotoKind::Earlier)]
    );
    assert_eq!(gallery.total, 2);
}

#[test]
fn the_total_never_falls_below_what_is_shown() {
    let changes = [change(20, 2), change(10, 1)];
    let current = photo(3);
    let gallery = chat_gallery(ChatId(5), Some(&current), &changes, 0, false, |id| {
        id == MessageId(10)
    });
    assert_eq!(
        kinds(&gallery),
        [(3, GalleryPhotoKind::Main), (2, GalleryPhotoKind::Earlier)]
    );
    assert_eq!(gallery.total, 2);
}

#[test]
fn the_personal_photo_leads_and_counts_once() {
    let list = [photo(1), photo(2)];
    let personal = photo(2);
    let gallery = user_gallery(31, &list, 2, None, Some(&personal), None);
    assert_eq!(
        kinds(&gallery),
        [(2, GalleryPhotoKind::Personal), (1, GalleryPhotoKind::Main)]
    );
    assert_eq!(gallery.total, 2);
    assert!(!gallery.has_more);
}

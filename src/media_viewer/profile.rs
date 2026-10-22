//! The profile photo viewer's header (tdesktop
//! `OverlayWidget::updateHeader`, media_view_overlay_widget.cpp).

use crate::lang::{keys, tr, tr_with};
use crate::state::GalleryPhotoKind;

/// Whose photo the header names when there is only one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoHeaderOwner {
    User,
    Group,
    Channel,
}

/// "Photo 2 of 5" over several photos, with "Public Photo" on a user's
/// last one and "Photo set by you" on the personal photo; a lone photo is
/// "Profile Photo", "Group Photo" or "Channel Photo".
pub fn profile_photo_header(
    owner: PhotoHeaderOwner,
    kind: GalleryPhotoKind,
    position: usize,
    total: usize,
) -> String {
    let user = owner == PhotoHeaderOwner::User;
    if total > 1 && position >= 1 && position <= total {
        if user && kind == GalleryPhotoKind::Public && position == total {
            return tr(keys::MEDIAVIEW_PROFILE_PUBLIC_PHOTO).to_string();
        }
        if user && kind == GalleryPhotoKind::Personal {
            return tr(keys::MEDIAVIEW_PROFILE_PHOTO_BY_YOU).to_string();
        }
        return tr_with(
            keys::MEDIAVIEW_N_OF_AMOUNT,
            &[("n", &position.to_string()), ("amount", &total.to_string())],
        );
    }
    tr(match owner {
        PhotoHeaderOwner::User if kind == GalleryPhotoKind::Personal => {
            keys::MEDIAVIEW_PROFILE_PHOTO_BY_YOU
        }
        PhotoHeaderOwner::User => keys::MEDIAVIEW_PROFILE_PHOTO,
        PhotoHeaderOwner::Group => keys::MEDIAVIEW_GROUP_PHOTO,
        PhotoHeaderOwner::Channel => keys::MEDIAVIEW_CHANNEL_PHOTO,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{PhotoHeaderOwner as Owner, profile_photo_header as header};
    use crate::state::GalleryPhotoKind as Kind;

    #[test]
    fn several_photos_count_and_name_the_special_ones() {
        assert_eq!(header(Owner::Group, Kind::Main, 1, 3), "Photo 1 of 3");
        assert_eq!(
            header(Owner::Channel, Kind::Earlier, 3, 40),
            "Photo 3 of 40"
        );
        assert_eq!(
            header(Owner::User, Kind::Personal, 1, 4),
            "Photo set by you"
        );
        assert_eq!(header(Owner::User, Kind::Public, 4, 4), "Public Photo");
        // The public photo is only named on the last place.
        assert_eq!(header(Owner::User, Kind::Public, 3, 4), "Photo 3 of 4");
    }

    #[test]
    fn a_lone_photo_names_its_owner() {
        assert_eq!(header(Owner::User, Kind::Main, 1, 1), "Profile Photo");
        assert_eq!(
            header(Owner::User, Kind::Personal, 1, 1),
            "Photo set by you"
        );
        assert_eq!(header(Owner::Group, Kind::Main, 1, 1), "Group Photo");
        assert_eq!(header(Owner::Channel, Kind::Main, 1, 1), "Channel Photo");
    }
}

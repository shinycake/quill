//! Generated-once classification of every `LocationPickerPurpose` variant.
use super::RequestKind;
use crate::location_picker::LocationPickerPurpose;

impl LocationPickerPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::ResolveVenueBot | Self::SearchVenues { .. } | Self::MapTile | Self::VenueIcon => {
                RequestKind::Lookup
            }
        }
    }
}

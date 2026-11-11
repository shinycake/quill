//! The open Premium dialog: which page it is on, the ref it was opened
//! with and the sticker that opened the stickers preview.
#![deny(clippy::indexing_slicing)]

use super::promo::PromoPage;
use crate::telegram::envelope::StickerItem;

/// The Premium dialog while it is open. Closing the dialog drops all of
/// it, so the page, the ref and the requested sticker cannot outlive it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PremiumBox {
    /// The page: the overview, "Doubled limits" or a feature preview.
    pub page: PromoPage,
    /// The ref the dialog was opened with (tdesktop `ShowPremium`).
    pub start_ref: String,
    /// The locked sticker whose click opened the stickers preview; it leads
    /// the preview (tdesktop `Descriptor::requestedSticker`).
    pub requested: Option<StickerItem>,
}

#[cfg(test)]
mod tests {
    use super::super::PremiumHub;
    use super::*;

    #[test]
    fn closing_drops_the_page_the_ref_and_the_requested_sticker() {
        let mut hub = PremiumHub::default();
        assert!(!hub.premium_open());
        hub.open_premium_box(PromoPage::Limits, "stories");
        assert_eq!(hub.promo_page(), PromoPage::Limits);
        assert_eq!(hub.premium_ref(), "stories");
        hub.close_premium_box();
        assert!(!hub.premium_open());
        assert_eq!(hub.promo_page(), PromoPage::Overview);
        assert_eq!(hub.premium_ref(), "");
        hub.set_promo_page(PromoPage::Limits);
        assert!(
            !hub.premium_open(),
            "a page change does not open the dialog"
        );
    }
}

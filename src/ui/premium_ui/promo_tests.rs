//! What the open Premium preview downloads depends only on the state, so a
//! slow answer is picked up whenever it lands (no fixed wait).
use super::preview_downloads;
use quill::ids::FileId;
use quill::premium_hub::promo::{PremiumAnimation, PreviewOpen, PromoFeature, PromoPage};
use quill::premium_hub::{PremiumFeature, PremiumHub, PremiumStateInfo};
use quill::telegram::envelope::{StickerFormat, StickerItem};

fn sticker(file_id: i32) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id: i64::from(file_id),
        set_id: 2,
        emoji: "x".into(),
        width: 100,
        height: 100,
        format: StickerFormat::Webp,
        file_id: FileId(file_id),
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: true,
        premium_animation: None,
    }
}

fn preview_of(feature: PremiumFeature) -> PremiumHub {
    PremiumHub {
        premium_open: true,
        promo_page: PromoPage::Preview {
            feature: PromoFeature::Premium(feature),
            opened: PreviewOpen::Overview,
        },
        ..PremiumHub::default()
    }
}

#[test]
fn the_clip_is_downloaded_whenever_the_premium_state_lands() {
    let mut hub = preview_of(PremiumFeature::UniqueReactions);
    assert!(preview_downloads(&hub).is_empty(), "state not known yet");
    hub.premium_state = Some(PremiumStateInfo {
        animations: vec![PremiumAnimation {
            feature: PromoFeature::Premium(PremiumFeature::UniqueReactions),
            file_id: 77,
            width: 1,
            height: 1,
        }],
        ..PremiumStateInfo::default()
    });
    assert_eq!(preview_downloads(&hub), vec![(77, 32)]);
}

#[test]
fn the_example_stickers_are_downloaded_whenever_their_answer_lands() {
    let mut hub = preview_of(PremiumFeature::UniqueStickers);
    assert!(preview_downloads(&hub).is_empty());
    hub.premium_stickers = vec![sticker(5), sticker(6)];
    assert_eq!(preview_downloads(&hub), vec![(5, 16), (6, 16)]);
    // A closed dialog or another page asks for nothing.
    hub.premium_open = false;
    assert!(preview_downloads(&hub).is_empty());
    hub.premium_open = true;
    hub.promo_page = PromoPage::Overview;
    assert!(preview_downloads(&hub).is_empty());
}

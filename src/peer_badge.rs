//! Title badges next to a chat-row name: verified check, premium star or
//! emoji status, and the SCAM / FAKE labels. Mirrors Telegram Desktop's
//! `Ui::PeerBadge::drawGetWidth` (`ui/unread_badge.cpp`), minus the
//! monoforum label.

/// `verificationStatus` (TDLib `schema/td_api.tl:860`), the flags the chat
/// list needs. `bot_verification_icon_custom_emoji_id` is not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VerificationStatus {
    pub is_verified: bool,
    pub is_scam: bool,
    pub is_fake: bool,
}

/// What to draw after the title, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBadge {
    /// Scam wins over everything: a bordered red "SCAM" label.
    Scam,
    /// A bordered red "FAKE" label.
    Fake,
    /// The verified check. Tdesktop hides it behind an emoji status.
    Verified,
    /// A Premium user's custom emoji status (`emoji_status.type`).
    EmojiStatus(i64),
    /// A Premium user without a status: the star.
    PremiumStar,
}

/// Telegram Desktop's order: scam/fake label, then verified (unless an
/// emoji status is set), then the status emoji, then the premium star.
/// `emoji_status_id` is `0` for none. Only users can be Premium, and
/// only Premium users show a status.
pub fn title_badge(
    verification: VerificationStatus,
    is_premium: bool,
    emoji_status_id: i64,
) -> Option<TitleBadge> {
    if verification.is_scam {
        return Some(TitleBadge::Scam);
    }
    if verification.is_fake {
        return Some(TitleBadge::Fake);
    }
    let status = is_premium && emoji_status_id != 0;
    if verification.is_verified && !status {
        return Some(TitleBadge::Verified);
    }
    if status {
        return Some(TitleBadge::EmojiStatus(emoji_status_id));
    }
    is_premium.then_some(TitleBadge::PremiumStar)
}

impl TitleBadge {
    /// The label text for the bordered badges.
    pub fn label(self) -> Option<&'static str> {
        match self {
            TitleBadge::Scam => Some("SCAM"),
            TitleBadge::Fake => Some("FAKE"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TitleBadge, VerificationStatus, title_badge};

    const NONE: VerificationStatus = VerificationStatus {
        is_verified: false,
        is_scam: false,
        is_fake: false,
    };

    #[test]
    fn plain_peer_has_no_badge() {
        assert_eq!(title_badge(NONE, false, 0), None);
        // A status id without Premium is stale data.
        assert_eq!(title_badge(NONE, false, 7), None);
    }

    #[test]
    fn premium_star_and_emoji_status() {
        assert_eq!(title_badge(NONE, true, 0), Some(TitleBadge::PremiumStar));
        assert_eq!(
            title_badge(NONE, true, 99),
            Some(TitleBadge::EmojiStatus(99))
        );
    }

    #[test]
    fn verified_hides_star_but_yields_to_emoji_status() {
        let verified = VerificationStatus {
            is_verified: true,
            ..NONE
        };
        assert_eq!(title_badge(verified, true, 0), Some(TitleBadge::Verified));
        assert_eq!(title_badge(verified, false, 0), Some(TitleBadge::Verified));
        assert_eq!(
            title_badge(verified, true, 5),
            Some(TitleBadge::EmojiStatus(5))
        );
    }

    #[test]
    fn scam_and_fake_win_and_carry_labels() {
        let scam = VerificationStatus {
            is_verified: true,
            is_scam: true,
            is_fake: true,
        };
        assert_eq!(title_badge(scam, true, 5), Some(TitleBadge::Scam));
        let fake = VerificationStatus {
            is_fake: true,
            ..NONE
        };
        assert_eq!(title_badge(fake, true, 0), Some(TitleBadge::Fake));
        assert_eq!(TitleBadge::Scam.label(), Some("SCAM"));
        assert_eq!(TitleBadge::Fake.label(), Some("FAKE"));
        assert_eq!(TitleBadge::Verified.label(), None);
    }
}

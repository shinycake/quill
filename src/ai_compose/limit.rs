//! The saved custom style limit (`TONES_SAVED_TOO_MANY`): tdesktop
//! `ShowAiComposeToneLimitError` (boxes/create_ai_tone_box.cpp).
use crate::premium_limits::LimitPair;

/// `premiumLimitTypeCustomTextCompositionStyleCount`
/// (`schema/td_api.tl:8733`). tdesktop's toast is the only place it shows,
/// so it is not one of the "Doubled limits" rows
/// ([`crate::premium_limits::PremiumLimitKind`]).
pub const CUSTOM_STYLES_LIMIT_TYPE: &str = "premiumLimitTypeCustomTextCompositionStyleCount";

/// tdesktop's values before the server answers (`data_premium_limits.cpp`
/// `aicompose_tone_saved_limit_default` 5, `_premium` 20).
pub const CUSTOM_STYLES_FALLBACK: LimitPair = LimitPair {
    default_value: 5,
    premium_value: 20,
};

/// What the toast says when no more styles can be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleLimitNotice {
    /// "You can save up to {count} styles. Delete one to add another."
    Final { count: i64 },
    /// The Premium promo: "Subscribe to Premium to save up to
    /// {premium_count} styles, or delete one of your {count} styles to add
    /// another."
    Promo { count: i64, premium_count: i64 },
}

/// `count` is the limit in force for this account
/// (`added_text_composition_style_count_max`), `premium_count` the Premium
/// value. A Premium account, or one that cannot buy Premium, gets the
/// final toast.
pub fn style_limit_message(
    count: i64,
    premium_count: i64,
    is_premium: bool,
    premium_possible: bool,
) -> StyleLimitNotice {
    if is_premium || !premium_possible {
        StyleLimitNotice::Final { count }
    } else {
        StyleLimitNotice::Promo {
            count,
            premium_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_notice_matches_tdesktop() {
        assert_eq!(
            style_limit_message(5, 20, false, true),
            StyleLimitNotice::Promo {
                count: 5,
                premium_count: 20
            }
        );
        assert_eq!(
            style_limit_message(20, 20, true, true),
            StyleLimitNotice::Final { count: 20 }
        );
        assert_eq!(
            style_limit_message(5, 20, false, false),
            StyleLimitNotice::Final { count: 5 }
        );
    }
}

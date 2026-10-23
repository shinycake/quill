//! A user's Stars rating (`userFullInfo.rating`, `pending_rating`) and
//! what tdesktop's rating box shows for it (`Ui::StarsRating`,
//! `ui/controls/stars_rating.cpp`).
#![deny(clippy::indexing_slicing)]

use serde_json::Value;

/// One `userRating` (schema `userRating level:int32
/// is_maximum_level_reached:Bool rating:int53 current_level_rating:int53
/// next_level_rating:int53`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StarsRating {
    /// Negative for an unreliable payer.
    pub level: i32,
    pub max_level_reached: bool,
    pub rating: i64,
    pub current_level_rating: i64,
    pub next_level_rating: i64,
}

/// The rating the current user will have on `date` (`pending_rating`,
/// `pending_rating_date`; TDLib sends it for the current user only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingRating {
    pub value: StarsRating,
    pub date: i32,
}

/// The ratings a `userFullInfo` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UserRatings {
    pub rating: Option<StarsRating>,
    pub pending: Option<PendingRating>,
}

fn int(value: &Value, key: &str) -> i64 {
    let field = value.get(key);
    field
        .and_then(Value::as_i64)
        .or_else(|| field.and_then(Value::as_str).and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

/// A `userRating` object; `None` for null or another constructor.
pub fn parse_user_rating(value: Option<&Value>) -> Option<StarsRating> {
    let value = value.filter(|v| v.get("@type").and_then(Value::as_str) == Some("userRating"))?;
    Some(StarsRating {
        level: i32::try_from(int(value, "level")).unwrap_or(0),
        max_level_reached: value
            .get("is_maximum_level_reached")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        rating: int(value, "rating"),
        current_level_rating: int(value, "current_level_rating"),
        next_level_rating: int(value, "next_level_rating"),
    })
}

/// The ratings of a `userFullInfo` object.
pub fn parse_user_ratings(info: Option<&Value>) -> UserRatings {
    let Some(info) = info else {
        return UserRatings::default();
    };
    let date = i32::try_from(int(info, "pending_rating_date")).unwrap_or(0);
    UserRatings {
        rating: parse_user_rating(info.get("rating")),
        pending: parse_user_rating(info.get("pending_rating"))
            .filter(|_| date > 0)
            .map(|value| PendingRating { value, date }),
    }
}

/// What the rating box's level bar shows (tdesktop `AdjustByReached`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RatingBar {
    /// The level at the bar's left end; `None` for a negative rating.
    pub left_level: Option<i32>,
    /// The level at the right end; `None` means "Negative Rating".
    pub right_level: Option<i32>,
    /// How full the bar is, 0..=1.
    pub ratio: f32,
    /// The bubble's count and the count of the next level (0 = none).
    pub stars: i64,
    pub next_stars: i64,
}

impl StarsRating {
    pub fn is_negative(&self) -> bool {
        self.level < 0
    }

    /// The level bar for this rating. At the top level the bar shows the
    /// previous level full, as tdesktop does.
    pub fn bar(&self) -> RatingBar {
        if self.is_negative() {
            return RatingBar {
                left_level: None,
                right_level: None,
                ratio: 0.5,
                stars: self.rating,
                next_stars: 0,
            };
        }
        let reached = self.max_level_reached || self.next_level_rating == 0;
        let (level, stars, this, next) = if reached {
            let top = self.rating.max(self.current_level_rating).max(1);
            (self.level.saturating_sub(1), top, 0, top)
        } else {
            let stars = self.current_level_rating.max(self.rating);
            (
                self.level,
                stars,
                self.current_level_rating,
                self.next_level_rating.max(stars.saturating_add(1)),
            )
        };
        let span = next.saturating_sub(this);
        let ratio = if span > 0 {
            (stars.saturating_sub(this) as f64 / span as f64).clamp(0., 1.) as f32
        } else {
            1.
        };
        RatingBar {
            left_level: Some(level),
            right_level: Some(level.saturating_add(1)),
            ratio,
            stars,
            next_stars: if reached { 0 } else { next },
        }
    }
}

/// A count on the rating box's bubble, shortened like tdesktop's
/// `BubbleTextFactory` once the larger count (`scale`) reaches 10,000.
pub fn bubble_count(count: i64, scale: i64) -> String {
    let short = |value: i64, unit: i64, suffix: char| {
        let tenths = value / (unit / 10);
        let whole = crate::premium_hub::group_digits(tenths / 10);
        match tenths % 10 {
            0 => format!("{whole}{suffix}"),
            fraction => format!("{whole}.{}{suffix}", fraction.abs()),
        }
    };
    if scale < 10_000 {
        crate::premium_hub::group_digits(count)
    } else if scale < 10_000_000 {
        short(count, 1_000, 'K')
    } else {
        short(count, 1_000_000, 'M')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rating(level: i32, rating: i64, this: i64, next: i64, max: bool) -> StarsRating {
        StarsRating {
            level,
            max_level_reached: max,
            rating,
            current_level_rating: this,
            next_level_rating: next,
        }
    }

    #[test]
    fn parses_rating_and_pending() {
        let info: Value = serde_json::from_str(
            r#"{"rating":{"@type":"userRating","level":3,"is_maximum_level_reached":false,"rating":1500,"current_level_rating":1000,"next_level_rating":2000},"pending_rating":{"@type":"userRating","level":4,"is_maximum_level_reached":false,"rating":2100,"current_level_rating":2000,"next_level_rating":3000},"pending_rating_date":1800000000}"#,
        )
        .unwrap_or_default();
        let parsed = parse_user_ratings(Some(&info));
        assert_eq!(parsed.rating, Some(rating(3, 1500, 1000, 2000, false)));
        let pending = parsed.pending.unwrap_or(PendingRating {
            value: StarsRating::default(),
            date: 0,
        });
        assert_eq!(pending.value.level, 4);
        assert_eq!(pending.date, 1_800_000_000);
    }

    #[test]
    fn null_rating_is_none() {
        let info: Value = serde_json::from_str(
            r#"{"rating":null,"pending_rating":null,"pending_rating_date":0}"#,
        )
        .unwrap_or_default();
        assert_eq!(parse_user_ratings(Some(&info)), UserRatings::default());
        assert_eq!(parse_user_ratings(None), UserRatings::default());
    }

    #[test]
    fn pending_without_date_is_dropped() {
        let info: Value = serde_json::from_str(
            r#"{"pending_rating":{"@type":"userRating","level":1,"is_maximum_level_reached":false,"rating":5,"current_level_rating":0,"next_level_rating":10},"pending_rating_date":0}"#,
        )
        .unwrap_or_default();
        assert_eq!(parse_user_ratings(Some(&info)).pending, None);
    }

    #[test]
    fn bar_fills_between_levels() {
        let bar = rating(3, 1500, 1000, 2000, false).bar();
        assert_eq!(bar.left_level, Some(3));
        assert_eq!(bar.right_level, Some(4));
        assert!((bar.ratio - 0.5).abs() < 1e-6);
        assert_eq!(bar.next_stars, 2000);
    }

    #[test]
    fn top_level_shows_previous_level_full() {
        let bar = rating(90, 5000, 4000, 0, true).bar();
        assert_eq!(bar.left_level, Some(89));
        assert_eq!(bar.right_level, Some(90));
        assert!((bar.ratio - 1.).abs() < 1e-6);
        assert_eq!(bar.next_stars, 0);
    }

    #[test]
    fn negative_rating_sits_mid_bar() {
        let bar = rating(-1, -20, 0, 0, false).bar();
        assert_eq!(bar.left_level, None);
        assert_eq!(bar.right_level, None);
        assert!((bar.ratio - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bubble_counts_shorten_for_large_scales() {
        assert_eq!(bubble_count(1500, 2000), "1,500");
        assert_eq!(bubble_count(12_345, 20_000), "12.3K");
        assert_eq!(bubble_count(12_000, 20_000), "12K");
        assert_eq!(bubble_count(20_000_000, 20_000_000), "20M");
    }
}

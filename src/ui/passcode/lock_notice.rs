//! The line under the lock screen's field: nothing, what the last try got
//! wrong, or the flood wait that disables Unlock until it ends.
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum LockNotice {
    #[default]
    None,
    /// A wrong or missing passcode, or another failure: typing clears it.
    Wrong(String),
    /// "Too many tries": typing leaves it, as it explains the disabled
    /// button; the countdown ends it.
    Flood { until: Instant, text: String },
}

impl LockNotice {
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Wrong(text) | Self::Flood { text, .. } => Some(text),
        }
    }

    pub(crate) fn flood_until(&self) -> Option<Instant> {
        match self {
            Self::Flood { until, .. } => Some(*until),
            Self::None | Self::Wrong(_) => None,
        }
    }

    /// The user typed: a wrong-passcode message goes, a flood wait stays.
    /// Returns whether the line changed.
    pub(crate) fn typed(&mut self) -> bool {
        if matches!(self, Self::Wrong(_)) {
            *self = Self::None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LockNotice;
    use std::time::Instant;

    #[test]
    fn typing_clears_a_wrong_passcode_but_not_the_flood_wait() {
        let mut notice = LockNotice::Wrong("Incorrect passcode".into());
        assert!(notice.typed());
        assert_eq!(notice, LockNotice::None);

        let until = Instant::now();
        let mut flood = LockNotice::Flood {
            until,
            text: "Too many tries".into(),
        };
        assert!(!flood.typed());
        assert_eq!(flood.text(), Some("Too many tries"));
        assert_eq!(flood.flood_until(), Some(until));
    }
}

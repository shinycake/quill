//! A box that focuses a field once, after its dialog has mounted (the
//! dialog focuses itself as it opens, so the field asks on the first draw
//! after that). One value, consumed by the box that owns the field.
#![deny(clippy::indexing_slicing)]

/// Where a box is between opening and its first draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mount {
    /// Opened; the first draw has not run.
    Mounting,
    /// The first draw ran: nothing left to focus.
    #[default]
    Shown,
}

impl Mount {
    /// The first draw asks: true once, when the box was still mounting.
    pub(crate) fn take_first_draw(&mut self) -> bool {
        std::mem::replace(self, Self::Shown) == Self::Mounting
    }
}

#[cfg(test)]
mod tests {
    use super::Mount;

    #[test]
    fn only_the_first_draw_after_opening_focuses() {
        let mut mount = Mount::Mounting;
        assert!(mount.take_first_draw());
        assert!(!mount.take_first_draw());
        assert_eq!(mount, Mount::Shown);
    }
}

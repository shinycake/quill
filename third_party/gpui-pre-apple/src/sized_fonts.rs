//! QUILL: one CTFont per font and size for `layout_line`.
//!
//! A new CTFont starts with empty shaping and fallback caches, so building one
//! for every run rebuilt them for every line that missed GPUI's two-frame line
//! cache. Std-only, so `scripts/test-sized-fonts.sh` runs these tests anywhere.

use std::collections::HashMap;
use std::hash::Hash;

/// Values built on first use and kept. Past `limit` keys it starts over, so a
/// tween through font sizes costs what building a font each time does, and no
/// memory.
pub(crate) struct SizedFonts<K, V> {
    built: HashMap<K, V>,
    limit: usize,
}

impl<K: Eq + Hash, V> SizedFonts<K, V> {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            built: HashMap::new(),
            limit,
        }
    }

    pub(crate) fn get_or_build(&mut self, key: K, build: impl FnOnce() -> V) -> &V {
        if self.built.len() >= self.limit && !self.built.contains_key(&key) {
            self.built.clear();
        }
        self.built.entry(key).or_insert_with(build)
    }
}

#[cfg(test)]
mod tests {
    use super::SizedFonts;
    use std::cell::Cell;

    #[test]
    fn a_font_at_a_size_is_built_once() {
        let builds = Cell::new(0);
        let mut fonts = SizedFonts::new(8);
        for _ in 0..5 {
            let font = fonts.get_or_build(("Inter", 16), || {
                builds.set(builds.get() + 1);
                "Inter at 16"
            });
            assert_eq!(*font, "Inter at 16");
        }
        assert_eq!(builds.get(), 1);
    }

    #[test]
    fn another_font_or_size_is_another_entry() {
        let mut fonts = SizedFonts::new(8);
        fonts.get_or_build(("Inter", 16), || "Inter at 16");
        assert_eq!(
            *fonts.get_or_build(("Inter", 17), || "Inter at 17"),
            "Inter at 17"
        );
        assert_eq!(
            *fonts.get_or_build(("Menlo", 16), || "Menlo at 16"),
            "Menlo at 16"
        );
    }

    #[test]
    fn a_size_tween_stays_within_the_limit() {
        let mut fonts = SizedFonts::new(8);
        for size in 0..1000 {
            assert_eq!(*fonts.get_or_build(size, || size * 2), size * 2);
            assert!(fonts.built.len() <= 8, "{} fonts", fonts.built.len());
        }
    }

    #[test]
    fn a_full_cache_keeps_what_is_asked_for_again() {
        let builds = Cell::new(0);
        let mut fonts = SizedFonts::new(2);
        fonts.get_or_build(1, || 1);
        fonts.get_or_build(2, || 2);
        for _ in 0..5 {
            fonts.get_or_build(2, || {
                builds.set(builds.get() + 1);
                2
            });
        }
        assert_eq!(builds.get(), 0, "a hit on a full cache clears nothing");
    }
}

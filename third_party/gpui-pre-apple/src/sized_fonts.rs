//! QUILL: one CTFont per font and size for `layout_line`.

use std::collections::HashMap;
use std::hash::Hash;

pub(crate) struct SizedFonts<K, V> {
    /// Each font with the lookup that last asked for it.
    built: HashMap<K, (V, u64)>,
    limit: usize,
    lookups: u64,
}

impl<K: Eq + Hash, V> SizedFonts<K, V> {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            built: HashMap::new(),
            limit,
            lookups: 0,
        }
    }

    pub(crate) fn get_or_build(&mut self, key: K, build: impl FnOnce() -> V) -> &V {
        self.lookups += 1;
        if self.built.len() >= self.limit && !self.built.contains_key(&key) {
            let oldest = self.built.values().map(|(_, used)| *used).min();
            self.built.retain(|_, (_, used)| Some(*used) != oldest);
        }
        let (font, used) = self.built.entry(key).or_insert_with(|| (build(), 0));
        *used = self.lookups;
        font
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
    fn a_full_cache_drops_the_font_unused_longest() {
        let builds = Cell::new(0);
        let mut fonts = SizedFonts::new(2);
        let mut ask = |fonts: &mut SizedFonts<i32, i32>, key| {
            *fonts.get_or_build(key, || {
                builds.set(builds.get() + 1);
                key
            })
        };
        ask(&mut fonts, 1);
        ask(&mut fonts, 2);
        ask(&mut fonts, 1);
        ask(&mut fonts, 3);
        ask(&mut fonts, 1);
        assert_eq!(builds.get(), 3, "the font in use survives the limit");
        ask(&mut fonts, 2);
        assert_eq!(builds.get(), 4, "the idle font went");
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

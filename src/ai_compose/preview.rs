//! The style link and the style preview's examples (tdesktop
//! `boxes/preview_ai_tone_box.cpp`, `PreviewAiToneBox`).

/// tdesktop's example count before the server says
/// (`aicompose_tone_examples_num`, default 3).
const EXAMPLE_COUNT_FALLBACK: usize = 3;

/// `https://t.me/addstyle/<name>` (tdesktop
/// `createInternalLinkFull("addstyle/" + slug)`); `None` for a style
/// without a name.
pub fn style_link(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| format!("https://t.me/addstyle/{name}"))
}

/// The preview's examples: how many were shown and whether the next one is
/// loading. Only one shows at a time; a new one replaces it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StyleExamples {
    shown: usize,
    loading: bool,
}

impl StyleExamples {
    /// The style came with its English example (`firstExample`), which
    /// counts as the first one shown.
    pub fn new(has_first: bool) -> Self {
        Self {
            shown: usize::from(has_first),
            loading: false,
        }
    }

    /// The example number to ask for next, or `None` while one is loading.
    pub fn start_next(&mut self) -> Option<usize> {
        if self.loading {
            return None;
        }
        self.loading = true;
        Some(self.shown)
    }

    pub fn loaded(&mut self) {
        self.loading = false;
        self.shown += 1;
    }

    pub fn failed(&mut self) {
        self.loading = false;
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    /// "Another example" shows while fewer than `max` examples were shown.
    pub fn can_show_another(&self, max: Option<usize>) -> bool {
        self.shown < max.unwrap_or(EXAMPLE_COUNT_FALLBACK)
    }
}

#[cfg(test)]
mod tests {
    use super::{StyleExamples, style_link};

    #[test]
    fn style_link_needs_a_name() {
        assert_eq!(
            style_link("pirate").as_deref(),
            Some("https://t.me/addstyle/pirate")
        );
        assert_eq!(style_link("  "), None);
    }

    #[test]
    fn a_style_without_an_example_loads_number_zero() {
        let mut examples = StyleExamples::new(false);
        assert_eq!(examples.start_next(), Some(0));
        assert_eq!(examples.start_next(), None);
        examples.loaded();
        assert_eq!(examples.start_next(), Some(1));
    }

    #[test]
    fn the_first_example_counts_toward_the_limit() {
        let mut examples = StyleExamples::new(true);
        assert!(examples.can_show_another(Some(2)));
        assert_eq!(examples.start_next(), Some(1));
        examples.loaded();
        assert!(!examples.can_show_another(Some(2)));
        assert!(examples.can_show_another(None));
    }

    #[test]
    fn a_failed_example_can_be_asked_again() {
        let mut examples = StyleExamples::new(true);
        assert_eq!(examples.start_next(), Some(1));
        examples.failed();
        assert!(!examples.loading());
        assert_eq!(examples.start_next(), Some(1));
    }
}

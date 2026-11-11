//! The style link and the style preview's examples (tdesktop
//! `boxes/preview_ai_tone_box.cpp`, `PreviewAiToneBox`).

use std::time::Instant;

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

/// Whether an example is being asked for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ExampleFetch {
    #[default]
    Idle,
    /// The request with this token runs since `at` (the refresh icon spins
    /// from there); only its answer ends it.
    Asked { token: u64, at: Instant },
}

/// The preview's examples: how many were shown and the request for the
/// next one. Only one shows at a time; a new one replaces it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StyleExamples {
    shown: usize,
    fetch: ExampleFetch,
}

impl StyleExamples {
    /// The style came with its English example (`firstExample`), which
    /// counts as the first one shown.
    pub fn new(has_first: bool) -> Self {
        Self {
            shown: usize::from(has_first),
            fetch: ExampleFetch::Idle,
        }
    }

    /// The example number to ask for next, as the request `token` sent at
    /// `now`, or `None` while one is loading.
    pub fn start_next(&mut self, token: u64, now: Instant) -> Option<usize> {
        if self.loading() {
            return None;
        }
        self.fetch = ExampleFetch::Asked { token, at: now };
        Some(self.shown)
    }

    /// The answer of the request with `token`: counts as shown. `false`
    /// (and nothing changes) when another request is the one awaited.
    pub fn loaded(&mut self, token: u64) -> bool {
        if self.token() != Some(token) {
            return false;
        }
        self.fetch = ExampleFetch::Idle;
        self.shown += 1;
        true
    }

    /// The request with `token` failed. `false` when another is awaited.
    pub fn failed(&mut self, token: u64) -> bool {
        if self.token() != Some(token) {
            return false;
        }
        self.fetch = ExampleFetch::Idle;
        true
    }

    pub fn loading(&self) -> bool {
        self.token().is_some()
    }

    /// The token of the request awaited.
    pub fn token(&self) -> Option<u64> {
        match self.fetch {
            ExampleFetch::Asked { token, .. } => Some(token),
            ExampleFetch::Idle => None,
        }
    }

    /// When the request awaited was sent.
    pub fn asked_at(&self) -> Option<Instant> {
        match self.fetch {
            ExampleFetch::Asked { at, .. } => Some(at),
            ExampleFetch::Idle => None,
        }
    }

    /// "Another example" shows while fewer than `max` examples were shown.
    pub fn can_show_another(&self, max: Option<usize>) -> bool {
        self.shown < max.unwrap_or(EXAMPLE_COUNT_FALLBACK)
    }
}

#[cfg(test)]
mod tests {
    use super::{StyleExamples, style_link};
    use std::time::Instant;

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
        assert_eq!(examples.start_next(1, Instant::now()), Some(0));
        assert_eq!(examples.start_next(1, Instant::now()), None);
        examples.loaded(1);
        assert_eq!(examples.start_next(1, Instant::now()), Some(1));
    }

    #[test]
    fn the_first_example_counts_toward_the_limit() {
        let mut examples = StyleExamples::new(true);
        assert!(examples.can_show_another(Some(2)));
        assert_eq!(examples.start_next(1, Instant::now()), Some(1));
        examples.loaded(1);
        assert!(!examples.can_show_another(Some(2)));
        assert!(examples.can_show_another(None));
    }

    #[test]
    fn a_failed_example_can_be_asked_again() {
        let mut examples = StyleExamples::new(true);
        assert_eq!(examples.start_next(1, Instant::now()), Some(1));
        examples.failed(1);
        assert!(!examples.loading());
        assert_eq!(examples.start_next(1, Instant::now()), Some(1));
    }

    #[test]
    fn only_the_awaited_requests_answer_counts() {
        let now = Instant::now();
        let mut examples = StyleExamples::new(true);
        assert_eq!(examples.start_next(7, now), Some(1));
        assert_eq!(examples.token(), Some(7));
        assert_eq!(examples.asked_at(), Some(now));
        assert!(!examples.loaded(6), "an older request's answer");
        assert!(!examples.failed(6));
        assert!(examples.loading());
        assert!(examples.failed(7));
        assert!(!examples.loading() && examples.asked_at().is_none());
        assert!(!examples.loaded(7), "nothing is awaited now");
    }
}

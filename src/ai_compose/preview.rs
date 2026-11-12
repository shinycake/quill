//! The style link and the style preview's examples (tdesktop
//! `boxes/preview_ai_tone_box.cpp`, `PreviewAiToneBox`).

use crate::ids::RequestId;
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
    /// The request runs since `at` (the refresh icon spins from there);
    /// only its answer ends it.
    Asked { request: RequestId, at: Instant },
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

    /// The example number to ask for next, or `None` while one is loading.
    pub fn next_number(&self) -> Option<usize> {
        (!self.loading()).then_some(self.shown)
    }

    /// The example asked for went out as `request` at `now`.
    pub fn begin(&mut self, request: RequestId, now: Instant) {
        self.fetch = ExampleFetch::Asked { request, at: now };
    }

    /// The answer of `request`: counts as shown. `false` (and nothing
    /// changes) when another request is the one awaited.
    pub fn loaded(&mut self, request: RequestId) -> bool {
        if self.request() != Some(request) {
            return false;
        }
        self.fetch = ExampleFetch::Idle;
        self.shown += 1;
        true
    }

    /// `request` failed. `false` when another is awaited.
    pub fn failed(&mut self, request: RequestId) -> bool {
        if self.request() != Some(request) {
            return false;
        }
        self.fetch = ExampleFetch::Idle;
        true
    }

    pub fn loading(&self) -> bool {
        self.request().is_some()
    }

    /// The request awaited.
    pub fn request(&self) -> Option<RequestId> {
        match self.fetch {
            ExampleFetch::Asked { request, .. } => Some(request),
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
    use crate::ids::RequestId;
    use std::time::Instant;

    fn ask(examples: &mut StyleExamples, request: u64) -> Option<usize> {
        let number = examples.next_number()?;
        examples.begin(RequestId(request), Instant::now());
        Some(number)
    }

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
        assert_eq!(ask(&mut examples, 1), Some(0));
        assert_eq!(ask(&mut examples, 1), None);
        examples.loaded(RequestId(1));
        assert_eq!(ask(&mut examples, 1), Some(1));
    }

    #[test]
    fn the_first_example_counts_toward_the_limit() {
        let mut examples = StyleExamples::new(true);
        assert!(examples.can_show_another(Some(2)));
        assert_eq!(ask(&mut examples, 1), Some(1));
        examples.loaded(RequestId(1));
        assert!(!examples.can_show_another(Some(2)));
        assert!(examples.can_show_another(None));
    }

    #[test]
    fn a_failed_example_can_be_asked_again() {
        let mut examples = StyleExamples::new(true);
        assert_eq!(ask(&mut examples, 1), Some(1));
        examples.failed(RequestId(1));
        assert!(!examples.loading());
        assert_eq!(ask(&mut examples, 1), Some(1));
    }

    #[test]
    fn only_the_awaited_requests_answer_counts() {
        let now = Instant::now();
        let mut examples = StyleExamples::new(true);
        assert_eq!(examples.next_number(), Some(1));
        examples.begin(RequestId(7), now);
        assert_eq!(examples.request(), Some(RequestId(7)));
        assert_eq!(examples.asked_at(), Some(now));
        assert!(!examples.loaded(RequestId(6)), "an older request's answer");
        assert!(!examples.failed(RequestId(6)));
        assert!(examples.loading());
        assert!(examples.failed(RequestId(7)));
        assert!(!examples.loading() && examples.asked_at().is_none());
        assert!(!examples.loaded(RequestId(7)), "nothing is awaited now");
    }
}

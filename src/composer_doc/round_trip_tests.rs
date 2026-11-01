//! Random marker-heavy markup through the writer and back. The generator is
//! seeded, so a failure repeats.

use super::*;

const PIECES: [&str; 34] = [
    "word",
    "a",
    "b",
    " ",
    "  ",
    "\n",
    "\n\n",
    "\r\n",
    "\n> ",
    "> ",
    "**",
    "*",
    "_",
    "__",
    "~~",
    "||",
    "`",
    "```",
    "```rust\n",
    "[",
    "](",
    "](https://x.y/z)",
    ")",
    "![",
    "😀",
    "日本",
    ".",
    ", ",
    "(",
    "tg://emoji?id=7)",
    "tg://user?id=12)",
    "https://x.y",
    "**bold**",
    "_it_",
];

pub(crate) struct Random(u64);

impl Random {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn below(&mut self, bound: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        usize::try_from(x % bound as u64).expect("below the bound")
    }

    pub(crate) fn markup(&mut self, pieces: usize) -> String {
        (0..pieces)
            .map(|_| PIECES[self.below(PIECES.len())])
            .collect()
    }
}

/// `doc` with spans cut at line breaks and code blocks, as markup writes them.
fn as_written(doc: &ComposerDoc) -> ComposerDoc {
    let mut lines = doc.clone();
    let blocks: Vec<DocSpan> = doc
        .spans
        .iter()
        .filter(|span| matches!(span.tag, ComposerTag::Pre(_)))
        .cloned()
        .collect();
    let outside_blocks = blocks.iter().fold(doc.spans.clone(), |spans, block| {
        carve(spans, &block.range, |tag| {
            !matches!(tag, ComposerTag::Pre(_))
        })
    });
    lines.spans = outside_blocks
        .iter()
        .flat_map(|span| {
            if span.tag == ComposerTag::Quote || matches!(span.tag, ComposerTag::Pre(_)) {
                return vec![span.clone()];
            }
            let mut start = span.range.start;
            let mut pieces = Vec::new();
            let body = doc.text.get(span.range.clone()).unwrap_or_default();
            for (ix, _) in body.match_indices('\n') {
                pieces.push(DocSpan::new(start..span.range.start + ix, span.tag.clone()));
                start = span.range.start + ix + 1;
            }
            pieces.push(DocSpan::new(start..span.range.end, span.tag.clone()));
            pieces
        })
        .collect();
    lines.normalize();
    lines
}

#[test]
fn a_parsed_document_comes_back_from_its_markup() {
    let mut random = Random::new(0x9e37_79b9_7f4a_7c15);
    let mut failed = Vec::new();
    for _ in 0..60_000 {
        let pieces = 1 + random.below(40);
        let input = random.markup(pieces);
        let doc = ComposerDoc::from_markup(&input);
        let markup = doc.to_final_markup();
        let back = ComposerDoc::from_markup(&markup);
        if as_written(&back) != as_written(&doc) {
            failed.push(format!("{input:?}\n  {doc:?}\n  {markup:?}\n  {back:?}"));
        }
    }
    let count = failed.len();
    failed.truncate(8);
    assert!(count == 0, "{count} failed\n{}", failed.join("\n"));
}

#[test]
fn a_marker_never_pairs_with_one_in_the_text() {
    let doc = ComposerDoc::from_markup("*[_日本https://x.y_\r\na");
    assert_eq!(doc.text, "*[日本https://x.y\r\na");
    assert_eq!(ComposerDoc::from_markup(&doc.to_markup()), doc);
}

#[test]
fn a_link_keeps_its_address_whatever_it_holds() {
    let mut doc = ComposerDoc::plain("see this page");
    let url = "https://x.y/a_(b)_*c*".to_string();
    doc.spans = vec![
        DocSpan::new(0..13, ComposerTag::Italic),
        DocSpan::new(4..8, ComposerTag::Link(url)),
    ];
    let back = ComposerDoc::from_markup(&doc.to_markup());
    let written = "https://x.y/a_(b%29_*c*".to_string();
    assert_eq!(back.text, doc.text);
    assert_eq!(
        back.spans,
        vec![
            DocSpan::new(0..13, ComposerTag::Italic),
            DocSpan::new(4..8, ComposerTag::Link(written)),
        ]
    );
}

#[test]
fn markers_typed_in_a_field_stay_markdown() {
    let doc = ComposerDoc::plain("*a* and **b**");
    assert_eq!(doc.to_markup(), "*a* and **b**");
}

#[test]
fn nothing_in_final_text_reads_as_a_marker() {
    for text in ["*a*", "> quote", "`code`", "[a](b)", "~~a~~", "```\na\n```"] {
        let doc = ComposerDoc::plain(text);
        assert_eq!(ComposerDoc::from_markup(&doc.to_final_markup()), doc);
    }
}

#[test]
fn spans_inside_an_italic_span_read_back_wherever_they_start() {
    const WORDS: &str = "aa bb cc dd ee";
    let word = |at: usize| at * 3;
    let span = |from: usize, to: usize, tag: ComposerTag| {
        DocSpan::new(word(from)..word(to) - 1, tag)
    };
    let mut failed = Vec::new();
    for bold in 0..4 {
        for bold_end in bold + 1..=5 {
            for underline in 0..4 {
                for underline_end in underline + 1..=5 {
                    let mut doc = ComposerDoc::plain(WORDS);
                    doc.spans = vec![
                        span(0, 5, ComposerTag::Italic),
                        span(bold, bold_end, ComposerTag::Bold),
                        span(underline, underline_end, ComposerTag::Underline),
                    ];
                    doc.normalize();
                    let markup = doc.to_markup();
                    let back = ComposerDoc::from_markup(&markup);
                    if back != doc {
                        failed.push(format!("{markup:?}\n  {doc:?}\n  {back:?}"));
                    }
                }
            }
        }
    }
    let count = failed.len();
    failed.truncate(8);
    assert!(count == 0, "{count} failed\n{}", failed.join("\n"));
}

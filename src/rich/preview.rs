//! The rich editor's preview, kept across revisions of the text. An edit
//! touches one block, and the blocks around it read the same as before, so
//! only the ones between the shared start and the shared end are parsed.
use super::*;
use std::ops::Range;
use std::rc::Rc;

/// `shown` is `source` through [`preview_block`], index for index.
#[derive(Default)]
pub struct PreviewBlocks {
    source: Vec<RichBlock>,
    shown: Vec<Rc<RichBlock>>,
}

impl PreviewBlocks {
    pub fn shown(&self) -> &[Rc<RichBlock>] {
        &self.shown
    }

    /// Move to `text`. Returns the range of old blocks that were replaced
    /// and how many new ones took their place.
    pub fn update(&mut self, text: &str) -> (Range<usize>, usize) {
        let source = markup_to_blocks(text);
        let start = self
            .source
            .iter()
            .zip(&source)
            .take_while(|(old, new)| old == new)
            .count();
        let old_rest = self.source.get(start..).unwrap_or_default();
        let new_rest = source.get(start..).unwrap_or_default();
        let shared_end = old_rest
            .iter()
            .rev()
            .zip(new_rest.iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        let replaced = start..start + old_rest.len() - shared_end;
        let added = new_rest.len() - shared_end;
        let parsed: Vec<Rc<RichBlock>> = new_rest
            .iter()
            .take(added)
            .cloned()
            .map(|block| Rc::new(preview_block(block)))
            .collect();
        self.shown.splice(replaced.clone(), parsed);
        self.source = source;
        (replaced, added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: [&str; 8] = [
        "# **Title**",
        "- first ~~point~~",
        "1. **second**",
        "plain _text_",
        "",
        "---",
        ">> More",
        "- [x] done",
    ];

    #[test]
    fn every_edit_shows_what_a_fresh_parse_shows() {
        let mut preview = PreviewBlocks::default();
        let mut lines: Vec<&str> = Vec::new();
        let mut seed = 7usize;
        for step in 0..4_000 {
            seed = seed.wrapping_mul(31).wrapping_add(step) % 1_000_003;
            let at = seed % (lines.len() + 1);
            let line = LINES.get(seed % LINES.len()).copied().unwrap_or_default();
            match seed % 3 {
                0 if at < lines.len() => {
                    lines.remove(at);
                }
                1 if at < lines.len() => {
                    if let Some(slot) = lines.get_mut(at) {
                        *slot = line;
                    }
                }
                _ => lines.insert(at, line),
            }
            let text = lines.join("\n");
            let before = preview.shown().len();
            let (replaced, added) = preview.update(&text);
            let shown: Vec<RichBlock> = preview.shown().iter().map(|b| (**b).clone()).collect();
            assert_eq!(shown, preview_blocks(&text), "{text:?}");
            assert_eq!(before - replaced.len() + added, shown.len());
        }
    }

    #[test]
    fn an_edit_parses_only_the_block_it_touches() {
        let mut preview = PreviewBlocks::default();
        preview.update("# a\n\n- b\n\nc\n\n---");
        let before = preview.shown().to_vec();
        let (replaced, added) = preview.update("# a\n\n- bb\n\nc\n\n---");
        assert_eq!((replaced, added), (1..2, 1));
        for (index, block) in before.iter().enumerate() {
            let kept = preview
                .shown()
                .get(index)
                .is_some_and(|b| Rc::ptr_eq(b, block));
            assert_eq!(kept, index != 1, "block {index}");
        }
    }
}

//! Pure decisions behind the folder tab strip fitting its sidebar: which
//! edges show a fade and when the selected tab must be scrolled into view.
#![deny(clippy::indexing_slicing)]

/// Width of the fade drawn over a clipped edge of the strip.
pub(super) const FADE_WIDTH: f32 = 28.;
/// Scroll distance below which an edge counts as reached.
const EDGE_EPSILON: f32 = 0.5;

/// Which edges of the scrolling strip hide tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) struct FadeEdges {
    pub start: bool,
    pub end: bool,
}

/// `offset` is the scroll offset (zero or negative while scrolled right) and
/// `max_offset` the overflow width (zero when everything fits).
pub(super) fn fade_edges(offset: f32, max_offset: f32) -> FadeEdges {
    if max_offset <= EDGE_EPSILON {
        return FadeEdges::default();
    }
    let scrolled = (-offset).clamp(0., max_offset);
    FadeEdges {
        start: scrolled > EDGE_EPSILON,
        end: max_offset - scrolled > EDGE_EPSILON,
    }
}

/// What the last reveal was computed for. The selected tab is scrolled into
/// view only when this changes, so the user's own scrolling is not undone on
/// every render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RevealKey {
    pub selected: usize,
    pub tab_count: usize,
    pub strip_width: i32,
}

pub(super) fn needs_reveal(last: Option<RevealKey>, now: RevealKey) -> bool {
    last != Some(now)
}

#[cfg(test)]
mod tests {
    use super::{FadeEdges, RevealKey, fade_edges, needs_reveal};

    #[test]
    fn no_fade_when_everything_fits() {
        assert_eq!(fade_edges(0., 0.), FadeEdges::default());
    }

    #[test]
    fn fades_follow_scroll_position() {
        assert_eq!(
            fade_edges(0., 120.),
            FadeEdges {
                start: false,
                end: true
            }
        );
        assert_eq!(
            fade_edges(-60., 120.),
            FadeEdges {
                start: true,
                end: true
            }
        );
        assert_eq!(
            fade_edges(-120., 120.),
            FadeEdges {
                start: true,
                end: false
            }
        );
        // Overscroll is clamped.
        assert!(!fade_edges(-200., 120.).end);
    }

    #[test]
    fn reveal_only_when_inputs_change() {
        let key = RevealKey {
            selected: 2,
            tab_count: 5,
            strip_width: 240,
        };
        assert!(needs_reveal(None, key));
        assert!(!needs_reveal(Some(key), key));
        assert!(needs_reveal(Some(key), RevealKey { selected: 3, ..key }));
        assert!(needs_reveal(
            Some(key),
            RevealKey {
                strip_width: 300,
                ..key
            }
        ));
    }
}

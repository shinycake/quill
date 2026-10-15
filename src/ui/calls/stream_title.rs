//! Where the live stream's title line goes and how much of it fits:
//! tdesktop `Panel::computeTitleRect` and `refreshTitleGeometry`.
//!
//! The line is the title, a 4 px gap, a bullet, a 4 px gap and the viewer
//! count, centered on the window. When it does not fit its rect, the
//! longer of the two texts gives way first until both are as wide, then
//! both shrink equally. A line that cannot be centered inside the rect is
//! pushed against its nearer edge.
#![deny(clippy::indexing_slicing)]

/// `groupCallTitleSeparator`.
pub(crate) const SEPARATOR_GAP: f32 = 4.;
/// macOS: the traffic lights take the first 70 px (`computeTitleRect`).
const MAC_LEFT: f32 = 70.;

/// The title line's place: its left edge and the two texts' widths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TitleLayout {
    pub(crate) left: f32,
    pub(crate) title: f32,
    pub(crate) viewers: f32,
}

/// The rect the line may use: (left, width). `pin` is the pin button's
/// width when it shows.
pub(crate) fn title_rect(window_width: f32, pin: f32, mac: bool) -> (f32, f32) {
    let remove = SEPARATOR_GAP + pin;
    let left = if mac { MAC_LEFT } else { SEPARATOR_GAP };
    (left, (window_width - remove - left).max(0.))
}

/// Lay the line out in `rect` (left, width) of a window `window_width`
/// wide, from the texts' natural widths.
pub(crate) fn title_layout(
    window_width: f32,
    rect: (f32, f32),
    title: f32,
    bullet: f32,
    viewers: f32,
) -> TitleLayout {
    let (left, width) = rect;
    let separator = bullet + 2. * SEPARATOR_GAP;
    let best = title + separator + viewers;
    let not_enough = (best - width).max(0.);
    let viewers_not_enough = (viewers - title).clamp(0., not_enough)
        + ((not_enough - (viewers - title).abs()) / 2.).max(0.);
    let title_width = (title - (not_enough - viewers_not_enough)).max(0.);
    let viewers_width = (viewers - viewers_not_enough).max(0.);
    let from = (window_width - best) / 2.;
    let start = if from >= left && from + best <= left + width {
        from
    } else if width < best || from < left {
        left
    } else {
        left + width - best
    };
    TitleLayout {
        left: start,
        title: title_width,
        viewers: viewers_width,
    }
}

#[cfg(test)]
mod tests {
    use super::{TitleLayout, title_layout, title_rect};

    #[test]
    fn a_line_that_fits_is_centered_on_the_window() {
        let rect = title_rect(720., 40., true);
        assert_eq!(rect, (70., 606.));
        let layout = title_layout(720., rect, 100., 6., 80.);
        assert_eq!(
            layout,
            TitleLayout {
                left: (720. - 194.) / 2.,
                title: 100.,
                viewers: 80.,
            }
        );
    }

    #[test]
    fn the_longer_text_gives_way_first_then_both() {
        // 14 px short: the longer title takes all of it.
        let layout = title_layout(300., (0., 280.), 200., 6., 80.);
        assert_eq!((layout.title, layout.viewers), (186., 80.));
        assert_eq!(layout.left, 0.);
        // 194 px short: 120 to even them, then 37 each.
        let layout = title_layout(100., (0., 100.), 200., 6., 80.);
        assert_eq!((layout.title, layout.viewers), (43., 43.));
    }

    #[test]
    fn a_line_off_center_is_pushed_inside_the_rect() {
        // Centered it would start at 30, left of the macOS rect.
        let layout = title_layout(300., (70., 226.), 150., 6., 76.);
        assert_eq!(layout.left, 70.);
    }
}

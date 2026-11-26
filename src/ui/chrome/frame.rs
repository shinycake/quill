//! Where the conversation's pills and rows sit in Floating: a pure function
//! of the pane's measured width and the pills' measured heights
//! (docs/decisions/floating-layout.md). Classic passes
//! [`HistoryInsets::ZERO`] everywhere, so every consumer has one code path.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{AnyElement, Bounds, IntoElement, ParentElement, Pixels, Styled, div, px};

/// What the frame is computed from, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct FrameInput {
    /// The history pane's measured width.
    pub pane: f32,
    pub gutter: f32,
    pub bar_gap: f32,
    /// The widest conversation column.
    pub chat_column: f32,
    /// The Header slice's measured height.
    pub top: f32,
    /// The Composer slice's measured height.
    pub composer: f32,
}

/// The column the pills and the rows share, and how far the rows keep
/// clear of the pills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct FloatingFrame {
    /// `min(chat_column, room − 2 × gutter)`.
    pub column: f32,
    /// The room on either side of the column.
    pub side: f32,
    pub insets: HistoryInsets,
}

/// How far the history's rows keep clear of what floats over them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct HistoryInsets {
    pub top: Pixels,
    pub bottom: Pixels,
    pub left: Pixels,
    pub right: Pixels,
}

impl HistoryInsets {
    /// Nothing floats over the history (Classic).
    pub const ZERO: Self = Self {
        top: px(0.),
        bottom: px(0.),
        left: px(0.),
        right: px(0.),
    };

    /// `bounds` less the insets: what a reader can see of the history.
    pub(in crate::ui) fn inside(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let mut readable = bounds;
        readable.origin.x += self.left;
        readable.origin.y += self.top;
        readable.size.width = (bounds.size.width - self.left - self.right).max(px(0.));
        readable.size.height = (bounds.size.height - self.top - self.bottom).max(px(0.));
        readable
    }
}

/// How the rows are placed in a history pane: across it (Classic), or in
/// a centred column at most `column` wide, `gutter` from its edges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum RowColumn {
    Pane,
    Centred { gutter: f32, column: f32 },
}

impl RowColumn {
    /// The width rows are laid out for in a pane `pane` wide.
    pub(in crate::ui) fn width_in(self, pane: f32) -> f32 {
        match self {
            Self::Pane => pane,
            Self::Centred { gutter, column } => {
                floating_frame(FrameInput {
                    pane,
                    gutter,
                    bar_gap: 0.,
                    chat_column: column,
                    top: 0.,
                    composer: 0.,
                })
                .column
            }
        }
    }
}

pub(in crate::ui) fn floating_frame(input: FrameInput) -> FloatingFrame {
    let column = input
        .chat_column
        .min(input.pane - 2. * input.gutter)
        .max(0.);
    let side = ((input.pane - column) / 2.).max(0.);
    let top = if input.top > 0. {
        input.gutter + input.top + input.bar_gap
    } else {
        0.
    };
    let bottom = if input.composer > 0. {
        input.bar_gap + input.composer + input.gutter
    } else {
        0.
    };
    FloatingFrame {
        column,
        side,
        insets: HistoryInsets {
            top: px(top),
            bottom: px(bottom),
            left: px(side),
            right: px(side),
        },
    }
}

/// Where a body that is not the list sits when the bars float: clear of the
/// pills above and below, and in the centred column. It needs no measured
/// width, so a placeholder lands right on its first frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct PaneFrame {
    pub top: Pixels,
    pub bottom: Pixels,
    pub gutter: Pixels,
    pub column: Option<Pixels>,
}

impl PaneFrame {
    pub(in crate::ui) fn wrap(self, pane: AnyElement) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .items_center()
            .pt(self.top)
            .pb(self.bottom)
            .px(self.gutter)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when_some(self.column, |this, column| this.max_w(column))
                    .child(pane),
            )
            .into_any_element()
    }
}

/// Where the conversation's parts sit in a window's frame: one value
/// that the pill lanes, the rows' column, their insets and a pane in the
/// list's place all read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) enum ConversationFrame {
    /// Classic: the bars stack over the history, the rows run across it.
    Stacked,
    /// Floating: the header and the composer float over the history.
    Floating(Pills),
}

/// The Floating conversation's pills and the room they take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct Pills {
    /// Window edge to pill, pill to rows.
    pub gutter: Pixels,
    /// The widest conversation column.
    pub column: Option<Pixels>,
    /// A left tab column card's room, which the pill lanes keep clear of.
    pub lanes_left: Pixels,
    /// How far the rows keep clear of the pills.
    pub insets: HistoryInsets,
}

impl ConversationFrame {
    /// How far the history's rows keep clear of what floats over them.
    pub(in crate::ui) fn insets(&self) -> HistoryInsets {
        match self {
            Self::Stacked => HistoryInsets::ZERO,
            Self::Floating(pills) => pills.insets,
        }
    }

    /// Where a body that is not the list sits: `None` leaves it as it is.
    pub(in crate::ui) fn pane_frame(&self) -> Option<PaneFrame> {
        match self {
            Self::Stacked => None,
            Self::Floating(pills) => Some(PaneFrame {
                top: pills.insets.top,
                bottom: pills.insets.bottom,
                gutter: pills.gutter,
                column: pills.column,
            }),
        }
    }

    /// How the history's rows are placed.
    pub(in crate::ui) fn row_column(&self) -> RowColumn {
        match self {
            Self::Floating(Pills {
                gutter,
                column: Some(column),
                ..
            }) => RowColumn::Centred {
                gutter: f32::from(*gutter),
                column: f32::from(*column),
            },
            Self::Floating(Pills { column: None, .. }) | Self::Stacked => RowColumn::Pane,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, size};

    fn input(pane: f32) -> FrameInput {
        FrameInput {
            pane,
            gutter: 16.,
            bar_gap: 8.,
            chat_column: 696.,
            top: 56.,
            composer: 52.,
        }
    }

    #[test]
    fn the_column_never_exceeds_its_width_and_keeps_the_gutters() {
        for pane in [320., 700., 728., 900., 1440., 2560.] {
            let frame = floating_frame(input(pane));
            assert!(frame.column <= 696., "pane {pane}");
            assert!(frame.side >= 16., "pane {pane}");
            assert!((frame.column + 2. * frame.side - pane).abs() < 1e-3);
        }
        assert_eq!(floating_frame(input(700.)).column, 668.);
        assert_eq!(floating_frame(input(1440.)).column, 696.);
    }

    #[test]
    fn the_rows_take_the_pane_or_the_centred_column() {
        assert_eq!(RowColumn::Pane.width_in(1440.), 1440.);
        let centred = RowColumn::Centred {
            gutter: 16.,
            column: 696.,
        };
        assert_eq!(centred.width_in(1440.), 696.);
        assert_eq!(centred.width_in(700.), 668.);
        assert_eq!(centred.width_in(728.), 696.);
    }

    #[test]
    fn insets_clear_the_pills_and_zero_heights_clear_nothing() {
        let frame = floating_frame(input(1440.));
        assert_eq!(frame.insets.top, px(16. + 56. + 8.));
        assert_eq!(frame.insets.bottom, px(8. + 52. + 16.));
        let bare = floating_frame(FrameInput {
            top: 0.,
            composer: 0.,
            ..input(1440.)
        });
        assert_eq!((bare.insets.top, bare.insets.bottom), (px(0.), px(0.)));
    }

    #[test]
    fn zero_insets_leave_a_rectangle_as_it_is() {
        let bounds = Bounds {
            origin: point(px(10.), px(20.)),
            size: size(px(300.), px(400.)),
        };
        assert_eq!(HistoryInsets::ZERO.inside(bounds), bounds);
        let insets = HistoryInsets {
            top: px(80.),
            bottom: px(76.),
            left: px(0.),
            right: px(0.),
        };
        let readable = insets.inside(bounds);
        assert_eq!(readable.origin.y, px(100.));
        assert_eq!(readable.size.height, px(244.));
    }
}

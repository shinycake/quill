//! The forum row's topics line, measured (Telegram Desktop
//! `TopicsView::paint`, `dialogs_topics_view.cpp:221-238`): titles go on whole
//! while they fit, the first that does not fit is elided with "…" and the rest
//! is dropped; when less than an ellipsis is left the line stops. An icon is
//! whole or gone. Rendering stays a pure function of state: the element only
//! reads the text system to measure, and keeps nothing between frames.
#![deny(clippy::indexing_slicing)]

use crate::ui::sticker_player::StickerView;
use gpui_kit::*;

/// Gap between two titles (`topicsSkip`, 8px at the 12px row font): an en
/// space and a thin space.
pub(super) const SKIP: &str = "\u{2002}\u{2009}";

/// Side of a topic icon picture.
const ICON: f32 = 14.;

/// A topic icon's picture.
pub(super) enum Picture {
    Layered(StickerView),
    Still(ImageSource),
}

impl Picture {
    /// The picture at its drawn size; `key` keeps image element ids apart.
    pub(super) fn element(&self, key: &str, id: i64) -> AnyElement {
        match self {
            Picture::Layered(clip) => crate::ui::anim_layer::frames(clip.clone())
                .size(px(ICON))
                .flex_none()
                .into_any_element(),
            Picture::Still(source) => img(source.clone())
                .id(SharedString::from(format!("topic-icon-{key}-{id}")))
                .size(px(ICON))
                .aspect_square()
                .object_fit(ObjectFit::Contain)
                .flex_none()
                .into_any_element(),
        }
    }
}

/// One title of the line: its icon (when the picture is ready) and name.
pub(super) struct Title {
    pub(super) icon: Option<(i64, Picture)>,
    pub(super) name: String,
    /// The bold, bright colour of an unread topic.
    pub(super) unread: Option<Hsla>,
}

/// Widths that shape the line, in pixels.
#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub(super) gap: f32,
    pub(super) ellipsis: f32,
    pub(super) after_icon: f32,
}

/// A title's measured size.
#[derive(Clone, Copy)]
pub(super) struct Entry {
    pub(super) icon: bool,
    pub(super) text: f32,
}

/// How a title is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Fit {
    Whole,
    /// The text is cut to this width and ends in "…".
    Elided(f32),
}

/// Which titles show and how, for a line `width` wide: the titles up to the
/// first that does not fit.
pub(super) fn fit(entries: &[Entry], metrics: Metrics, width: f32) -> Vec<Fit> {
    let mut shown = Vec::new();
    let mut room = width;
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            room -= metrics.gap;
        }
        if room < metrics.ellipsis {
            break;
        }
        let lead = if entry.icon {
            ICON + metrics.after_icon
        } else {
            0.
        };
        if lead + entry.text <= room {
            shown.push(Fit::Whole);
            room -= lead + entry.text;
            continue;
        }
        let text = room - lead;
        if text >= metrics.ellipsis {
            shown.push(Fit::Elided(text));
        }
        break;
    }
    shown
}

/// The titles as one measured line, `width` of its parent.
pub(super) struct FittedTitles {
    pub(super) titles: Vec<Title>,
    /// Element ids of the pieces: `(forum-title-piece, n)` for title `n`.
    pub(super) key: String,
}

fn measure(window: &Window, text: &str, size: Pixels, run: TextRun) -> f32 {
    let run = TextRun {
        len: text.len(),
        ..run
    };
    window
        .text_system()
        .shape_line(SharedString::from(text.to_string()), size, &[run], None)
        .width
        .into()
}

impl FittedTitles {
    /// The line laid out for `bounds`.
    fn lay_out(&self, bounds: Bounds<Pixels>, window: &Window) -> Div {
        let style = window.text_style();
        let size = style.font_size.to_pixels(window.rem_size());
        let plain = style.clone().to_run(0);
        let bold = style
            .clone()
            .highlight(HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            })
            .to_run(0);
        let run_for = |title: &Title| {
            if title.unread.is_some() {
                bold.clone()
            } else {
                plain.clone()
            }
        };
        let metrics = Metrics {
            gap: measure(window, SKIP, size, plain.clone()),
            ellipsis: measure(window, "…", size, plain.clone()),
            after_icon: measure(window, " ", size, plain.clone()),
        };
        let entries: Vec<Entry> = self
            .titles
            .iter()
            .map(|title| Entry {
                icon: title.icon.is_some(),
                text: measure(window, &title.name, size, run_for(title)),
            })
            .collect();
        let fits = fit(&entries, metrics, bounds.size.width.into());
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .w(bounds.size.width)
            .h(bounds.size.height);
        for (index, (title, fit)) in self.titles.iter().zip(fits).enumerate() {
            if index > 0 {
                row = row.child(div().flex_none().w(px(metrics.gap)));
            }
            if let Some((id, picture)) = &title.icon {
                row = row.child(
                    div()
                        .id(("forum-title-icon", index as u64))
                        .test_support()
                        .flex()
                        .flex_none()
                        .child(picture.element(&self.key, *id)),
                );
                row = row.child(div().flex_none().w(px(metrics.after_icon)));
            }
            let mut name = div()
                .id(("forum-title-piece", index as u64))
                .test_support()
                .whitespace_nowrap()
                .child(match title.unread {
                    Some(color) => StyledText::new(title.name.clone()).with_highlights(vec![(
                        0..title.name.len(),
                        HighlightStyle {
                            font_weight: Some(FontWeight::BOLD),
                            color: Some(color),
                            ..Default::default()
                        },
                    )]),
                    None => StyledText::new(title.name.clone()),
                });
            name = match fit {
                Fit::Whole => name.flex_none(),
                Fit::Elided(width) => name.flex_none().w(px(width)).truncate(),
            };
            row = row.child(name);
        }
        row
    }
}

impl IntoElement for FittedTitles {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FittedTitles {
    type RequestLayoutState = ();
    type PrepaintState = AnyElement;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text = window.text_style();
        let size = text.font_size.to_pixels(window.rem_size());
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = text
            .line_height
            .to_pixels(size.into(), window.rem_size())
            .into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mut line = self.lay_out(bounds, window).into_any_element();
        line.prepaint_as_root(bounds.origin, bounds.size.map(Into::into), window, cx);
        line
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        line: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        line.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Fit, Metrics, fit};

    const M: Metrics = Metrics {
        gap: 8.,
        ellipsis: 10.,
        after_icon: 3.,
    };

    fn text(width: f32) -> Entry {
        Entry {
            icon: false,
            text: width,
        }
    }

    fn icon(width: f32) -> Entry {
        Entry {
            icon: true,
            text: width,
        }
    }

    #[test]
    fn whole_titles_go_on_while_they_fit_and_the_first_that_does_not_is_elided() {
        let got = fit(&[text(70.), text(70.), text(70.), text(70.)], M, 200.);
        // 70 + 8 + 70 + 8 = 156, 44 left: the third is cut to 44.
        assert_eq!(got, vec![Fit::Whole, Fit::Whole, Fit::Elided(44.)]);
    }

    #[test]
    fn a_title_that_fills_the_line_exactly_is_whole_and_ends_the_line() {
        assert_eq!(fit(&[text(200.), text(30.)], M, 200.), vec![Fit::Whole]);
    }

    #[test]
    fn less_than_an_ellipsis_left_stops_the_line() {
        // 190 + 8 leaves 2: no stub.
        assert_eq!(fit(&[text(190.), text(40.)], M, 200.), vec![Fit::Whole]);
        // 190 and an icon title: the icon is dropped, never half-drawn.
        assert_eq!(fit(&[text(190.), icon(40.)], M, 200.), vec![Fit::Whole]);
        // 170 leaves 22 after the gap: the icon (17) fits but not the "…".
        assert_eq!(fit(&[text(170.), icon(40.)], M, 200.), vec![Fit::Whole]);
        // 150 leaves 42: the icon, then the text elided to 25.
        assert_eq!(
            fit(&[text(150.), icon(40.)], M, 200.),
            vec![Fit::Whole, Fit::Elided(25.)]
        );
    }

    #[test]
    fn sixteen_titles_with_icons_show_whole_ones_and_one_elided_tail() {
        let entries: Vec<Entry> = (0..16).map(|_| icon(30.)).collect();
        let got = fit(&entries, M, 200.);
        let (last, whole) = got.split_last().expect("some titles show");
        assert!(whole.iter().all(|f| *f == Fit::Whole));
        assert!(matches!(last, Fit::Elided(_) | Fit::Whole));
        assert!(got.len() < 16);
        // Nothing past the edge: every shown title plus gaps fits 200.
        let used: f32 = got
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let gap = if i > 0 { M.gap } else { 0. };
                gap + 14.
                    + M.after_icon
                    + match f {
                        Fit::Whole => 30.,
                        Fit::Elided(w) => *w,
                    }
            })
            .sum();
        assert!(used <= 200.001, "{used}");
    }

    /// The measured line through GPUI: titles in a 200px row.
    #[cfg(feature = "demo-capture")]
    mod window {
        use super::super::{FittedTitles, Picture, Title};
        use gpui_kit::test::TestWindowExt;
        use gpui_kit::{
            Context, ImageSource, InteractiveElement, IntoElement, ParentElement, Render, Styled,
            TestAppContext, TestSupportExt, Window, div, px,
        };

        struct Line(Vec<(String, bool)>);

        impl Render for Line {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut Context<Self>,
            ) -> impl IntoElement {
                let titles = self
                    .0
                    .iter()
                    .enumerate()
                    .map(|(i, (name, icon))| Title {
                        icon: icon.then(|| {
                            (
                                i as i64 + 1,
                                Picture::Still(ImageSource::from(std::path::PathBuf::from(
                                    "icon.png",
                                ))),
                            )
                        }),
                        name: name.clone(),
                        unread: (i % 3 == 0).then(|| gpui_kit::rgb(0).into()),
                    })
                    .collect();
                div().w(px(200.)).text_xs().child(
                    div()
                        .id(("line", 1u64))
                        .test_support()
                        .flex()
                        .items_center()
                        .child(FittedTitles {
                            titles,
                            key: "1".into(),
                        }),
                )
            }
        }

        /// `(left, right)` of each title's name and whether its icon shows,
        /// plus the line's right edge.
        fn laid_out(
            titles: Vec<(String, bool)>,
            cx: &mut TestAppContext,
        ) -> (Vec<(f32, f32, bool)>, f32) {
            cx.update(gpui_kit::init);
            let (_view, vcx) = cx.add_window_view(move |_, _| Line(titles));
            vcx.run_until_parked();
            vcx.update(|window, cx| window.render_frame(cx));
            vcx.update(|window, _| {
                let line = window.find(("line", 1u64)).bounds();
                let origin: f32 = line.left().into();
                let edge: f32 = f32::from(line.right()) - origin;
                let mut shown = Vec::new();
                for index in 0..40u64 {
                    let Some(name) = window.try_find(("forum-title-piece", index)) else {
                        break;
                    };
                    let bounds = name.bounds();
                    let icon = window.try_find(("forum-title-icon", index)).is_some();
                    shown.push((
                        f32::from(bounds.left()) - origin,
                        f32::from(bounds.right()) - origin,
                        icon,
                    ));
                }
                (shown, edge)
            })
        }

        #[gpui_kit::test]
        fn sixteen_titles_with_icons_fit_whole_with_one_elided_tail(cx: &mut TestAppContext) {
            let titles = (0..16)
                .map(|i| (format!("Topic number {i}"), true))
                .collect();
            let (shown, edge) = laid_out(titles, cx);
            assert!(shown.len() > 1 && shown.len() < 16, "{}", shown.len());
            let mut previous_right = 0.;
            for (left, right, icon) in &shown {
                assert!(*icon, "an icon sits before every shown title");
                assert!(*left >= previous_right, "titles do not overlap");
                assert!(*right <= edge + 0.5, "{right} past {edge}");
                previous_right = *right;
            }
            // Only the last one may be cut; every other title is whole, so
            // the first ones are as wide as their text.
            let widths: Vec<f32> = shown.iter().map(|(l, r, _)| r - l).collect();
            let whole = widths.first().copied().unwrap_or_default();
            for width in widths.iter().take(widths.len().saturating_sub(1)) {
                assert!((width - whole).abs() < 6., "{widths:?}");
            }
        }

        #[gpui_kit::test]
        fn a_long_title_before_an_icon_topic_is_elided_and_the_icon_dropped(
            cx: &mut TestAppContext,
        ) {
            let long = "Release planning for the fourth quarter and beyond, really long ".repeat(2);
            let (shown, edge) = laid_out(vec![(long, false), ("Design".into(), true)], cx);
            assert_eq!(shown.len(), 1, "the icon topic is dropped");
            let (left, right, _) = shown.first().copied().unwrap_or_default();
            assert!(left < 1., "starts at the line start");
            assert!((right - edge).abs() < 1., "elided at the edge");
        }
    }
}

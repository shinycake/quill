//! One text drawn at several widths in the same frame: replies quoting the
//! same message, each quote line as wide as its bubble, or a paragraph in
//! columns of different widths; and the budget of laid-out widths
//! (`docs/decisions/paragraph-cache-widths.md`).
use super::{
    Entry, Key, Pool, Width, held_in_tests, layouts_in_tests, relayouts_in_tests, reset_in_tests,
};
use crate::ui::selectable_text::SelectableRichText;
use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, StyleRefinement,
    Styled, StyledText, TestAppContext, TextRun, VisualTestContext, Window, div, font, px, size,
};
use std::cell::RefCell;
use std::rc::Rc;

/// `words` words in a right-to-left script, letters picked by code point.
fn rtl_text(words: usize) -> SharedString {
    (0..words)
        .map(|w| {
            (0..4)
                .filter_map(|i| char::from_u32(0x5D0 + ((w * 7 + i * 3) % 27) as u32))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
        .into()
}

/// The same one-line text in boxes of each width, as a chat's quote lines.
struct Quotes {
    text: SharedString,
    widths: Vec<f32>,
}

impl Render for Quotes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .children(self.widths.iter().map(|width| {
                div()
                    .w(px(*width))
                    .truncate()
                    .child(crate::ui::bidi_line::one_line_plain(self.text.clone()))
            }))
    }
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

/// The same paragraph in columns of each width, wrapped.
struct Columns {
    text: SharedString,
    widths: Vec<f32>,
}

impl Render for Columns {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .text_size(px(14.))
            .children(self.widths.iter().enumerate().map(|(ix, width)| {
                let text = self.text.clone();
                div().w(px(*width)).child(
                    SelectableRichText::new(("column", ix), text.clone(), StyledText::new(text))
                        .bidi(Vec::new(), Vec::new()),
                )
            }))
    }
}

/// Draws `view` for a few frames: the first frame lays each width out
/// once, the later ones lay nothing out, and the one paragraph stays laid
/// out at no more than `laid_out` widths.
fn draws_each_width_once<V: Render>(
    cx: &mut TestAppContext,
    view: V,
    one_line: bool,
    laid_out: usize,
) {
    reset_in_tests();
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(600.), px(800.)), move |_, _| view);
    let vcx = &mut VisualTestContext::from_window(handle.into(), cx);
    redraw(vcx);
    let first = layouts_in_tests();
    assert!(first > 0, "the text took the bidi layout");
    assert_eq!(
        relayouts_in_tests(),
        0,
        "no width laid out twice in a frame"
    );
    for _ in 0..5 {
        redraw(vcx);
    }
    assert_eq!(layouts_in_tests(), first, "later frames reuse every width");
    let held = held_in_tests(one_line);
    assert_eq!(held.drawn, 1, "{held:?}");
    assert!(held.laid_out <= laid_out, "{held:?}");
    assert_eq!(held.most_measured_only, 0, "{held:?}");
}

#[gpui_kit::test]
fn quotes_cut_short_at_four_widths_are_laid_out_once(cx: &mut TestAppContext) {
    let quotes = Quotes {
        text: rtl_text(40),
        widths: vec![60., 80., 100., 120.],
    };
    draws_each_width_once(cx, quotes, true, super::SIZES);
}

#[gpui_kit::test]
fn quotes_that_fit_at_four_widths_are_laid_out_once(cx: &mut TestAppContext) {
    let quotes = Quotes {
        text: rtl_text(2),
        widths: vec![300., 340., 380., 420.],
    };
    draws_each_width_once(cx, quotes, true, super::SIZES);
}

#[gpui_kit::test]
fn a_paragraph_wrapped_at_four_widths_is_laid_out_once(cx: &mut TestAppContext) {
    let columns = Columns {
        text: rtl_text(80),
        widths: vec![200., 260., 320., 380.],
    };
    // Laid out at the four widths drawn, and at none only measured.
    draws_each_width_once(cx, columns, false, 4);
}

/// One quote line in a view of its own, as a reply's bubble is.
struct Quote {
    text: SharedString,
    width: f32,
}

impl Render for Quote {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(self.width))
            .truncate()
            .child(crate::ui::bidi_line::one_line_plain(self.text.clone()))
    }
}

/// Quotes in cached views: a frame draws again only the one notified, and
/// replays the others without asking the cache.
struct CachedQuotes {
    quotes: Vec<(Entity<Quote>, f32)>,
}

impl Render for CachedQuotes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .children(self.quotes.iter().map(|(quote, width)| {
                quote
                    .clone()
                    .cached(StyleRefinement::default().w(px(*width)).h(px(24.)))
            }))
    }
}

/// Quotes at four widths in cached views, each drawn again in turn.
#[gpui_kit::test]
fn quotes_drawn_again_after_a_replay_are_laid_out_once(cx: &mut TestAppContext) {
    reset_in_tests();
    cx.update(gpui_kit::init);
    let text = rtl_text(40);
    let quotes = Rc::new(RefCell::new(Vec::new()));
    let quotes_in = quotes.clone();
    let handle = cx.open_window(size(px(600.), px(400.)), move |_, cx| {
        let quotes: Vec<(Entity<Quote>, f32)> = [60., 80., 100., 120.]
            .into_iter()
            .map(|width| {
                let text = text.clone();
                (cx.new(|_| Quote { text, width }), width)
            })
            .collect();
        *quotes_in.borrow_mut() = quotes.iter().map(|(quote, _)| quote.clone()).collect();
        CachedQuotes { quotes }
    });
    let vcx = &mut VisualTestContext::from_window(handle.into(), cx);
    redraw(vcx);
    let first = layouts_in_tests();
    assert!(first > 0, "the text took the bidi layout");
    let quotes: Vec<Entity<Quote>> = quotes.borrow().clone();
    for quote in quotes.iter().cycle().take(12) {
        vcx.update(|_, cx| quote.update(cx, |_, cx| cx.notify()));
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.run_until_parked();
    }
    assert_eq!(
        layouts_in_tests(),
        first,
        "each quote found its width laid out"
    );
    assert_eq!(relayouts_in_tests(), 0);
}

/// Past the budget, the least recently used layouts go, down to three
/// quarters of it, and never one the frame being ended used.
#[gpui_kit::test]
fn past_the_budget_the_least_recently_used_layouts_go(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(200.), px(100.)), |_, _| Columns {
        text: SharedString::default(),
        widths: Vec::new(),
    });
    let vcx = &mut VisualTestContext::from_window(handle.into(), cx);
    let text = rtl_text(6);
    let run = TextRun {
        len: text.len(),
        font: font("Helvetica"),
        color: gpui_kit::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let paragraph = vcx
        .update(|window, _| BidiParagraph::layout(&text, &[run], px(14.), px(20.), None, window))
        .map(Rc::new)
        .expect("right-to-left text takes the bidi layout");
    let mut pool = Pool::new(16, true);
    // Ten paragraphs, each drawn at one width, the nth used at clock n.
    for n in 1..=10u64 {
        let key = Key {
            text: n.to_string().into(),
            runs: 0,
            font_size: 0,
            line_height: 0,
        };
        let width = Width {
            wrap: Some(1),
            size: paragraph.size(),
            paragraph: Some(paragraph.clone()),
            drawn: n,
            used: n,
        };
        pool.entries.insert(
            key,
            Entry {
                widths: vec![width],
                measured: None,
                used: n,
            },
        );
    }
    let laid_out = |pool: &Pool| {
        let mut used: Vec<u64> = pool
            .entries
            .values()
            .flat_map(|e| &e.widths)
            .filter(|w| w.paragraph.is_some())
            .map(|w| w.used)
            .collect();
        used.sort_unstable();
        used
    };
    // A budget of eight: down to six, the four oldest out.
    pool.trim(7, 8);
    assert_eq!(laid_out(&pool), vec![5, 6, 7, 8, 9, 10]);
    assert_eq!(pool.laid_out, 6);
    // A frame that began at 7 used 8 to 10: those stay, whatever the budget.
    pool.trim(7, 2);
    assert_eq!(laid_out(&pool), vec![8, 9, 10]);
    assert_eq!(pool.laid_out, 3);
}

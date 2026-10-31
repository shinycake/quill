//! One text drawn at several widths in the same frame: replies quoting the
//! same message, each quote line as wide as its bubble
//! (`docs/decisions/paragraph-cache-widths.md`).
use super::{held_in_tests, layouts_in_tests, relayouts_in_tests, reset_in_tests};
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, StyleRefinement,
    Styled, TestAppContext, VisualTestContext, Window, div, px, size,
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

/// Draws `text` at each of `widths` for a few frames: the first frame lays
/// each width out once, the later ones lay nothing out.
fn draws_each_width_once(cx: &mut TestAppContext, text: SharedString, widths: Vec<f32>) {
    reset_in_tests();
    cx.update(gpui_kit::init);
    let handle = cx.open_window(size(px(600.), px(400.)), move |_, _| Quotes {
        text,
        widths,
    });
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
    let held = held_in_tests(true);
    assert_eq!(held.drawn, 1, "{held:?}");
    assert!(held.laid_out <= super::SIZES, "{held:?}");
}

#[gpui_kit::test]
fn quotes_cut_short_at_four_widths_are_laid_out_once(cx: &mut TestAppContext) {
    draws_each_width_once(cx, rtl_text(40), vec![60., 80., 100., 120.]);
}

#[gpui_kit::test]
fn quotes_that_fit_at_four_widths_are_laid_out_once(cx: &mut TestAppContext) {
    draws_each_width_once(cx, rtl_text(2), vec![300., 340., 380., 420.]);
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

#[gpui_kit::test]
fn quotes_drawn_again_after_a_replay_are_laid_out_once(cx: &mut TestAppContext) {
    replays_lay_out_nothing(cx);
}

/// Draws quotes at four widths in cached views, then each again in turn.
fn replays_lay_out_nothing(cx: &mut TestAppContext) {
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

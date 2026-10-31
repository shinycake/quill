//! One text drawn at several widths in the same frame: replies quoting the
//! same message, each quote line as wide as its bubble, or a paragraph in
//! columns of different widths; and the budgets of laid-out widths, sizes
//! and texts (`docs/decisions/paragraph-cache-widths.md`).
use super::{
    Budget, Cache, Entry, Key, LAID_OUT, Pool, Width, held_in_tests, layouts_in_tests,
    relayouts_in_tests, reset_in_tests, trim_layouts,
};
use crate::ui::selectable_text::SelectableRichText;
use gpui_kit::base::input::bidi_paragraph::BidiParagraph;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, StyleRefinement,
    Styled, StyledText, TestAppContext, TextRun, VisualTestContext, Window, WindowHandle, div,
    font, px, size,
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
/// once, and the later ones lay nothing out.
fn draws_each_width_once<V: Render>(cx: &mut TestAppContext, view: V, one_line: bool) {
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
    assert_eq!(held.unwrapped_undrawn, 0, "{held:?}");
}

#[gpui_kit::test]
fn quotes_cut_short_at_four_widths_are_laid_out_once(cx: &mut TestAppContext) {
    let quotes = Quotes {
        text: rtl_text(40),
        widths: vec![60., 80., 100., 120.],
    };
    draws_each_width_once(cx, quotes, true);
}

#[gpui_kit::test]
fn quotes_that_fit_at_four_widths_are_laid_out_once(cx: &mut TestAppContext) {
    let quotes = Quotes {
        text: rtl_text(2),
        widths: vec![300., 340., 380., 420.],
    };
    draws_each_width_once(cx, quotes, true);
}

#[gpui_kit::test]
fn a_paragraph_wrapped_at_four_widths_is_laid_out_once(cx: &mut TestAppContext) {
    let columns = Columns {
        text: rtl_text(80),
        widths: vec![200., 260., 320., 380.],
    };
    draws_each_width_once(cx, columns, false);
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

/// A bubble's paragraph `step` frames into a splitter drag that narrows it
/// a pixel a frame: drawn at its room, and measured at a narrower width too
/// (a column of its own here).
fn dragged(step: usize) -> Vec<f32> {
    let step = step as f32;
    vec![404. - step, 318. - step]
}

/// Layouts made while the drag goes through each of `steps`, a frame each.
fn layouts_dragging(
    window: WindowHandle<Columns>,
    cx: &mut VisualTestContext,
    steps: impl Iterator<Item = usize>,
) -> u64 {
    let before = layouts_in_tests();
    for step in steps {
        window
            .update(cx, |columns, _, _| columns.widths = dragged(step))
            .expect("the window is open");
        redraw(cx);
    }
    layouts_in_tests() - before
}

/// A splitter dragged slowly over 64 frames and back: the paragraph is laid
/// out at 128 widths on the way, and the way back finds every one, as the
/// old cache's 2,048 entries did.
#[gpui_kit::test]
fn dragging_back_over_128_widths_lays_out_nothing(cx: &mut TestAppContext) {
    reset_in_tests();
    cx.update(gpui_kit::init);
    let columns = Columns {
        text: rtl_text(80),
        widths: dragged(0),
    };
    let window = cx.open_window(size(px(600.), px(800.)), move |_, _| columns);
    let vcx = &mut VisualTestContext::from_window(window.into(), cx);
    redraw(vcx);
    let out = layouts_dragging(window, vcx, 1..64);
    assert!(out >= 2 * 63, "each frame laid out two widths: {out}");
    assert_eq!(layouts_dragging(window, vcx, (0..64).rev()), 0);
    assert_eq!(relayouts_in_tests(), 0);
    let held = held_in_tests(false);
    assert!(held.laid_out >= 128, "{held:?}");
}

/// A short right-to-left paragraph laid out unconstrained, to fill a
/// cache's widths with.
fn a_paragraph(cx: &mut TestAppContext) -> Rc<BidiParagraph> {
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
    vcx.update(|window, _| BidiParagraph::layout(&text, &[run], px(14.), px(20.), None, window))
        .map(Rc::new)
        .expect("right-to-left text takes the bidi layout")
}

fn width(wrap: Option<f32>, paragraph: Option<&Rc<BidiParagraph>>, drawn: u64, used: u64) -> Width {
    Width {
        wrap: wrap.map(f32::to_bits),
        size: paragraph.map(|p| p.size()).unwrap_or_default(),
        paragraph: paragraph.cloned(),
        bytes: 0,
        drawn,
        used,
    }
}

/// When a frame of a window ends, a wrapped paragraph lets go of the
/// layouts at its widest line or wider that were never drawn, if wider than
/// the window, and keeps every other: a row measured just off screen at one
/// width and drawn at another finds the first laid out when the window goes
/// back to it, and a paragraph that fits the window finds its widest line
/// laid out when its box grows to fit it.
#[gpui_kit::test]
fn a_frame_lets_go_only_of_widest_line_layouts_wider_than_its_window(cx: &mut TestAppContext) {
    let paragraph = a_paragraph(cx);
    let natural = f32::from(paragraph.size().width);
    let p = Some(&paragraph);
    let laid_out_after_a_frame = |window: f32| {
        let mut entry = Entry {
            widths: vec![
                // Unconstrained: a size only, as measured.
                Width {
                    paragraph: None,
                    ..width(None, p, 0, 1)
                },
                width(Some(natural), p, 0, 2),
                width(Some(natural + 100.), p, 0, 3),
                width(Some(natural + 200.), p, 4, 4),
                width(Some(natural / 2.), p, 0, 5),
                width(Some(natural / 3.), p, 6, 6),
            ],
            used: 6,
        };
        entry.settle(window);
        entry
            .widths
            .iter()
            .filter(|w| w.paragraph.is_some())
            .map(|w| w.used)
            .collect::<Vec<u64>>()
    };
    // In a window narrower than its widest line, the widest line and wider,
    // never drawn, go; drawn ones and narrower ones only measured stay.
    assert_eq!(laid_out_after_a_frame(natural / 2.), vec![4, 5, 6]);
    // In a window it fits, only what is wider than the window goes.
    assert_eq!(laid_out_after_a_frame(natural + 50.), vec![2, 4, 5, 6]);
}

fn key(n: u64) -> Key {
    Key {
        text: n.to_string().into(),
        runs: 0,
        font_size: 0,
        line_height: 0,
    }
}

/// Past its paragraphs and an eighth more, a pool lets the least recently
/// used go down to its count with the new one, never one the frame being
/// drawn used.
#[gpui_kit::test]
fn past_its_paragraphs_the_least_recently_used_go(cx: &mut TestAppContext) {
    let paragraph = a_paragraph(cx);
    let mut pool = Pool::new(8, true);
    let add = |pool: &mut Pool, n: u64, start: u64| {
        pool.entry(key(n), start)
            .add(width(Some(100.), Some(&paragraph), n, n));
    };
    let held = |pool: &Pool| {
        let mut used: Vec<u64> = pool.entries.values().map(|e| e.used).collect();
        used.sort_unstable();
        used
    };
    for n in 1..=9 {
        add(&mut pool, n, 9);
    }
    assert_eq!(held(&pool), (1..=9).collect::<Vec<_>>());
    add(&mut pool, 10, 9);
    assert_eq!(held(&pool), (3..=10).collect::<Vec<_>>());
    add(&mut pool, 11, 10);
    // A frame that began at 2 used them all: they stay.
    add(&mut pool, 12, 2);
    assert_eq!(held(&pool), (3..=12).collect::<Vec<_>>());
}

/// Lays out `texts` texts of each kind in `cache`, a width of `bytes` each,
/// the nth wrapped one used at clock 2n + 1 and the nth one-line one at
/// 2n + 2.
fn fill(cache: &mut Cache, paragraph: &Rc<BidiParagraph>, texts: u64, bytes: usize) {
    for n in 0..texts {
        for (one_line, used) in [(false, 2 * n + 1), (true, 2 * n + 2)] {
            cache.laid_out(key(n), one_line, bytes).add(Width {
                bytes,
                ..width(Some(100.), Some(paragraph), used, used)
            });
        }
    }
}

/// When each layout `cache` holds was last used: wrapped, then one-line.
fn laid_out(cache: &Cache) -> (Vec<u64>, Vec<u64>) {
    let used = |pool: &Pool| {
        let mut used: Vec<u64> = pool.laid_out_widths().map(|w| w.used).collect();
        used.sort_unstable();
        used
    };
    (used(&cache.paragraphs), used(&cache.lines))
}

/// Wrapped paragraphs and one-line texts share one budget of laid-out
/// widths, the old cache's: past it, the least recently used of either kind
/// go.
#[gpui_kit::test]
fn both_kinds_keep_to_one_budget_of_layouts(cx: &mut TestAppContext) {
    let paragraph = a_paragraph(cx);
    let mut cache = Cache::default();
    // Each kind within the budget, both together past it.
    let each = LAID_OUT.layouts as u64 * 9 / 16;
    fill(&mut cache, &paragraph, each, 0);
    cache.end_frame(2 * each, 600.);
    let (wrapped, lines) = laid_out(&cache);
    assert_eq!(wrapped.len() + lines.len(), LAID_OUT.layouts * 3 / 4);
    assert_eq!(wrapped.len(), lines.len());
    let oldest = wrapped.first().min(lines.first()).copied();
    assert_eq!(oldest, Some(2 * each - LAID_OUT.layouts as u64 * 3 / 4 + 1));
}

/// Both kinds share one budget of bytes too: past it, the least recently
/// used layouts of either kind go until they hold three quarters of it,
/// however few they are, and never one the frame being ended used.
#[gpui_kit::test]
fn both_kinds_keep_to_one_budget_of_bytes(cx: &mut TestAppContext) {
    let paragraph = a_paragraph(cx);
    let mut cache = Cache::default();
    let eighth = LAID_OUT.bytes / 8;
    // Six of each kind, an eighth of the budget each: half as much again.
    fill(&mut cache, &paragraph, 6, eighth);
    // The frame being ended began at 4: what it used, 5 to 12, stays.
    cache.end_frame(4, 600.);
    assert_eq!(laid_out(&cache), (vec![5, 7, 9, 11], vec![6, 8, 10, 12]));
    // One more, past the budget again: down to three quarters of it.
    cache.laid_out(key(6), false, eighth).add(Width {
        bytes: eighth,
        ..width(Some(100.), Some(&paragraph), 13, 13)
    });
    cache.end_frame(13, 600.);
    assert_eq!(laid_out(&cache), (vec![9, 11, 13], vec![8, 10, 12]));
}

/// Past the budget, the least recently used layouts go, down to three
/// quarters of it, and never one the frame being ended used.
#[gpui_kit::test]
fn past_the_budget_the_least_recently_used_layouts_go(cx: &mut TestAppContext) {
    let paragraph = a_paragraph(cx);
    let mut pool = Pool::new(16, true);
    // Ten paragraphs, each drawn at one width, the nth used at clock n.
    for n in 1..=10u64 {
        let entry = Entry {
            widths: vec![width(Some(100.), Some(&paragraph), n, n)],
            used: n,
        };
        pool.entries.insert(key(n), entry);
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
    let layouts = |layouts| Budget {
        bytes: usize::MAX,
        layouts,
    };
    trim_layouts(&mut [&mut pool], 7, layouts(8));
    assert_eq!(laid_out(&pool), vec![5, 6, 7, 8, 9, 10]);
    assert_eq!(pool.laid_out, 6);
    // A frame that began at 7 used 8 to 10: those stay, whatever the budget.
    trim_layouts(&mut [&mut pool], 7, layouts(2));
    assert_eq!(laid_out(&pool), vec![8, 9, 10]);
    assert_eq!(pool.laid_out, 3);
}

/// Past the budget of widths, the least recently used that keep only a
/// size go, never a laid-out one and never one the frame being ended used;
/// a paragraph left with no width goes too.
#[gpui_kit::test]
fn past_the_budget_the_least_recently_used_sizes_go(cx: &mut TestAppContext) {
    let paragraph = a_paragraph(cx);
    let mut pool = Pool::new(16, true);
    // Paragraph 1: sizes at clock 1 to 6, laid out at 7. Paragraph 2: a
    // size at 8, laid out at 9, sizes at 10 to 12.
    let mut first = Entry::default();
    for n in 1..=6 {
        first.add(width(Some(n as f32), None, 0, n));
    }
    first.add(width(Some(7.), Some(&paragraph), 7, 7));
    let mut second = Entry::default();
    second.add(width(Some(8.), None, 0, 8));
    second.add(width(Some(9.), Some(&paragraph), 9, 9));
    for n in 10..=12 {
        second.add(width(Some(n as f32), None, 0, n));
    }
    pool.entries.insert(key(1), first);
    pool.entries.insert(key(2), second);
    let used = |pool: &Pool| {
        let mut used: Vec<u64> = pool
            .entries
            .values()
            .flat_map(|e| &e.widths)
            .map(|w| w.used)
            .collect();
        used.sort_unstable();
        used
    };
    // Twelve against a budget of eight: down to six, the six oldest sizes
    // out.
    pool.trim_widths(11, 8);
    assert_eq!(used(&pool), vec![7, 8, 9, 10, 11, 12]);
    assert_eq!(pool.widths, 6);
    // Against a budget of two, with a frame that began at 9: the layouts
    // at 7 and 9 and the sizes used at 10 to 12 stay; only 8 goes.
    pool.trim_widths(9, 2);
    assert_eq!(used(&pool), vec![7, 9, 10, 11, 12]);
    // Once their layouts have gone, paragraph 1 goes with its last width.
    for width in pool.entries.values_mut().flat_map(|e| &mut e.widths) {
        width.paragraph = None;
    }
    pool.trim_widths(12, 2);
    assert_eq!(used(&pool), vec![12]);
    assert_eq!(pool.entries.len(), 1);
}

/// What the cache lets go is noted for the idle heap release
/// (`quill::free_heap`), byte for byte as each layout was counted: layouts
/// past the budget, and a widest line a frame never drew.
#[gpui_kit::test]
fn what_the_cache_lets_go_is_noted_for_the_heap_release(cx: &mut TestAppContext) {
    let paragraph = a_paragraph(cx);
    let mut cache = Cache::default();
    let eighth = LAID_OUT.bytes / 8;
    fill(&mut cache, &paragraph, 6, eighth);
    let before = quill::free_heap::freed();
    // As in `both_kinds_keep_to_one_budget_of_bytes`: the four used at 1
    // to 4 go.
    cache.end_frame(4, 600.);
    assert_eq!(quill::free_heap::freed() - before, 4 * eighth);

    let natural = f32::from(paragraph.size().width);
    let mut entry = Entry {
        widths: vec![
            // Unconstrained: a size only, as measured.
            Width {
                paragraph: None,
                ..width(None, Some(&paragraph), 0, 1)
            },
            Width {
                bytes: 3000,
                ..width(Some(natural), Some(&paragraph), 0, 1)
            },
            Width {
                bytes: 5000,
                ..width(Some(natural / 2.), Some(&paragraph), 2, 2)
            },
        ],
        used: 2,
    };
    let before = quill::free_heap::freed();
    entry.settle(natural / 2.);
    assert_eq!(quill::free_heap::freed() - before, 3000);
}

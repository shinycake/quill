//! The formatted composer field (codex:composer-input) driven through a real
//! kit `Textarea`: the caret and selection across formatted runs, custom
//! emoji as one caret stop (also inside an Arabic paragraph), mention tags
//! through edits and undo, typed markdown, and IME composition inside a
//! format. They need gpui-kit's `test-support`:
//! `cargo test --features demo-capture --bin quill composer_field`.

use super::composer_field::{apply_markdown, doc_content, field_doc, span_styler};
use super::composer_rtl::tests::BidiTextSystem;
use gpui_base::input::{InlineToken, InputContent};
use gpui_kit::component::Root;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::test::{TestSupportExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, Entity, EntityInputHandler, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, Styled, TestAppContext, Window, div, px, size,
};
use quill::composer_doc::{ComposerDoc, ComposerTag, DocEmoji, DocSpan};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Width every inline token is drawn at in these tests.
const TOKEN_WIDTH: f32 = 20.;

struct Field {
    input: Entity<TextareaState>,
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p_4().child(
            div().id("composer").test_support().w(px(400.)).child(
                Textarea::new(&self.input)
                    .appearance(false)
                    .bordered(false)
                    .token(|token, _, _| {
                        div()
                            .id("emoji")
                            .test_support()
                            .w(px(TOKEN_WIDTH))
                            .h(token.line_height())
                    }),
            ),
        )
    }
}

/// The composer field (styler installed) in a window whose text system
/// shapes like the real platforms (`BidiTextSystem`).
fn with_field(test: impl FnOnce(&mut TestAppContext, AnyWindowHandle, Entity<TextareaState>)) {
    let dispatcher = gpui_kit::TestDispatcher::new(0);
    let mut cx = TestAppContext::build_with_text_system(
        dispatcher,
        None,
        Arc::new(BidiTextSystem { base_rtl: None }),
    );
    cx.update(gpui_kit::init);
    let slot: Rc<RefCell<Option<Entity<TextareaState>>>> = Rc::new(RefCell::new(None));
    let handle = cx.open_window(size(px(640.), px(240.)), {
        let slot = slot.clone();
        move |window, cx| {
            let input = cx.new(|cx| {
                let mut state = TextareaState::new(window, cx);
                state.set_span_styler(Some(span_styler()), cx);
                state
            });
            *slot.borrow_mut() = Some(input.clone());
            let view = cx.new(|_| Field { input });
            Root::new(view, window, cx)
        }
    });
    let input = slot.borrow().clone().expect("the field opened");
    test(&mut cx, handle.into(), input);
    cx.quit();
}

fn caret(input: &Entity<TextareaState>, at: usize, cx: &gpui_kit::App) -> Bounds<Pixels> {
    input
        .read(cx)
        .range_to_bounds(&(at..at))
        .unwrap_or_else(|| panic!("offset {at} is laid out"))
}

fn load(
    input: &Entity<TextareaState>,
    doc: &ComposerDoc,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) {
    let content = doc_content(doc);
    input.update(cx, |state, cx| state.set_value(content, window, cx));
    window.render_frame(cx);
    window.render_frame(cx);
}

fn park(input: &Entity<TextareaState>, at: usize, window: &mut Window, cx: &mut gpui_kit::App) {
    input.update(cx, |state, cx| state.set_selected_range(at..at, cx));
    window.render_frame(cx);
}

fn span(text: &str, part: &str, tag: ComposerTag) -> DocSpan {
    let start = text.find(part).expect("part in text");
    DocSpan::new(start..start + part.len(), tag)
}

fn doc(text: &str, spans: Vec<DocSpan>, emoji: Vec<DocEmoji>) -> ComposerDoc {
    let mut doc = ComposerDoc::plain(text);
    doc.spans = spans;
    doc.emoji = emoji;
    doc.normalize();
    doc
}

#[test]
fn the_caret_and_selection_cross_formatted_runs_one_character_at_a_time() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            let text = "plain bold both italic end";
            let d = doc(
                text,
                vec![
                    span(text, "bold both", ComposerTag::Bold),
                    span(text, "both italic", ComposerTag::Italic),
                ],
                vec![],
            );
            load(&input, &d, window, cx);
            assert_eq!(field_doc(input.read(cx)), d);
            park(&input, 0, window, cx);
            let mut xs = vec![caret(&input, 0, cx).left()];
            for step in 1..=text.len() {
                window.press("right", cx);
                assert_eq!(input.read(cx).cursor(), step, "one character per step");
                xs.push(caret(&input, step, cx).left());
            }
            for pair in xs.windows(2) {
                assert!(
                    pair[1] > pair[0],
                    "the caret moves right at every step: {xs:?}"
                );
            }
            // A selection across both formats is one rectangle on the row,
            // from caret to caret.
            park(&input, 4, window, cx);
            for _ in 0..14 {
                window.press("shift-right", cx);
            }
            assert_eq!(input.read(cx).selected_range(), 4..18);
            let rects = input.read(cx).range_to_rects(&(4..18));
            assert_eq!(rects.len(), 1, "{rects:?}");
            assert!((rects[0].left() - caret(&input, 4, cx).left()).abs() < px(0.5));
            assert!((rects[0].right() - caret(&input, 18, cx).left()).abs() < px(0.5));
            // Typing over the selection: the new text takes the format of
            // the first selected character (plain here), the rest keep theirs.
            window.input("X", cx);
            window.render_frame(cx);
            let after = field_doc(input.read(cx));
            assert_eq!(after.text, "plaiXalic end");
            assert_eq!(
                after.spans,
                [DocSpan::new(5..9, ComposerTag::Italic)],
                "{after:?}"
            );
        })
        .unwrap();
    });
}

#[test]
fn a_custom_emoji_is_one_caret_stop() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            // A plain emoji and one with a skin tone (two code points).
            for fallback in ["😀", "👍🏽"] {
                let text = format!("a{fallback}b");
                let end = 1 + fallback.len();
                let d = doc(
                    &text,
                    vec![],
                    vec![DocEmoji {
                        range: 1..end,
                        custom_emoji_id: 77,
                    }],
                );
                load(&input, &d, window, cx);
                assert_eq!(input.read(cx).tokens().len(), 1);
                park(&input, 0, window, cx);
                let mut stops = Vec::new();
                for _ in 0..3 {
                    window.press("right", cx);
                    stops.push(input.read(cx).cursor());
                }
                assert_eq!(stops, [1, end, end + 1], "{fallback}");
                window.press("left", cx);
                window.press("left", cx);
                assert_eq!(input.read(cx).cursor(), 1);
                // The emoji is as wide as the element drawn for it.
                let width = caret(&input, end, cx).left() - caret(&input, 1, cx).left();
                assert!((width - px(TOKEN_WIDTH)).abs() < px(0.5), "{width:?}");
                // Backspace takes it whole; undo brings it back.
                park(&input, end, window, cx);
                window.press("backspace", cx);
                assert_eq!(input.read(cx).value().as_ref(), "ab");
                assert!(input.read(cx).tokens().is_empty());
                window.press("cmd-z", cx);
                window.render_frame(cx);
                assert_eq!(input.read(cx).value().as_ref(), text);
                assert_eq!(field_doc(input.read(cx)).emoji, d.emoji);
            }
        })
        .unwrap();
    });
}

#[test]
fn a_custom_emoji_in_an_arabic_paragraph_is_placed_and_crossed_in_visual_order() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            let text = "مرحبا 😀 بك";
            let start = text.find('😀').unwrap();
            let end = start + '😀'.len_utf8();
            let d = doc(
                text,
                vec![span(text, "بك", ComposerTag::Bold)],
                vec![DocEmoji {
                    range: start..end,
                    custom_emoji_id: 5,
                }],
            );
            load(&input, &d, window, cx);
            let field = window.find("composer").bounds();
            // Right-to-left: the paragraph starts at the right edge.
            let first = caret(&input, 0, cx).left();
            assert!(first > field.right() - px(24.), "{first:?} in {field:?}");
            // The emoji element sits in the emoji's own box, not at the box's
            // right edge (where its start offset's caret is).
            let rects = input.read(cx).range_to_rects(&(start..end));
            assert_eq!(rects.len(), 1);
            let drawn = window.find("emoji").bounds();
            assert!(
                (drawn.left() - rects[0].left()).abs() < px(0.5),
                "emoji drawn at {drawn:?}, its box is {:?}",
                rects[0]
            );
            assert!((rects[0].size.width - px(TOKEN_WIDTH)).abs() < px(0.5));
            // Left goes forward in a right-to-left run: one press crosses
            // the emoji.
            park(&input, start, window, cx);
            window.press("left", cx);
            assert_eq!(input.read(cx).cursor(), end);
            window.press("right", cx);
            assert_eq!(input.read(cx).cursor(), start);
            // Typing more Arabic at the end keeps moving the caret left.
            let len = text.len();
            park(&input, len, window, cx);
            let before = caret(&input, len, cx).left();
            window.input("ا", cx);
            window.render_frame(cx);
            let after = caret(&input, input.read(cx).cursor(), cx).left();
            assert!(after < before, "{after:?} < {before:?}");
            // The bold word continued over the typed letter.
            let bold = field_doc(input.read(cx)).spans;
            assert_eq!(bold, [DocSpan::new(len - 4..len + 2, ComposerTag::Bold)]);
        })
        .unwrap();
    });
}

#[test]
fn a_mention_tag_survives_edits_around_and_inside_it() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            let text = "Hi Ann, see you";
            let d = doc(
                text,
                vec![span(text, "Ann", ComposerTag::Mention(42))],
                vec![],
            );
            load(&input, &d, window, cx);
            let mention = |input: &Entity<TextareaState>, cx: &gpui_kit::App| {
                field_doc(input.read(cx))
                    .spans
                    .into_iter()
                    .filter(|s| s.tag == ComposerTag::Mention(42))
                    .map(|s| (s.range.start, s.range.end))
                    .collect::<Vec<_>>()
            };
            // Text typed before it moves it.
            park(&input, 0, window, cx);
            window.input("Oh ", cx);
            window.render_frame(cx);
            assert_eq!(mention(&input, cx), [(6, 9)]);
            // A comma typed right after the name does not join it.
            park(&input, 9, window, cx);
            window.input(";", cx);
            window.render_frame(cx);
            assert_eq!(mention(&input, cx), [(6, 9)]);
            // Deleting a letter inside keeps the rest tagged.
            park(&input, 8, window, cx);
            window.press("backspace", cx);
            assert_eq!(input.read(cx).value().as_ref(), "Oh Hi An;, see you");
            assert_eq!(mention(&input, cx), [(6, 8)]);
            // It is still sent as a mention of user 42.
            let markup = field_doc(input.read(cx)).to_markup();
            assert_eq!(markup, "Oh Hi [An](tg://user?id=42);, see you");
            let (_, entities) = quill::composer::parse_format_markup(&markup);
            assert_eq!(entities[0].kind, quill::composer::FormatKind::MentionName);
            // Undo walks back through the edits, tag included.
            window.press("cmd-z", cx);
            window.render_frame(cx);
            assert_eq!(mention(&input, cx), [(6, 9)]);
            // Deleting the whole name removes the tag.
            input.update(cx, |state, cx| state.set_selected_range(6..9, cx));
            window.press("backspace", cx);
            assert!(mention(&input, cx).is_empty());
        })
        .unwrap();
    });
}

#[test]
fn typed_markdown_becomes_formatting_and_backspace_takes_it_back() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            let mut prev = String::new();
            let mut revert = None;
            for ch in "say **hi**".chars() {
                window.input(&ch.to_string(), cx);
                revert = input.update(cx, |state, cx| apply_markdown(state, &prev, window, cx));
                prev = input.read(cx).value().to_string();
                window.render_frame(cx);
            }
            let replaced = field_doc(input.read(cx));
            assert_eq!(replaced.text, "say hi");
            assert_eq!(replaced.spans, [DocSpan::new(4..6, ComposerTag::Bold)]);
            assert!(revert.is_some());
            assert_eq!(input.read(cx).cursor(), 6);
            // What follows is plain.
            window.input("!", cx);
            window.render_frame(cx);
            assert_eq!(
                field_doc(input.read(cx)).spans,
                [DocSpan::new(4..6, ComposerTag::Bold)]
            );
            // Undo removes the "!", then puts the markers back.
            window.press("cmd-z", cx);
            window.press("cmd-z", cx);
            window.render_frame(cx);
            assert_eq!(input.read(cx).value().as_ref(), "say **hi**");
            assert!(input.read(cx).spans().is_empty());
        })
        .unwrap();
    });
}

#[test]
fn ime_composition_inside_a_bold_word_stays_bold() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            let d = doc("bold", vec![DocSpan::new(0..4, ComposerTag::Bold)], vec![]);
            load(&input, &d, window, cx);
            park(&input, 2, window, cx);
            for marked in ["か", "かん"] {
                input.update(cx, |state, cx| {
                    let range = state.marked_text_range(window, cx);
                    state.replace_and_mark_text_in_range(range, marked, None, window, cx);
                });
                window.render_frame(cx);
            }
            input.update(cx, |state, cx| {
                let range = state.marked_text_range(window, cx);
                state.replace_text_in_range(range, "漢", window, cx);
                state.unmark_text(window, cx);
            });
            window.render_frame(cx);
            let after = field_doc(input.read(cx));
            assert_eq!(after.text, "bo漢ld");
            assert_eq!(after.spans, [DocSpan::new(0..7, ComposerTag::Bold)]);
        })
        .unwrap();
    });
}

#[test]
fn markup_loads_into_the_field_and_reads_back_the_same() {
    with_field(|cx, handle, input| {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let markup =
                "**bold _both_** [Ann](tg://user?id=3) ![😀](tg://emoji?id=9) `code`\n> quoted";
            let doc = ComposerDoc::from_markup(markup);
            load(&input, &doc, window, cx);
            assert_eq!(input.read(cx).tokens().len(), 1);
            assert_eq!(field_doc(input.read(cx)).to_markup(), markup);
            // An emoji token's text is its fallback, as the content says.
            let content = InputContent::new("😀")
                .with_token(0..4, InlineToken::new("custom-emoji:1", "😀"))
                .unwrap();
            input.update(cx, |state, cx| state.set_value(content, window, cx));
            assert_eq!(
                field_doc(input.read(cx)).to_markup(),
                "![😀](tg://emoji?id=1)"
            );
        })
        .unwrap();
    });
}

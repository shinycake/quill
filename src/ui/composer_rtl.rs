//! Right-to-left typing in the composer (codex:rtl-composer), after
//! Telegram Desktop's `InputField` (Qt `QTextEdit` with
//! `Qt::LayoutDirectionAuto`): each paragraph takes the direction of its first
//! strong character, a Hebrew or Arabic paragraph is right-aligned, its caret
//! starts at the right edge and advances leftwards, and left/right arrows
//! follow the glyphs.
//!
//! The layout lives in the bidi-aware input engine (`third_party/gpui-base`,
//! see `docs/decisions/codex-rtl-composer.md`); these tests drive the real
//! kit `Textarea` through it. They need gpui-kit's `test-support`, which the
//! `demo-capture` feature enables:
//! `cargo test --features demo-capture --bin quill composer_rtl`.

#[cfg(all(test, feature = "demo-capture"))]
pub(super) mod tests {
    use gpui_kit::component::Root;
    use gpui_kit::component::input::{Textarea, TextareaState};
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AnyWindowHandle, AppContext, Bounds, Context, Entity, EntityInputHandler,
        InteractiveElement, IntoElement, ParentElement, Pixels, Render, Styled, TestAppContext,
        Window, div, point, px, size,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    /// A text system that shapes like CoreText and cosmic-text do: glyphs come
    /// back in *visual* order, runs of a right-to-left level reversed, every
    /// character `0.6 em` wide. gpui's own test text system lays characters
    /// out in typing order and so cannot show bidi at all. `base_rtl` forces
    /// the paragraph direction (DirectWrite always lays out left to right:
    /// `Some(false)`); `None` takes it from the first strong character.
    pub(in crate::ui) struct BidiTextSystem {
        pub(in crate::ui) base_rtl: Option<bool>,
    }

    thread_local! {
        /// Every string handed to the text system, to see which pieces the bidi
        /// layout shapes (a platform picks the colour emoji font per shaped string).
        static SHAPED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    impl gpui_kit::PlatformTextSystem for BidiTextSystem {
        fn add_fonts(&self, fonts: Vec<std::borrow::Cow<'static, [u8]>>) -> gpui_kit::Result<()> {
            gpui_kit::NoopTextSystem.add_fonts(fonts)
        }
        fn all_font_names(&self) -> Vec<String> {
            Vec::new()
        }
        fn font_id(&self, descriptor: &gpui_kit::Font) -> gpui_kit::Result<gpui_kit::FontId> {
            gpui_kit::NoopTextSystem.font_id(descriptor)
        }
        fn font_metrics(&self, font_id: gpui_kit::FontId) -> gpui_kit::FontMetrics {
            gpui_kit::NoopTextSystem.font_metrics(font_id)
        }
        fn typographic_bounds(
            &self,
            font_id: gpui_kit::FontId,
            glyph_id: gpui_kit::GlyphId,
        ) -> gpui_kit::Result<Bounds<f32>> {
            gpui_kit::NoopTextSystem.typographic_bounds(font_id, glyph_id)
        }
        fn advance(
            &self,
            font_id: gpui_kit::FontId,
            glyph_id: gpui_kit::GlyphId,
        ) -> gpui_kit::Result<gpui_kit::Size<f32>> {
            gpui_kit::NoopTextSystem.advance(font_id, glyph_id)
        }
        fn glyph_for_char(&self, font_id: gpui_kit::FontId, ch: char) -> Option<gpui_kit::GlyphId> {
            gpui_kit::NoopTextSystem.glyph_for_char(font_id, ch)
        }
        fn glyph_raster_bounds(
            &self,
            params: &gpui_kit::RenderGlyphParams,
        ) -> gpui_kit::Result<Bounds<gpui_kit::DevicePixels>> {
            gpui_kit::NoopTextSystem.glyph_raster_bounds(params)
        }
        fn rasterize_glyph(
            &self,
            params: &gpui_kit::RenderGlyphParams,
            raster_bounds: Bounds<gpui_kit::DevicePixels>,
        ) -> gpui_kit::Result<(gpui_kit::Size<gpui_kit::DevicePixels>, Vec<u8>)> {
            gpui_kit::NoopTextSystem.rasterize_glyph(params, raster_bounds)
        }
        fn recommended_rendering_mode(
            &self,
            font_id: gpui_kit::FontId,
            font_size: Pixels,
        ) -> gpui_kit::TextRenderingMode {
            gpui_kit::NoopTextSystem.recommended_rendering_mode(font_id, font_size)
        }
        fn layout_line(
            &self,
            text: &str,
            font_size: Pixels,
            runs: &[gpui_kit::FontRun],
        ) -> gpui_kit::LineLayout {
            SHAPED.with(|shaped| shaped.borrow_mut().push(text.to_string()));
            let mut layout = gpui_kit::NoopTextSystem.layout_line(text, font_size, runs);
            if text.is_empty() {
                return layout;
            }
            let advance = font_size * 0.6;
            let base = self.base_rtl.map(|rtl| {
                if rtl {
                    unicode_bidi::RTL_LEVEL
                } else {
                    unicode_bidi::LTR_LEVEL
                }
            });
            let info = unicode_bidi::ParagraphBidiInfo::new(text, base);
            let (levels, visual) = info.visual_runs(0..text.len());
            let mut glyphs = Vec::new();
            let mut x = px(0.);
            for run in visual {
                let mut chars: Vec<(usize, char)> = text[run.clone()]
                    .char_indices()
                    .map(|(i, c)| (run.start + i, c))
                    .collect();
                if levels[run.start].is_rtl() {
                    chars.reverse();
                }
                for (index, ch) in chars {
                    glyphs.push(gpui_kit::ShapedGlyph {
                        id: gpui_kit::GlyphId(ch.len_utf16() as u32),
                        position: point(x, px(0.)),
                        index,
                        is_emoji: false,
                    });
                    x += advance;
                }
            }
            layout.width = x;
            layout.runs = vec![gpui_kit::ShapedRun {
                font_id: gpui_kit::FontId(0),
                glyphs,
            }];
            layout
        }
    }

    struct Composer {
        input: Entity<TextareaState>,
    }

    impl Render for Composer {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().p_4().child(
                div()
                    .id("composer")
                    .test_support()
                    .w(px(400.))
                    .child(Textarea::new(&self.input).appearance(false).bordered(false)),
            )
        }
    }

    /// A composer-like Textarea in a window whose text system shapes bidi text
    /// the way real platforms do (see [`BidiTextSystem`]); runs `test` with the
    /// window handle and the input state.
    fn with_composer(
        base_rtl: Option<bool>,
        test: impl FnOnce(&mut TestAppContext, AnyWindowHandle, Entity<TextareaState>),
    ) {
        let dispatcher = gpui_kit::TestDispatcher::new(0);
        let mut cx = TestAppContext::build_with_text_system(
            dispatcher,
            None,
            Arc::new(BidiTextSystem { base_rtl }),
        );
        cx.update(gpui_kit::init);
        let slot: Rc<RefCell<Option<Entity<TextareaState>>>> = Rc::new(RefCell::new(None));
        let handle = cx.open_window(size(px(640.), px(240.)), {
            let slot = slot.clone();
            move |window, cx| {
                let input = cx.new(|cx| TextareaState::new(window, cx));
                *slot.borrow_mut() = Some(input.clone());
                let view = cx.new(|_| Composer { input });
                Root::new(view, window, cx)
            }
        });
        let input = slot.borrow().clone().expect("the composer opened");
        test(&mut cx, handle.into(), input);
        cx.quit();
    }

    /// The caret rectangle at byte offset `at`.
    fn caret(input: &Entity<TextareaState>, at: usize, cx: &gpui_kit::App) -> Bounds<Pixels> {
        input
            .read(cx)
            .range_to_bounds(&(at..at))
            .unwrap_or_else(|| panic!("offset {at} is laid out"))
    }

    fn set_text(
        input: &Entity<TextareaState>,
        text: &str,
        window: &mut Window,
        cx: &mut gpui_kit::App,
    ) {
        input.update(cx, |state, cx| state.set_value(text, window, cx));
        window.render_frame(cx);
        window.render_frame(cx);
    }

    fn type_char(window: &mut Window, ch: char, cx: &mut gpui_kit::App) {
        window.input(&ch.to_string(), cx);
        window.render_frame(cx);
    }

    #[test]
    fn hebrew_text_is_right_anchored_and_the_caret_moves_left() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                let field = window.find("composer").bounds();

                // Empty: the caret rests at the left, as in any LTR field.
                let empty = caret(&input, 0, cx).left();
                assert!(empty < field.left() + px(24.), "{empty:?} in {field:?}");

                let mut xs = Vec::new();
                for ch in "שלום עולם".chars() {
                    type_char(window, ch, cx);
                    let cursor = input.read(cx).cursor();
                    xs.push(caret(&input, cursor, cx).left());
                }
                // Every typed character moves the caret further left.
                for pair in xs.windows(2) {
                    assert!(pair[1] < pair[0], "caret must move left: {xs:?}");
                }
                // The text hugs the right edge: the caret before the first
                // character sits there, within the caret's own margin.
                let start = caret(&input, 0, cx).left();
                assert!(
                    start > field.right() - px(24.) && start <= field.right(),
                    "text starts at the right edge: {start:?} in {field:?}"
                );
            })
            .unwrap();
        });
    }

    #[test]
    fn latin_text_still_starts_at_the_left_and_the_caret_moves_right() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                let field = window.find("composer").bounds();
                let mut xs = Vec::new();
                for ch in "hello world".chars() {
                    type_char(window, ch, cx);
                    let cursor = input.read(cx).cursor();
                    xs.push(caret(&input, cursor, cx).left());
                }
                for pair in xs.windows(2) {
                    assert!(pair[1] > pair[0], "caret must move right: {xs:?}");
                }
                assert!(caret(&input, 0, cx).left() < field.left() + px(24.));
            })
            .unwrap();
        });
    }

    #[test]
    fn each_paragraph_takes_its_own_direction() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let text = "שלום עולם\nhello world\n123";
                set_text(&input, text, window, cx);
                let field = window.find("composer").bounds();
                let second = text.find("hello").unwrap();
                let third = text.find("123").unwrap();

                let hebrew = caret(&input, 0, cx);
                let english = caret(&input, second, cx);
                let digits = caret(&input, third, cx);
                assert!(hebrew.left() > field.center().x, "{hebrew:?}");
                assert!(english.left() < field.left() + px(24.), "{english:?}");
                // A paragraph with no strong character is left to right.
                assert!(digits.left() < field.left() + px(24.), "{digits:?}");
                // Three paragraphs, three rows.
                assert!(hebrew.top() < english.top() && english.top() < digits.top());
            })
            .unwrap();
        });
    }

    #[test]
    fn mixed_runs_are_laid_out_in_visual_order() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let text = "שלום hello 123 עולם";
                set_text(&input, text, window, cx);
                let hello = text.find("hello").unwrap();
                let digits = text.find("123").unwrap();
                let world = text.find("עולם").unwrap();

                let x = |at: usize| caret(&input, at, cx).left();
                // Right-to-left paragraph: reading order runs right to left.
                assert!(x(0) > x(hello), "the first word is rightmost");
                assert!(x(hello) > x(world), "the last word is leftmost");
                // The digits continue the Latin word's left-to-right run (W7),
                // so they read rightwards from it.
                assert!(x(digits) > x(hello));
                assert!(x(hello + 3) > x(hello));
                assert!(x(digits + 2) > x(digits));
                // The end of the paragraph is at its left end.
                assert!(x(text.len()) <= x(world));
            })
            .unwrap();
        });
    }

    #[test]
    fn layout_does_not_depend_on_the_platform_base_direction() {
        // DirectWrite lays every line out left to right whatever it holds; the
        // engine shapes single-direction runs, so the result must match the
        // platforms that pick the direction from the text.
        let text = "שלום hello 123 עולם";
        let measure = |base: Option<bool>| {
            let out: Rc<RefCell<Vec<Pixels>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = out.clone();
            with_composer(base, move |cx, handle, input| {
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    set_text(&input, text, window, cx);
                    for (at, _) in text.char_indices() {
                        sink.borrow_mut().push(caret(&input, at, cx).left());
                    }
                })
                .unwrap();
            });
            out.take()
        };
        assert_eq!(measure(None), measure(Some(false)));
    }

    #[test]
    fn arrows_follow_the_glyphs_in_a_right_to_left_paragraph() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, "אבג", window, cx);
                input.update(cx, |state, cx| state.set_selected_range(0..0, cx));
                window.render_frame(cx);

                // At the right edge: Right has nowhere to go, Left steps onto
                // the next letter, which comes later in typing order.
                window.press("right", cx);
                assert_eq!(input.read(cx).cursor(), 0);
                window.press("left", cx);
                assert_eq!(input.read(cx).cursor(), 2);
                window.press("left", cx);
                assert_eq!(input.read(cx).cursor(), 4);
                window.press("right", cx);
                assert_eq!(input.read(cx).cursor(), 2);

                // Shift+Left extends the selection leftwards on screen.
                input.update(cx, |state, cx| state.set_selected_range(0..0, cx));
                window.render_frame(cx);
                window.press("shift-left", cx);
                window.press("shift-left", cx);
                assert_eq!(input.read(cx).selected_range(), 0..4);
            })
            .unwrap();
        });
    }

    #[test]
    fn arrows_are_logical_in_a_left_to_right_paragraph() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, "abc", window, cx);
                input.update(cx, |state, cx| state.set_selected_range(0..0, cx));
                window.render_frame(cx);
                window.press("right", cx);
                assert_eq!(input.read(cx).cursor(), 1);
                window.press("left", cx);
                assert_eq!(input.read(cx).cursor(), 0);
            })
            .unwrap();
        });
    }

    #[test]
    fn arrows_walk_a_mixed_paragraph_across_every_caret_stop() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                let text = "אב hello גד";
                set_text(&input, text, window, cx);
                // Park the caret at the visual left edge, then press Right
                // until it stops: it must pass strictly rightwards.
                let left_edge = (0..=text.len())
                    .filter(|&b| text.is_char_boundary(b))
                    .min_by(|&a, &b| {
                        caret(&input, a, cx)
                            .left()
                            .partial_cmp(&caret(&input, b, cx).left())
                            .unwrap()
                    })
                    .unwrap();
                input.update(cx, |state, cx| {
                    state.set_selected_range(left_edge..left_edge, cx)
                });
                window.render_frame(cx);
                let mut x = caret(&input, left_edge, cx).left();
                let mut stops = 0;
                loop {
                    let before = input.read(cx).cursor();
                    window.press("right", cx);
                    window.render_frame(cx);
                    let after = input.read(cx).cursor();
                    if after == before {
                        break;
                    }
                    // Drawn where the caret really is (a boundary offset has two places).
                    let nx = input.read(cx).caret_bounds().unwrap().left();
                    assert!(nx > x, "Right moves rightwards: {x:?} -> {nx:?}");
                    x = nx;
                    stops += 1;
                    assert!(stops < 40, "arrows must terminate");
                }
                assert!(stops >= 6, "visited the stops of the line: {stops}");
            })
            .unwrap();
        });
    }

    #[test]
    fn clicking_places_the_caret_at_the_glyph_under_the_pointer() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, "שלום", window, cx);
                let field = window.find("composer").bounds();
                // Aim at the boundary between the second and third letters.
                let at = caret(&input, 4, cx);
                window.click_at(
                    "composer",
                    point(at.left() - field.left(), at.center().y - field.top()),
                    cx,
                );
                assert_eq!(input.read(cx).cursor(), 4);
                // The far right is the start of the text, the far left of the
                // field is past its end.
                window.click_at("composer", point(field.size.width - px(2.), px(8.)), cx);
                assert_eq!(input.read(cx).cursor(), 0);
                window.click_at("composer", point(px(2.), px(8.)), cx);
                assert_eq!(input.read(cx).cursor(), "שלום".len());
            })
            .unwrap();
        });
    }

    #[test]
    fn ime_composition_in_hebrew_grows_leftwards() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                let mut xs = Vec::new();
                let mut composing = String::new();
                for ch in "שלום".chars() {
                    composing.push(ch);
                    let marked = composing.clone();
                    input.update(cx, |state, cx| {
                        let range = state.marked_text_range(window, cx);
                        state.replace_and_mark_text_in_range(range, &marked, None, window, cx);
                    });
                    window.render_frame(cx);
                    let end = input.read(cx).value().len();
                    xs.push(caret(&input, end, cx).left());
                    assert_eq!(input.read(cx).value().as_ref(), composing);
                }
                for pair in xs.windows(2) {
                    assert!(pair[1] < pair[0], "composition caret moves left: {xs:?}");
                }
                // Commit the composition.
                input.update(cx, |state, cx| {
                    let range = state.marked_text_range(window, cx);
                    state.replace_text_in_range(range, "שלום", window, cx);
                    state.unmark_text(window, cx);
                });
                window.render_frame(cx);
                assert_eq!(input.read(cx).value().as_ref(), "שלום");
                assert!(
                    input
                        .update(cx, |state, cx| state.marked_text_range(window, cx))
                        .is_none()
                );
            })
            .unwrap();
        });
    }

    #[test]
    fn word_rectangles_follow_the_drawn_word() {
        // The spellcheck underlines are painted from `range_to_rects`: a
        // Hebrew word is underlined where it is drawn, the first word at the
        // right edge, and a range crossing into Latin text splits in two.
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let text = "שלום עולם hello";
                set_text(&input, text, window, cx);
                let field = window.find("composer").bounds();

                let word = input.read(cx).range_to_rects(&(0.."שלום".len()));
                assert_eq!(word.len(), 1, "{word:?}");
                let width = word[0].size.width;
                assert!(
                    (width - px(4. * 8.4)).abs() < px(0.5),
                    "four letters wide: {width:?}"
                );
                assert!(
                    (word[0].right() - field.right()).abs() < px(24.),
                    "the first word ends at the right edge: {:?} in {field:?}",
                    word[0]
                );

                // The whole line is one rectangle: the pieces touch.
                let all = input.read(cx).range_to_rects(&(0..text.len()));
                assert_eq!(all.len(), 1, "touching pieces merge: {all:?}");

                // The last letter of the Hebrew word, the space and the start
                // of the Latin word: drawn as "hel" at the far left and the
                // letter plus space further right, with "lo" between them.
                let hello = text.find("hello").unwrap();
                let from = hello - " ".len() - "ם".len();
                let split = input.read(cx).range_to_rects(&(from..hello + 3));
                assert_eq!(split.len(), 2, "{split:?}");
                assert!(split[0].right() < split[1].left());
                assert!((split[0].size.width - px(3. * 8.4)).abs() < px(0.5));
                assert!((split[1].size.width - px(2. * 8.4)).abs() < px(0.5));
            })
            .unwrap();
        });
    }

    #[test]
    fn select_all_and_retyping_survive_mixed_text() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                let text = "שלום hello עולם";
                set_text(&input, text, window, cx);
                window.press("cmd-a", cx);
                assert_eq!(input.read(cx).selected_range(), 0..text.len());
                window.input("ש", cx);
                window.render_frame(cx);
                assert_eq!(input.read(cx).value().as_ref(), "ש");
                window.press("backspace", cx);
                assert_eq!(input.read(cx).value().as_ref(), "");
            })
            .unwrap();
        });
    }

    #[test]
    fn a_long_hebrew_paragraph_wraps_and_stays_right_aligned() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let text = "שלום ".repeat(24);
                set_text(&input, &text, window, cx);
                let field = window.find("composer").bounds();
                let first = caret(&input, 0, cx);
                let last = caret(&input, text.len() - 1, cx);
                assert!(last.top() > first.top(), "the paragraph wrapped");
                // Every row starts at the right edge.
                let mut row_starts = std::collections::BTreeMap::new();
                for (at, _) in text.char_indices() {
                    let c = caret(&input, at, cx);
                    row_starts
                        .entry((c.top() / px(1.)) as i32)
                        .and_modify(|x: &mut Pixels| *x = (*x).max(c.left()))
                        .or_insert(c.left());
                }
                for (row, x) in row_starts {
                    assert!(x > field.right() - px(24.), "row {row} starts at {x:?}");
                }
            })
            .unwrap();
        });
    }

    // --- message bubbles (`BidiParagraph`, see selectable_text.rs) ---

    use gpui_kit::base::input::bidi_paragraph::BidiParagraph;

    /// A paragraph laid out at `wrap` width with the default text style.
    fn paragraph(text: &str, wrap: f32, window: &mut Window) -> BidiParagraph {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let runs = vec![style.to_run(text.len())];
        BidiParagraph::layout(
            &text.to_string().into(),
            &runs,
            font_size,
            px(20.),
            Some(px(wrap)),
            window,
        )
        .expect("the text has right-to-left characters")
    }

    #[test]
    fn plain_text_is_left_to_gpui() {
        with_composer(None, |cx, handle, _| {
            cx.update_window(handle, |_, window, _| {
                let style = window.text_style();
                let size = style.font_size.to_pixels(window.rem_size());
                let runs = vec![style.to_run(5)];
                assert!(
                    BidiParagraph::layout(
                        &"hello".to_string().into(),
                        &runs,
                        size,
                        px(20.),
                        Some(px(100.)),
                        window
                    )
                    .is_none()
                );
            })
            .unwrap();
        });
    }

    #[test]
    fn a_long_hebrew_message_wraps_in_typing_order() {
        with_composer(None, |cx, handle, _| {
            cx.update_window(handle, |_, window, _| {
                // 5-glyph words at 8.4px: a 150px row holds about three of them.
                let words = [
                    "שלום",
                    "עולם",
                    "מה",
                    "שלומך",
                    "היום",
                    "זו",
                    "הודעה",
                    "ארוכה",
                ];
                let text = words.join(" ");
                let p = paragraph(&text, 150., window);
                let width = px(150.);
                // The first character is on the first row at the right edge...
                let first = p.position_for_index(0, width).unwrap();
                assert_eq!(first.y, px(0.));
                assert!((first.x - width).abs() < px(0.5), "{first:?}");
                // ...the last word is on the last row, and rows grow downward in
                // typing order (the end of the sentence is not on the first row).
                let mut last_y = px(-1.);
                for word in words {
                    let at = text.find(word).unwrap();
                    let y = p.position_for_index(at, width).unwrap().y;
                    assert!(y >= last_y, "{word} is on row y={y:?} after y={last_y:?}");
                    last_y = y;
                }
                assert!(last_y > px(0.), "the message wrapped");
                assert_eq!(p.size().height, px(20.) * ((last_y / px(20.)) + 1.));
                // Every row starts at the right edge.
                for word in words {
                    let at = text.find(word).unwrap();
                    let pos = p.position_for_index(at, width).unwrap();
                    assert!(pos.x <= width + px(0.5));
                }
            })
            .unwrap();
        });
    }

    #[test]
    fn a_short_rtl_message_measures_its_shaped_width_not_the_available_width() {
        // A bubble hugs its text: the measured width is the widest row, however wide
        // the wrap width is.
        with_composer(None, |cx, handle, _| {
            cx.update_window(handle, |_, window, _| {
                let text = "מחכה לעוד עדכונים ממנה";
                let p = paragraph(text, 560., window);
                let shaped = px(text.chars().count() as f32 * 9.6);
                assert!(
                    (p.size().width - shaped).abs() < px(0.5),
                    "{:?} vs {shaped:?}",
                    p.size().width
                );
                assert_eq!(p.size().height, px(20.));
                // And it still wraps at the wrap width when it must.
                let narrow = paragraph(text, 120., window);
                assert!(narrow.size().width <= px(120.));
                assert!(narrow.size().height > px(20.));
            })
            .unwrap();
        });
    }

    #[test]
    fn bubble_hit_testing_and_selection_follow_the_rows() {
        with_composer(None, |cx, handle, _| {
            cx.update_window(handle, |_, window, _| {
                let text = "שלום עולם מה שלומך היום זו הודעה ארוכה";
                let p = Rc::new(paragraph(text, 150., window));
                let width = px(150.);
                // A click on a caret position's glyph names that character.
                let at = text.find("מה").unwrap();
                let pos = p.position_for_index(at, width).unwrap();
                let hit = p.index_for_position(point(pos.x - px(2.), pos.y + px(8.)), width);
                assert_eq!(hit, Ok(at));
                // Rectangles of a word are where it is drawn: as wide as it is.
                let rects = p.range_rects(at..at + "מה".len(), width);
                assert_eq!(rects.len(), 1);
                assert!(
                    (rects[0].size.width - px(2. * 9.6)).abs() < px(0.5),
                    "{rects:?}"
                );
                // Dragging from the first row to the last selects the text in
                // between in typing order, not by x.
                let geometry = p.geometry(point(px(0.), px(0.)), width);
                let start = p.position_for_index(0, width).unwrap();
                let end = p.position_for_index(text.len(), width).unwrap();
                let all = geometry
                    .selected_range(
                        point(start.x - px(1.), start.y + px(8.)),
                        point(end.x + px(1.), end.y + px(8.)),
                    )
                    .unwrap();
                assert_eq!(all, Some(0..text.len()));
                // Nothing is selected above or below the paragraph.
                assert_eq!(
                    geometry.selected_range(point(px(10.), px(-80.)), point(px(20.), px(-40.))),
                    Some(None)
                );
            })
            .unwrap();
        });
    }

    #[test]
    fn bubble_rows_order_mixed_runs_visually_and_agree_across_platform_bases() {
        let text = "שלום hello 123 עולם";
        let measure = |base: Option<bool>| {
            let out: Rc<RefCell<Vec<Pixels>>> = Rc::new(RefCell::new(Vec::new()));
            let sink = out.clone();
            with_composer(base, move |cx, handle, _| {
                cx.update_window(handle, |_, window, _| {
                    let p = paragraph(text, 400., window);
                    for (at, _) in text.char_indices() {
                        sink.borrow_mut()
                            .push(p.position_for_index(at, px(400.)).unwrap().x);
                    }
                })
                .unwrap();
            });
            out.take()
        };
        let auto = measure(None);
        assert_eq!(auto, measure(Some(false)));
        // First word rightmost, last word leftmost, the Latin word's letters
        // advance rightwards.
        let hello = text.find("hello").unwrap();
        let world = text.find("עולם").unwrap();
        let x = |at: usize| auto[text.char_indices().position(|(i, _)| i == at).unwrap()];
        assert!(x(0) > x(hello) && x(hello) > x(world));
        assert!(x(hello + 3) > x(hello));
    }

    #[cfg(target_os = "macos")]
    const WORD_LEFT: &str = "alt-left";
    #[cfg(target_os = "macos")]
    const WORD_RIGHT: &str = "alt-right";
    #[cfg(not(target_os = "macos"))]
    const WORD_LEFT: &str = "ctrl-left";
    #[cfg(not(target_os = "macos"))]
    const WORD_RIGHT: &str = "ctrl-right";

    /// Presses `key` until the caret stops, returning each `(offset, hangs on the character
    /// before it)` stop the caret visited, starting with where it was, and the x it was
    /// drawn at.
    fn walk_keys(
        input: &Entity<TextareaState>,
        key: &str,
        window: &mut Window,
        cx: &mut gpui_kit::App,
    ) -> Vec<((usize, bool), Pixels)> {
        let here = |cx: &gpui_kit::App| {
            let state = input.read(cx);
            (
                (state.cursor(), state.caret_hangs_on_previous_character()),
                state.caret_bounds().expect("the caret is laid out").left(),
            )
        };
        let mut stops = vec![here(cx)];
        for _ in 0..40 {
            window.press(key, cx);
            window.render_frame(cx);
            let next = here(cx);
            if next.0 == stops.last().unwrap().0 {
                return stops;
            }
            stops.push(next);
        }
        panic!("the caret never stopped: {stops:?}");
    }

    fn park(input: &Entity<TextareaState>, at: usize, window: &mut Window, cx: &mut gpui_kit::App) {
        input.update(cx, |state, cx| state.set_selected_range(at..at, cx));
        window.render_frame(cx);
    }

    #[test]
    fn a_direction_boundary_offset_is_reached_from_both_sides() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                // "ab" then Hebrew: offset 2 is after "b" and before the Hebrew "ש".
                set_text(&input, "abשג", window, cx);
                park(&input, 0, window, cx);
                let right = walk_keys(&input, "right", window, cx);
                let stops: Vec<_> = right.iter().map(|(stop, _)| *stop).collect();
                // 0, 1, then offset 2 after "b", 4 inside the Hebrew run, and offset 2
                // again on the other side (before "ש", the run's right edge).
                assert_eq!(
                    stops,
                    vec![(0, false), (1, false), (2, true), (4, false), (2, false)]
                );
                // The caret moves strictly rightwards through all of them.
                for pair in right.windows(2) {
                    assert!(pair[1].1 > pair[0].1, "{right:?}");
                }
                // And back with Left, through the Hebrew run to the start.
                let left = walk_keys(&input, "left", window, cx);
                let stops: Vec<_> = left.iter().map(|(stop, _)| *stop).collect();
                assert_eq!(
                    stops,
                    vec![(2, false), (4, false), (6, false), (1, false), (0, false)]
                );
                for pair in left.windows(2) {
                    assert!(pair[1].1 < pair[0].1, "{left:?}");
                }
            })
            .unwrap();
        });
    }

    #[test]
    fn typing_at_a_boundary_stop_goes_to_the_side_the_caret_hangs_on() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, "abשג", window, cx);
                park(&input, 0, window, cx);
                window.press("right", cx);
                window.press("right", cx);
                let state = input.read(cx);
                assert_eq!(
                    (state.cursor(), state.caret_hangs_on_previous_character()),
                    (2, true)
                );
                // The caret after "b" is where an inserted letter lands next to "b", and
                // an edit places the caret by offset again.
                type_char(window, 'c', cx);
                assert_eq!(input.read(cx).value().as_ref(), "abcשג");
                assert!(!input.read(cx).caret_hangs_on_previous_character());
            })
            .unwrap();
        });
    }

    #[test]
    fn clicking_picks_the_side_of_a_direction_boundary() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, "abשג", window, cx);
                let field = window.find("composer").bounds();
                // The two places of offset 2: after "b", and at the Hebrew run's right edge.
                park(&input, 0, window, cx);
                window.press("right", cx);
                window.press("right", cx);
                window.render_frame(cx);
                let after_b = input.read(cx).caret_bounds().unwrap();
                park(&input, 2, window, cx);
                let hebrew_right = input.read(cx).caret_bounds().unwrap().left();
                let after_b_x = after_b.left();
                assert!(hebrew_right > after_b_x);
                let y = after_b.center().y - field.top();
                let after_b = after_b_x;
                window.click_at("composer", point(after_b - field.left() - px(1.), y), cx);
                let state = input.read(cx);
                assert_eq!(
                    (state.cursor(), state.caret_hangs_on_previous_character()),
                    (2, true)
                );
                let drawn = state.caret_bounds().unwrap().left();
                assert!((drawn - after_b).abs() < px(1.), "{drawn:?} vs {after_b:?}");
                window.click_at(
                    "composer",
                    point(hebrew_right - field.left() + px(1.), y),
                    cx,
                );
                let state = input.read(cx);
                assert_eq!(
                    (state.cursor(), state.caret_hangs_on_previous_character()),
                    (2, false)
                );
                let drawn = state.caret_bounds().unwrap().left();
                assert!(
                    (drawn - hebrew_right).abs() < px(1.),
                    "{drawn:?} vs {hebrew_right:?}"
                );
            })
            .unwrap();
        });
    }

    #[test]
    fn word_movement_follows_the_runs_of_a_mixed_paragraph() {
        // Visually, left to right: עולם  hello  שלום (a right-to-left paragraph).
        let text = "שלום hello עולם";
        let hello = text.find("hello").unwrap();
        let world = text.find("עולם").unwrap();
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, text, window, cx);
                park(&input, 0, window, cx);
                // From the right edge, each press goes one word to the left on screen,
                // whichever way the word reads.
                let left = walk_keys(&input, WORD_LEFT, window, cx);
                let offsets: Vec<_> = left.iter().map(|((o, _), _)| *o).collect();
                assert_eq!(
                    offsets,
                    vec![0, "שלום".len(), hello, text.len()],
                    "{left:?}"
                );
                for pair in left.windows(2) {
                    assert!(pair[1].1 < pair[0].1, "{left:?}");
                }
                // And back to the right.
                let right = walk_keys(&input, WORD_RIGHT, window, cx);
                for pair in right.windows(2) {
                    assert!(pair[1].1 > pair[0].1, "{right:?}");
                }
                assert_eq!(right.len(), 4, "{right:?}");
                assert_eq!(right.last().unwrap().0.0, 0);
                assert_eq!(right[1].0.0, world);
            })
            .unwrap();
        });
    }

    #[test]
    fn word_movement_in_a_latin_paragraph_crosses_a_hebrew_word_by_its_run() {
        // "one שלום two": Latin paragraph holding a Hebrew word.
        let text = "one שלום two";
        let hebrew = text.find("שלום").unwrap();
        let two = text.find("two").unwrap();
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, text, window, cx);
                park(&input, 0, window, cx);
                let right = walk_keys(&input, WORD_RIGHT, window, cx);
                for pair in right.windows(2) {
                    assert!(pair[1].1 > pair[0].1, "moves right on screen: {right:?}");
                }
                let offsets: Vec<_> = right.iter().map(|((o, _), _)| *o).collect();
                // "one", the Hebrew word (entered at its left edge, i.e. its logical end,
                // and left at its right edge), then "two".
                assert_eq!(offsets.len(), 4, "{right:?}");
                assert_eq!(offsets[1], 3);
                assert_eq!(offsets[2], hebrew);
                assert_eq!(offsets[3], text.len());
                let _ = two;
            })
            .unwrap();
        });
    }

    #[test]
    fn plain_paragraphs_keep_logical_word_movement() {
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, "one two three", window, cx);
                park(&input, 0, window, cx);
                let right = walk_keys(&input, WORD_RIGHT, window, cx);
                let offsets: Vec<_> = right.iter().map(|((o, _), _)| *o).collect();
                assert_eq!(offsets, vec![0, 3, 7, 13]);
            })
            .unwrap();
        });
    }

    #[test]
    fn selecting_by_word_extends_along_the_runs() {
        let text = "שלום hello עולם";
        with_composer(None, |cx, handle, input| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("composer", cx);
                set_text(&input, text, window, cx);
                park(&input, 0, window, cx);
                let key = format!("shift-{WORD_LEFT}");
                window.press(&key, cx);
                assert_eq!(input.read(cx).selected_range(), 0.."שלום".len());
            })
            .unwrap();
        });
    }

    #[test]
    fn a_hebrew_preview_is_elided_at_its_logical_end() {
        with_composer(None, |cx, handle, _| {
            cx.update_window(handle, |_, window, _| {
                let text = "שלום עולם מה שלומך היום זו הודעה ארוכה מאוד";
                let style = window.text_style();
                let size = style.font_size.to_pixels(window.rem_size());
                let runs = vec![style.to_run(text.len())];
                let shared: gpui_kit::SharedString = text.to_string().into();
                let line = |width: f32, window: &mut Window| {
                    BidiParagraph::layout_one_line(&shared, &runs, size, px(20.), px(width), window)
                        .expect("the text has right-to-left characters")
                };
                // Roomy: nothing elided, the line is as wide as the text.
                let whole = line(2000., window);
                assert!(whole.width() < px(2000.));
                assert!(whole.is_rtl());
                // Narrow: it fits, starts at the right edge like the sentence does, and
                // ends (at its left) in the ellipsis; a longer allowance keeps more text.
                let narrow = line(120., window);
                let wider = line(200., window);
                assert!(narrow.width() <= px(120.), "{:?}", narrow.width());
                assert!(wider.width() <= px(200.) && wider.width() > narrow.width());
                assert!(narrow.is_rtl());
                // Position 0 is the first word, at the right edge of the allowance.
                let first = narrow.position_for_index(0, px(120.)).unwrap();
                assert!(first.x > px(100.), "{first:?}");
                // The ellipsis is the last character: leftmost.
                let end = narrow.position_for_index(narrow.len(), px(120.)).unwrap();
                assert!(end.x < px(20.), "{end:?}");
                // Latin text with no Hebrew in sight is left to GPUI.
                let latin: gpui_kit::SharedString = "hello world".to_string().into();
                assert!(
                    BidiParagraph::layout_one_line(
                        &latin,
                        &[style.to_run(11)],
                        size,
                        px(20.),
                        px(50.),
                        window
                    )
                    .is_none()
                );
            })
            .unwrap();
        });
    }

    #[test]
    fn a_latin_preview_with_hebrew_late_is_elided_at_the_right() {
        with_composer(None, |cx, handle, _| {
            cx.update_window(handle, |_, window, _| {
                let text = "meeting at noon שלום עולם";
                let style = window.text_style();
                let size = style.font_size.to_pixels(window.rem_size());
                let runs = vec![style.to_run(text.len())];
                let shared: gpui_kit::SharedString = text.to_string().into();
                let line =
                    BidiParagraph::layout_one_line(&shared, &runs, size, px(20.), px(90.), window)
                        .expect("has right-to-left characters");
                // A Latin-first line stays left-aligned: its start at the left edge.
                assert!(!line.is_rtl());
                assert!(line.width() <= px(90.));
                assert!(line.position_for_index(0, px(90.)).unwrap().x < px(2.));
            })
            .unwrap();
        });
    }

    // --- emoji in right-to-left text (codex:rtl-emoji) ---

    /// The emoji of the report and its relatives: variation selector, skin tone, ZWJ family,
    /// flag, keycap.
    const EMOJI: [&str; 5] = [
        "\u{261D}\u{FE0F}",
        "\u{1F44D}\u{1F3FD}",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{1F1EE}\u{1F1F1}",
        "1\u{FE0F}\u{20E3}",
    ];
    const CAPTION: &str =
        "תיעוד התקיפה, אחרי התרעת פינוי, על בית משפחת א-סואפירי בשכונת צברה בעיר עזה";

    /// Asserts that, of the strings shaped since `SHAPED` was cleared, none starts with a
    /// character that belongs to the cluster before it or ends in a joiner, and that `emoji`
    /// was shaped whole inside one string.
    fn assert_emoji_shaped_whole(emoji: &str, composer: bool) {
        use unicode_segmentation::UnicodeSegmentation as _;
        SHAPED.with(|shaped| {
            let shaped = shaped.borrow();
            assert!(
                shaped.iter().any(|s| s.contains(emoji)),
                "{emoji:?} is never shaped whole: {shaped:?}"
            );
            // The composer's soft wrapping (GPUI's own line wrapper) measures single
            // characters; those are never drawn.
            for s in shaped.iter().filter(|s| !composer || s.chars().count() > 1) {
                let first = s.chars().next();
                assert!(
                    !matches!(
                        first,
                        Some('\u{FE0F}' | '\u{200D}' | '\u{20E3}' | '\u{1F3FB}'..='\u{1F3FF}')
                    ),
                    "{s:?} starts inside a cluster"
                );
                assert!(!s.ends_with('\u{200D}'), "{s:?} ends on a joiner");
                // No string ends half way through a flag (regional indicator pair).
                let tail = s.graphemes(true).last().unwrap_or("");
                let regional = |c: char| ('\u{1F1E6}'..='\u{1F1FF}').contains(&c);
                if tail.chars().all(regional) && !tail.is_empty() {
                    assert_eq!(tail.chars().count() % 2, 0, "{s:?} splits a flag");
                }
            }
        });
    }

    #[test]
    fn emoji_in_a_hebrew_bubble_are_shaped_whole_and_sit_at_the_left_end() {
        for emoji in EMOJI {
            for wrap in [2000., 150.] {
                with_composer(None, |cx, handle, _| {
                    cx.update_window(handle, |_, window, _| {
                        let text = format!("{CAPTION} {emoji}");
                        SHAPED.with(|s| s.borrow_mut().clear());
                        let p = paragraph(&text, wrap, window);
                        assert_emoji_shaped_whole(emoji, false);
                        if wrap > 1000. {
                            // One row: the emoji ends the sentence, so it is drawn at the
                            // left end, left of the last Hebrew word.
                            let width = px(wrap);
                            let emoji_at = text.rfind(emoji).unwrap();
                            let last_word = text.rfind("עזה").unwrap();
                            let e = p.position_for_index(emoji_at, width).unwrap().x;
                            let w = p.position_for_index(last_word, width).unwrap().x;
                            assert!(e < w, "{emoji:?} at {e:?} is not left of the word at {w:?}");
                        }
                    })
                    .unwrap();
                });
            }
        }
    }

    #[test]
    fn emoji_between_hebrew_and_latin_text_are_shaped_whole() {
        for emoji in EMOJI {
            for text in [
                format!("שלום {emoji} עולם"),
                format!("hello {emoji} שלום"),
                format!("{emoji}{emoji} שלום"),
            ] {
                with_composer(None, |cx, handle, _| {
                    cx.update_window(handle, |_, window, _| {
                        SHAPED.with(|s| s.borrow_mut().clear());
                        let _ = paragraph(&text, 2000., window);
                        assert_emoji_shaped_whole(emoji, false);
                    })
                    .unwrap();
                });
            }
        }
    }

    #[test]
    fn a_hebrew_preview_never_elides_inside_an_emoji() {
        // A line of emoji clusters, narrower than the whole: whatever the cut, no piece of
        // the shaped output starts or ends inside a cluster.
        for emoji in EMOJI {
            with_composer(None, |cx, handle, _| {
                cx.update_window(handle, |_, window, _| {
                    let text = format!("שלום עולם {emoji}{emoji}{emoji}{emoji}");
                    let style = window.text_style();
                    let size = style.font_size.to_pixels(window.rem_size());
                    let runs = vec![style.to_run(text.len())];
                    let shared: gpui_kit::SharedString = text.into();
                    for width in (40..260).step_by(7) {
                        SHAPED.with(|s| s.borrow_mut().clear());
                        let line = BidiParagraph::layout_one_line(
                            &shared,
                            &runs,
                            size,
                            px(20.),
                            px(width as f32),
                            window,
                        )
                        .expect("right-to-left text");
                        assert!(line.width() <= px(width as f32));
                        SHAPED.with(|shaped| {
                            for s in shaped.borrow().iter() {
                                assert!(
                                    !s.starts_with(['\u{FE0F}', '\u{200D}', '\u{20E3}']),
                                    "{s:?} (width {width}) starts inside a cluster"
                                );
                                assert!(!s.ends_with('\u{200D}'), "{s:?} (width {width})");
                            }
                        });
                    }
                })
                .unwrap();
            });
        }
    }

    #[test]
    fn emoji_typed_into_a_hebrew_composer_are_shaped_whole() {
        for emoji in EMOJI {
            with_composer(None, |cx, handle, input| {
                cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    SHAPED.with(|s| s.borrow_mut().clear());
                    set_text(&input, &format!("{CAPTION} {emoji}"), window, cx);
                    assert_emoji_shaped_whole(emoji, true);
                })
                .unwrap();
            });
        }
    }
}

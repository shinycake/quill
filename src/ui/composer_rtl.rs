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
mod tests {
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
    struct BidiTextSystem {
        base_rtl: Option<bool>,
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
                    let nx = caret(&input, after, cx).left();
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

    use gpui_kit::base::RunGeometry;
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
}

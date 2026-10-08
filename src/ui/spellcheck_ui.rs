//! Composer spellcheck UI (codex:spellcheck-native), after Telegram
//! Desktop's `Spellchecker::SpellingHighlighter`:
//!
//! - misspelled words get a red wavy underline painted over the kit
//!   Textarea (an overlay canvas: the Textarea has no decoration hooks,
//!   but `TextareaState::range_to_bounds` gives each word's laid-out
//!   rect);
//! - every edit shifts the existing underlines with the text, then
//!   schedules a background re-check — after a "cold" pause while a word
//!   is being typed, almost immediately after a space, punctuation or a
//!   deletion. The checker caches per-word answers, so a re-check of a
//!   long draft only asks the dictionary about new words;
//! - right-click on a misspelled word: suggestions first (click replaces
//!   the word, undoably), then "Add to Dictionary" and "Ignore"; on a word
//!   the user added, "Remove from Dictionary". Cut / Copy / Paste /
//!   Select All follow.

use std::sync::Arc;
use std::time::Duration;

use super::actions::{SpellingIgnore, SpellingLearn, SpellingReplace, SpellingUnlearn};
use super::app::QuillApp;
use super::chat_theme::danger;
use gpui_kit::component::input::{Copy, Cut, Paste, SelectAll, TextareaState};
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::*;
use quill::spellcheck::{
    LearnedIn, MAX_SUGGESTIONS, Misspelling, SpellChecker, is_typing_word, shift_misspellings,
    word_at,
};

/// Re-check delay while a word is being typed (tdesktop
/// `kColdSpellcheckingTimeout` is 1s; a little snappier here).
const COLD_DELAY: Duration = Duration::from_millis(700);
/// Re-check delay after a separator, deletion or paste: just enough to
/// coalesce a burst of edits.
const WARM_DELAY: Duration = Duration::from_millis(40);

/// The Appearance → Spelling row's hint, per platform engine.
#[cfg(target_os = "macos")]
pub(super) const SPELLCHECK_SETTING_HINT: &str = "Underline misspelled words in the composer, using the macOS spelling languages and dictionary.";
#[cfg(not(target_os = "macos"))]
pub(super) const SPELLCHECK_SETTING_HINT: &str =
    "Underline misspelled words in the composer (English).";

impl QuillApp {
    /// The platform engine: macOS NSSpellChecker when it has a spelling
    /// language, else the embedded English wordlist. `app_words` loads the
    /// words added on platforms without a system dictionary.
    pub(super) fn new_spellchecker(app_words: bool) -> Arc<SpellChecker> {
        #[cfg(target_os = "macos")]
        let checker = match super::spellcheck_mac::SystemSpellBackend::new() {
            Some(backend) => SpellChecker::new(Arc::new(backend)),
            None => SpellChecker::wordlist(),
        };
        #[cfg(not(target_os = "macos"))]
        let checker = SpellChecker::wordlist();
        if app_words {
            checker.set_app_words(
                quill::settings::load_spellcheck_words(&Self::appearance_paths()).words,
            );
        }
        Arc::new(checker)
    }

    /// Per input event: carry the underlines across the edit and schedule
    /// the debounced background re-check. Cheap — no dictionary calls.
    pub(super) fn sync_spellcheck(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.chat_prefs.spellcheck_enabled {
            self.spell_task = None;
            self.spell_checked_text.clear();
            if !self.spell_misspellings.is_empty() {
                self.spell_misspellings.clear();
                cx.notify();
            }
            return;
        }
        if text == self.spell_checked_text && !text.is_empty() {
            return;
        }
        let shifted = shift_misspellings(&self.spell_checked_text, text, &self.spell_misspellings);
        let delay = if !self.spell_checked_text.is_empty()
            && is_typing_word(&self.spell_checked_text, text)
        {
            COLD_DELAY
        } else {
            WARM_DELAY
        };
        if shifted != self.spell_misspellings {
            self.spell_misspellings = shifted;
            cx.notify();
        }
        self.spell_checked_text = text.to_string();
        self.schedule_spellcheck(delay, cx);
    }

    /// Re-check `spell_checked_text` on a background thread after
    /// `delay`; replaces (cancels) any pending check. The result is
    /// dropped if the draft changed meanwhile — that edit scheduled its own.
    fn schedule_spellcheck(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let text = self.spell_checked_text.clone();
        if text.trim().is_empty() {
            self.spell_task = None;
            return;
        }
        let checker = self.spellchecker.clone();
        self.spell_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let checked = text.clone();
            let found = cx
                .background_spawn(async move { checker.check_text(&checked) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.chat_prefs.spellcheck_enabled
                    && this.spell_checked_text == text
                    && this.spell_misspellings != found
                {
                    this.spell_misspellings = found;
                    cx.notify();
                }
            });
        }));
    }

    /// Check the current draft synchronously (screenshot fixtures, so the
    /// first captured frame already has its underlines).
    pub(super) fn spellcheck_now(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        self.spell_task = None;
        self.spell_misspellings = if self.chat_prefs.spellcheck_enabled {
            self.spellchecker.check_text(&text)
        } else {
            Vec::new()
        };
        self.spell_checked_text = text;
        cx.notify();
    }

    /// After the dictionary changed: re-check everything soon.
    fn recheck_spelling(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        self.spell_checked_text = text;
        // Words the user just accepted lose their underline right away.
        let checker = self.spellchecker.clone();
        self.spell_misspellings
            .retain(|m| !checker.is_correct(&m.word));
        cx.notify();
        self.schedule_spellcheck(WARM_DELAY, cx);
    }

    /// The composer's right-click menu (replaces the kit default, so the
    /// edit items are rebuilt here). Use as the Textarea's
    /// `context_menu` builder: see [`deferred_input_menu`] for why it
    /// returns an empty menu and shows the real one a moment later.
    pub(super) fn composer_context_menu(
        owner: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> NativeMenu {
        deferred_input_menu(window, cx, move |cx| Self::build_composer_menu(&owner, cx))
    }

    /// The menu itself; reads the composer, so only call it outside the
    /// composer's own update (see [`deferred_input_menu`]).
    fn build_composer_menu(owner: &WeakEntity<Self>, cx: &App) -> NativeMenu {
        let menu = NativeMenu::new();
        let Some(app) = owner.upgrade() else {
            return edit_menu_items(menu, false);
        };
        let app = app.read(cx);
        let input = app.composer.read(cx);
        let selection = input.selected_range();
        let has_selection = !selection.is_empty();
        let mut menu = menu;
        if app.chat_prefs.spellcheck_enabled {
            let text = input.value().to_string();
            menu = spelling_menu_items(menu, &app.spellchecker, &text, selection);
        }
        edit_menu_items(menu, has_selection)
    }

    pub(super) fn on_spelling_replace(
        &mut self,
        action: &SpellingReplace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        // The draft may have changed while the menu was open.
        if text.get(action.start..action.end) != Some(action.word.as_str()) {
            return;
        }
        let range = action.start..action.end;
        let replacement = action.replacement.clone();
        self.composer.update(cx, |input, cx| {
            // Select the word and replace the selection: an undoable edit
            // with the caret after the replacement (tdesktop restores the
            // old cursor; after the word is what macOS does).
            input.set_selected_range(range, cx);
            input.replace(replacement, window, cx);
            input.focus(window, cx);
        });
        let text = self.composer.read(cx).value().to_string();
        self.note_open_draft(true, cx);
        self.sync_spellcheck(&text, cx);
    }

    pub(super) fn on_spelling_learn(&mut self, action: &SpellingLearn, cx: &mut Context<Self>) {
        if self.spellchecker.learn(&action.word) == LearnedIn::App {
            self.save_app_words();
        }
        self.recheck_spelling(cx);
    }

    pub(super) fn on_spelling_unlearn(&mut self, action: &SpellingUnlearn, cx: &mut Context<Self>) {
        if self.spellchecker.unlearn(&action.word) == Some(LearnedIn::App) {
            self.save_app_words();
        }
        self.recheck_spelling(cx);
    }

    pub(super) fn on_spelling_ignore(&mut self, action: &SpellingIgnore, cx: &mut Context<Self>) {
        self.spellchecker.ignore(&action.word);
        self.recheck_spelling(cx);
    }

    /// Persist app-level words (`spellcheck_words.json`).
    fn save_app_words(&mut self) {
        let prefs = quill::settings::SpellcheckWords {
            words: self.spellchecker.app_words(),
        };
        if let Err(err) = quill::settings::save_spellcheck_words(&Self::appearance_paths(), &prefs)
        {
            self.status_note = format!("Couldn't save dictionary: {err}");
        }
    }

    /// The red wavy underlines, as an overlay over the composer Textarea
    /// (place it after the Textarea inside a `relative()` wrapper).
    pub(super) fn spellcheck_underlines(&self, cx: &App) -> Option<AnyElement> {
        if !self.chat_prefs.spellcheck_enabled || self.spell_misspellings.is_empty() {
            return None;
        }
        // Only paint ranges that match the text on screen.
        if self.composer.read(cx).value().as_ref() != self.spell_checked_text {
            return None;
        }
        let composer = self.composer.clone();
        let misspellings = self.spell_misspellings.clone();
        let color: Hsla = danger().into();
        Some(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    paint_underlines(&composer, &misspellings, color, bounds, window, cx);
                },
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        )
    }
}

/// A Textarea `context_menu` builder that may read the input's state.
///
/// The kit calls the builder from inside the input entity's own update
/// (`handle_right_click_menu` → `cx.defer_in`, which still holds the
/// entity), so reading the `TextareaState` there panics ("already being
/// updated"). Instead: hand the kit an empty menu (it shows nothing) and
/// build + show the real one at the click position once that update is
/// over. Right-click has already moved the caret to the clicked word.
pub(super) fn deferred_input_menu(
    window: &mut Window,
    cx: &mut App,
    build: impl FnOnce(&App) -> NativeMenu + 'static,
) -> NativeMenu {
    let position = window.mouse_position();
    window.defer(cx, move |window, cx| {
        build(cx).show(position, window, cx);
    });
    NativeMenu::new()
}

/// Suggestions, then Add to Dictionary / Ignore, for the misspelled word
/// under the caret (right-click moves the caret to the clicked spot) or
/// exactly selected; "Remove from Dictionary" for a word the user added.
fn spelling_menu_items(
    mut menu: NativeMenu,
    checker: &SpellChecker,
    text: &str,
    selection: std::ops::Range<usize>,
) -> NativeMenu {
    let Some(range) = word_at(text, selection.start) else {
        return menu;
    };
    if !selection.is_empty() && selection != range {
        return menu;
    }
    let word = text[range.clone()].to_string();
    if checker.is_correct(&word) {
        if checker.is_learned(&word) {
            menu = menu
                .menu("Remove from Dictionary", Box::new(SpellingUnlearn { word }))
                .separator();
        }
        return menu;
    }
    let suggestions = checker.suggestions(&word, MAX_SUGGESTIONS);
    if suggestions.is_empty() {
        menu = menu.menu_with_disabled(
            "No Guesses Found",
            true,
            Box::new(SpellingIgnore { word: word.clone() }),
        );
    }
    for replacement in suggestions {
        menu = menu.menu(
            replacement.clone(),
            Box::new(SpellingReplace {
                start: range.start,
                end: range.end,
                word: word.clone(),
                replacement,
            }),
        );
    }
    menu.separator()
        .menu(
            "Add to Dictionary",
            Box::new(SpellingLearn { word: word.clone() }),
        )
        .menu("Ignore", Box::new(SpellingIgnore { word }))
        .separator()
}

/// The kit's default input menu items (a custom builder replaces them).
fn edit_menu_items(menu: NativeMenu, has_selection: bool) -> NativeMenu {
    menu.menu_with_disabled("Cut", !has_selection, Box::new(Cut))
        .menu_with_disabled("Copy", !has_selection, Box::new(Copy))
        .menu("Paste", Box::new(Paste))
        .separator()
        .menu("Select All", Box::new(SelectAll))
}

/// One underline per visual line a word occupies (a word longer than the
/// composer is wide wraps mid-word).
fn word_line_rects(
    state: &TextareaState,
    text: &str,
    range: std::ops::Range<usize>,
) -> Vec<Bounds<Pixels>> {
    let (Some(start), Some(end)) = (
        state.range_to_bounds(&(range.start..range.start)),
        state.range_to_bounds(&(range.end..range.end)),
    ) else {
        return Vec::new();
    };
    if start.top() == end.top() {
        return vec![Bounds::from_corners(start.origin, end.bottom_left())];
    }
    // Wrapped: walk the characters and group them by line.
    let mut rects: Vec<Bounds<Pixels>> = Vec::new();
    let word = &text[range.clone()];
    for (ix, c) in word.char_indices() {
        let from = range.start + ix;
        let to = from + c.len_utf8();
        let (Some(a), Some(b)) = (
            state.range_to_bounds(&(from..from)),
            state.range_to_bounds(&(to..to)),
        ) else {
            continue;
        };
        if a.top() != b.top() || b.left() < a.left() {
            continue;
        }
        match rects.last_mut() {
            Some(last) if last.top() == a.top() => {
                *last = Bounds::from_corners(last.origin, b.bottom_left());
            }
            _ => rects.push(Bounds::from_corners(a.origin, b.bottom_left())),
        }
    }
    rects
}

fn paint_underlines(
    composer: &Entity<TextareaState>,
    misspellings: &[Misspelling],
    color: Hsla,
    clip: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let state = composer.read(cx);
    let text = state.value().to_string();
    // The kit composer renders at `text_sm` (Input Size::Medium).
    let font_size = window.rem_size() * 0.875;
    let font_id = window
        .text_system()
        .resolve_font(&window.text_style().font());
    let ascent = window.text_system().ascent(font_id, font_size);
    let descent = window.text_system().descent(font_id, font_size);
    let style = UnderlineStyle {
        color: Some(color),
        thickness: px(1.),
        wavy: true,
    };
    let mut lines: Vec<(Point<Pixels>, Pixels)> = Vec::new();
    for m in misspellings {
        if text.get(m.range()) != Some(m.word.as_str()) {
            continue;
        }
        for rect in word_line_rects(state, &text, m.range()) {
            let line_height = rect.size.height;
            // Same baseline offset GPUI uses for text underlines.
            let y = rect.top() + (line_height - ascent - descent) / 2. + ascent + descent * 0.618;
            lines.push((point(rect.left(), y), rect.size.width));
        }
    }
    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
        for (origin, width) in lines {
            if width > px(0.) {
                window.paint_underline(origin, width, &style);
            }
        }
    });
}

// UI integration test (needs gpui-kit `test-support`, enabled by the
// `demo-capture` feature): `cargo test --features demo-capture --bin quill
// spellcheck_ui`.
#[cfg(all(test, feature = "demo-capture"))]
mod tests {
    use super::deferred_input_menu;
    use gpui_kit::component::Root;
    use gpui_kit::component::input::{Textarea, TextareaState};
    use gpui_kit::component::native_menu::NativeMenu;
    use gpui_kit::test::{TestSupportExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, div, px, size,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    type Seen = Rc<RefCell<Option<(String, usize)>>>;

    /// A Textarea whose context-menu builder reads its own state, the way
    /// the composer's spelling menu does.
    struct Composer {
        input: Entity<TextareaState>,
        seen: Seen,
    }

    impl Render for Composer {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let input = self.input.clone();
            let seen = self.seen.clone();
            div()
                .size_full()
                .p_4()
                .child(div().id("composer").test_support().w(px(400.)).child(
                    Textarea::new(&self.input).context_menu(move |_, window, cx| {
                        let input = input.clone();
                        let seen = seen.clone();
                        deferred_input_menu(window, cx, move |cx| {
                            let state = input.read(cx);
                            *seen.borrow_mut() = Some((state.value().to_string(), state.cursor()));
                            NativeMenu::new()
                        })
                    }),
                ))
        }
    }

    /// Regression: reading the TextareaState from the right-click menu
    /// builder aborted the app ("already being updated").
    #[gpui_kit::test]
    fn right_click_menu_reads_the_input_after_its_update(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let seen: Seen = Rc::new(RefCell::new(None));
        let handle = cx.open_window(size(px(640.), px(240.)), |window, cx| {
            let input = cx.new(|cx| TextareaState::new(window, cx));
            let seen = seen.clone();
            let view = cx.new(|_| Composer { input, seen });
            Root::new(view, window, cx)
        });
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("composer", cx);
            window.input("teh speling", cx);
            assert!(seen.borrow().is_none());
            window.right_click("composer", cx);
        })
        .unwrap();
        cx.run_until_parked();
        let seen = seen.borrow().clone();
        let (text, cursor) = seen.expect("the menu builder ran");
        assert_eq!(text, "teh speling");
        assert!(cursor <= text.len());
    }
}

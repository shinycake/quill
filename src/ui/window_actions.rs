//! The actions every window that shows a chat handles: the composer's
//! formatting, spelling, emoji-word and paste items, the history and
//! selection keys, in-chat search and the media viewer's.
//!
//! The main window and each chat window bind the same list on their root
//! ([`WindowActions::window_actions`]), each with its own state put in
//! place first. A chat window draws the same composer, history and viewer
//! with the same key contexts, so the keymap and the native context menu
//! dispatch these actions there too; bound on the main root only, they did
//! nothing in a chat window. What only the main window draws (the chat
//! list, global search, the story viewer, sign-in) stays on its root.
#![deny(clippy::indexing_slicing)]

use super::actions::{
    AttachFile, ChatSearchNewer, ChatSearchOlder, ComposerEditCodeLanguage, ComposerEditDate,
    ComposerEditLink, ComposerInsertControlChar, ComposerPastePlain, DeleteSelection,
    EmojiWordBlock, EmojiWordSuggest, EmojiWordsOff, FocusComposer, FormatBlockQuote, FormatBold,
    FormatClear, FormatItalic, FormatMonospace, FormatSpoiler, FormatStrikethrough,
    FormatUnderline, HistoryPageDown, HistoryPageUp, HistoryToBottom, HistoryToTop, LoadOlder,
    OpenChatSearch, ReplyToNext, ReplyToPrevious, SelectionExtendNewer, SelectionExtendOlder,
    SelectionFocusNewer, SelectionFocusOlder, SpellingIgnore, SpellingLearn, SpellingReplace,
    SpellingUnlearn, ToggleMessageSelection, ViewerCopy, ViewerFlipHorizontal, ViewerFlipVertical,
    ViewerNext, ViewerPrev, ViewerSave, ViewerZoomIn, ViewerZoomOut, ViewerZoomReset,
};
use super::actions::{
    LockApp, MinimizeWindow, OpenHelp, OpenSettings, OpenShortcuts, QuitApp, ToggleFullscreen,
    ToggleTheme, ZoomWindow,
};
use super::app::QuillApp;
use super::shortcut_pack::HistoryKey;
use gpui_kit::component::input::{Copy as CopyAction, Paste as PasteAction};
use gpui_kit::*;
use quill::agent::ui_driver::guard::{self, HardSurface};
use quill::composer::ComposerShortcut;
use quill::ids::ChatId;
use quill::settings::ThemeChoice;

/// An action handler run with its window's state in place. `false` lets
/// the key go on to whatever else wants it.
type Handler<A> = fn(&mut QuillApp, &A, &mut Window, &mut Context<QuillApp>) -> bool;

/// Run `f` for `action` as the window `scope` (`None`: the main window).
/// `None` when the app or that window is gone.
fn run<A>(
    owner: &WeakEntity<QuillApp>,
    scope: Option<ChatId>,
    f: Handler<A>,
    action: &A,
    window: &mut Window,
    cx: &mut App,
) -> Option<bool> {
    owner
        .update(cx, |app, cx| {
            app.in_window_scope(scope, cx, |app, cx| f(app, action, window, cx))
        })
        .ok()
        .flatten()
}

/// Bind `f` to `A` in the bubble phase.
fn on<E: InteractiveElement, A: Action>(
    el: E,
    owner: &WeakEntity<QuillApp>,
    scope: Option<ChatId>,
    f: Handler<A>,
) -> E {
    let owner = owner.clone();
    el.on_action(move |action: &A, window, cx| {
        if !run(&owner, scope, f, action, window, cx).unwrap_or(false) {
            cx.propagate();
        }
    })
}

/// Bind `f` to `A` in the capture phase: `true` stops the action there,
/// before the focused element sees it.
fn capture<E: InteractiveElement, A: Action>(
    el: E,
    owner: &WeakEntity<QuillApp>,
    scope: Option<ChatId>,
    f: Handler<A>,
) -> E {
    let owner = owner.clone();
    el.capture_action(move |action: &A, window, cx| {
        if run(&owner, scope, f, action, window, cx).unwrap_or(false) {
            cx.stop_propagation();
        }
    })
}

/// A formatting shortcut; it applies only while the composer has focus.
fn format<E: InteractiveElement, A: Action>(
    el: E,
    owner: &WeakEntity<QuillApp>,
    scope: Option<ChatId>,
    shortcut: ComposerShortcut,
) -> E {
    let owner = owner.clone();
    el.on_action(move |_: &A, window, cx| {
        let _ = owner.update(cx, |app, cx| {
            app.in_window_scope(scope, cx, |app, cx| {
                app.run_composer_shortcut(shortcut, window, cx);
            })
        });
    })
}

/// Binds the window actions to a window's root element.
pub(super) trait WindowActions: InteractiveElement + Sized {
    /// Every action of a window that shows a chat, run as the window
    /// `scope` (`None`: the main window).
    fn window_actions(self, owner: WeakEntity<QuillApp>, scope: Option<ChatId>) -> Self {
        let el = self;
        let o = &owner;
        // With message text selected, ⌘C copies it even while the composer
        // has focus; in selection mode it copies the selected messages.
        let el = capture::<_, CopyAction>(el, o, scope, |app, _, window, cx| {
            if let Some(((chat_id, _), text)) =
                super::selectable_text::selected_message_text(window, cx)
            {
                if !app.refuse_protected_copy(ChatId(chat_id), cx) {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                return true;
            }
            let Some((chat_id, text)) = app.selected_messages_text() else {
                return false;
            };
            if !app.refuse_protected_copy(chat_id, cx) {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            true
        });
        // The kit Textarea's paste is text-only, and GPUI stops an action
        // at the first handler, so this one captures (runs before the
        // focused textarea): clipboard images and copied files become
        // attachments; anything else falls through to the text paste.
        let el = capture::<_, PasteAction>(el, o, scope, |app, _, window, cx| {
            if app.paste_image_from_clipboard(window, cx) {
                return true;
            }
            app.offer_rich_paste(window, cx);
            false
        });
        let el = on::<_, FocusComposer>(el, o, scope, |app, _, window, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
            true
        });
        // Shortcut pack (`shortcut_pack.rs`): a handler that has no use for
        // the key propagates it to the focused input.
        let el = on::<_, ReplyToPrevious>(el, o, scope, |app, _, window, cx| {
            app.reply_by_key(false, window, cx)
        });
        let el = on::<_, ReplyToNext>(el, o, scope, |app, _, window, cx| {
            app.reply_by_key(true, window, cx)
        });
        let el = on::<_, AttachFile>(el, o, scope, |app, _, window, cx| {
            app.attach_by_key(window, cx)
        });
        let el = on::<_, HistoryPageUp>(el, o, scope, |app, _, window, cx| {
            app.scroll_history_by_key(HistoryKey::PageUp, window, cx)
        });
        let el = on::<_, HistoryPageDown>(el, o, scope, |app, _, window, cx| {
            app.scroll_history_by_key(HistoryKey::PageDown, window, cx)
        });
        let el = on::<_, HistoryToTop>(el, o, scope, |app, _, window, cx| {
            app.scroll_history_by_key(HistoryKey::Top, window, cx)
        });
        let el = on::<_, HistoryToBottom>(el, o, scope, |app, _, window, cx| {
            app.scroll_history_by_key(HistoryKey::Bottom, window, cx)
        });
        let el = on::<_, DeleteSelection>(el, o, scope, |app, _, window, cx| {
            app.delete_selection_by_key(window, cx)
        });
        let el = on::<_, ToggleMessageSelection>(el, o, scope, |app, _, window, cx| {
            app.toggle_focused_selection_by_key(window, cx)
        });
        let el = on::<_, SelectionFocusOlder>(el, o, scope, |app, _, window, cx| {
            app.move_selection_focus_by_key(true, false, window, cx)
        });
        let el = on::<_, SelectionFocusNewer>(el, o, scope, |app, _, window, cx| {
            app.move_selection_focus_by_key(false, false, window, cx)
        });
        let el = on::<_, SelectionExtendOlder>(el, o, scope, |app, _, window, cx| {
            app.move_selection_focus_by_key(true, true, window, cx)
        });
        let el = on::<_, SelectionExtendNewer>(el, o, scope, |app, _, window, cx| {
            app.move_selection_focus_by_key(false, true, window, cx)
        });
        let el = on::<_, LoadOlder>(el, o, scope, |app, _, _, cx| {
            app.load_older_action(cx);
            true
        });
        let el = on::<_, OpenChatSearch>(el, o, scope, |app, _, window, cx| {
            app.open_chat_search_ui(window, cx);
            true
        });
        let el = on::<_, ChatSearchNewer>(el, o, scope, |app, _, _, cx| {
            app.chat_search_newer(cx);
            true
        });
        let el = on::<_, ChatSearchOlder>(el, o, scope, |app, _, _, cx| {
            app.chat_search_older(cx);
            true
        });
        let el = viewer_actions(el, o, scope);
        composer_actions(el, o, scope)
    }

    /// A chat window's half of what the main root binds for the app and
    /// for its own window: Lock, Quit, the theme toggle, Help, Settings and
    /// its Shortcuts page (which open in the main window, brought
    /// forward), and Minimize, Zoom and Full Screen of this window. Bound
    /// on the main root only, they did nothing while a chat window had
    /// the focus.
    fn chat_window_app_actions(self, owner: WeakEntity<QuillApp>, scope: Option<ChatId>) -> Self {
        let o = &owner;
        // The lock is the app's: it runs as the main window, and reaches
        // every window from there.
        let el = on::<_, LockApp>(self, o, None, |app, _, _, cx| {
            app.lock_by_passcode(cx);
            true
        });
        let el = on::<_, ToggleTheme>(el, o, None, |app, _, _, cx| {
            app.cycle_theme_by_key(cx);
            true
        });
        let el = on::<_, OpenHelp>(el, o, None, |app, _, _, cx| {
            app.open_link("https://github.com/shinycake/quill", cx);
            true
        });
        let el = on::<_, QuitApp>(el, o, scope, |app, _, _, cx| {
            if guard::refuse_hard_if_injecting(HardSurface::Window) {
                return true;
            }
            app.hand_to_main_window(cx, |app, window, cx| {
                app.exit_after_exports(super::export_quit::quit_app, window, cx);
            });
            true
        });
        let el = on::<_, OpenSettings>(el, o, scope, |app, _, _, cx| {
            app.hand_to_main_window(cx, |app, window, cx| {
                app.navigate(super::navigation::NavigationAction::Settings, window, cx);
            });
            true
        });
        let el = on::<_, OpenShortcuts>(el, o, scope, |app, _, _, cx| {
            app.hand_to_main_window(cx, |app, _, cx| {
                app.open_settings(Some(quill::settings_catalog::Page::Shortcuts), cx);
                cx.notify();
            });
            true
        });
        let el = on::<_, MinimizeWindow>(el, o, scope, |_, _, window, _| {
            if !guard::refuse_hard_if_injecting(HardSurface::Window) {
                window.minimize_window();
            }
            true
        });
        let el = on::<_, ZoomWindow>(el, o, scope, |_, _, window, _| {
            if !guard::refuse_hard_if_injecting(HardSurface::Window) {
                window.zoom_window();
            }
            true
        });
        on::<_, ToggleFullscreen>(el, o, scope, |_, _, window, _| {
            if !guard::refuse_hard_if_injecting(HardSurface::Window) {
                window.toggle_fullscreen();
            }
            true
        })
    }
}

impl<E: InteractiveElement + Sized> WindowActions for E {}

/// Left and Right step the media viewer (the story viewer first, which
/// only the main window draws); `0` resets the zoom. Only while a viewer
/// is open: otherwise the key reaches the focused input (an action handler
/// stops propagation unless told to propagate, which used to eat the
/// composer's arrow keys and "0").
fn viewer_actions<E: InteractiveElement>(
    el: E,
    o: &WeakEntity<QuillApp>,
    scope: Option<ChatId>,
) -> E {
    let el = on::<_, ViewerPrev>(el, o, scope, |app, _, window, cx| {
        app.step_viewer_by_key(-1, window, cx)
    });
    let el = on::<_, ViewerNext>(el, o, scope, |app, _, window, cx| {
        app.step_viewer_by_key(1, window, cx)
    });
    let el = on::<_, ViewerZoomReset>(el, o, scope, |app, _, _, cx| {
        app.viewer.state.is_open() && {
            app.viewer_toggle_natural_zoom(cx);
            true
        }
    });
    let el = on::<_, ViewerZoomIn>(el, o, scope, |app, _, _, cx| {
        app.viewer.state.is_open() && {
            app.viewer_zoom_step(true, cx);
            true
        }
    });
    let el = on::<_, ViewerZoomOut>(el, o, scope, |app, _, _, cx| {
        app.viewer.state.is_open() && {
            app.viewer_zoom_step(false, cx);
            true
        }
    });
    let el = on::<_, ViewerFlipHorizontal>(el, o, scope, |app, _, _, cx| {
        app.viewer.state.is_open() && {
            app.flip_viewer_horizontal(cx);
            true
        }
    });
    let el = on::<_, ViewerFlipVertical>(el, o, scope, |app, _, _, cx| {
        app.viewer.state.is_open() && {
            app.flip_viewer_vertical(cx);
            true
        }
    });
    let el = on::<_, ViewerCopy>(el, o, scope, |app, _, _, cx| {
        app.viewer.state.is_open() && {
            app.copy_viewer_photo(cx);
            true
        }
    });
    on::<_, ViewerSave>(el, o, scope, |app, _, _, cx| {
        app.viewer.state.is_open() && {
            app.save_viewer_media(cx);
            true
        }
    })
}

/// The composer's spelling, emoji-word, formatting and paste items. The
/// formatting shortcuts apply only while the composer has focus
/// (otherwise the key belongs to whatever is focused).
fn composer_actions<E: InteractiveElement>(
    el: E,
    o: &WeakEntity<QuillApp>,
    scope: Option<ChatId>,
) -> E {
    // codex:spellcheck-native: composer context-menu spelling items.
    let el = on::<_, SpellingReplace>(el, o, scope, |app, action, window, cx| {
        app.on_spelling_replace(action, window, cx);
        true
    });
    let el = on::<_, SpellingLearn>(el, o, scope, |app, action, _, cx| {
        app.on_spelling_learn(action, cx);
        true
    });
    let el = on::<_, SpellingUnlearn>(el, o, scope, |app, action, _, cx| {
        app.on_spelling_unlearn(action, cx);
        true
    });
    let el = on::<_, SpellingIgnore>(el, o, scope, |app, action, _, cx| {
        app.on_spelling_ignore(action, cx);
        true
    });
    let el = on::<_, EmojiWordSuggest>(el, o, scope, |app, action, _, cx| {
        app.on_emoji_word_suggest(action, cx);
        true
    });
    let el = on::<_, EmojiWordBlock>(el, o, scope, |app, action, _, cx| {
        app.on_emoji_word_block(action, cx);
        true
    });
    let el = on::<_, EmojiWordsOff>(el, o, scope, |app, _, _, cx| {
        app.on_emoji_words_off(cx);
        true
    });
    let el = format::<_, FormatBold>(el, o, scope, ComposerShortcut::Bold);
    let el = format::<_, FormatItalic>(el, o, scope, ComposerShortcut::Italic);
    let el = format::<_, FormatUnderline>(el, o, scope, ComposerShortcut::Underline);
    let el = format::<_, FormatStrikethrough>(el, o, scope, ComposerShortcut::Strikethrough);
    let el = format::<_, FormatMonospace>(el, o, scope, ComposerShortcut::Monospace);
    let el = format::<_, FormatBlockQuote>(el, o, scope, ComposerShortcut::BlockQuote);
    let el = format::<_, FormatSpoiler>(el, o, scope, ComposerShortcut::Spoiler);
    let el = format::<_, FormatClear>(el, o, scope, ComposerShortcut::ClearFormatting);
    let el = on::<_, ComposerEditLink>(el, o, scope, |app, _, window, cx| {
        app.composer_link_chord(window, cx);
        true
    });
    let el = on::<_, ComposerEditDate>(el, o, scope, |app, _, window, cx| {
        app.composer_date_chord(window, cx);
        true
    });
    let el = on::<_, ComposerInsertControlChar>(el, o, scope, |app, action, window, cx| {
        app.insert_composer_control_char(action.character, window, cx);
        true
    });
    let el = on::<_, ComposerEditCodeLanguage>(el, o, scope, |app, _, window, cx| {
        app.open_code_language_dialog(window, cx);
        true
    });
    on::<_, ComposerPastePlain>(el, o, scope, |app, _, window, cx| {
        app.paste_plain_text(window, cx);
        true
    })
}

impl QuillApp {
    /// The theme toggle, from any window. A settings change, written to
    /// disk, through the appearance funnel (persist and re-apply), so the
    /// auto-night tick can't silently revert the flip; auto-night, when
    /// on, still overrides the manual choice while active, as in the
    /// dialog. It cycles all three modes (Light, Dark, High contrast).
    pub(super) fn cycle_theme_by_key(&mut self, cx: &mut Context<Self>) {
        if guard::refuse_if_injecting("change the theme") {
            return;
        }
        let next = match self.appearance.theme {
            ThemeChoice::Light => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::HighContrast,
            ThemeChoice::HighContrast => ThemeChoice::Light,
        };
        self.set_appearance(cx, |a| a.theme = next);
    }

    /// Left or Right with a viewer open: the story viewer (the main
    /// window's), else the media viewer of the window in place.
    fn step_viewer_by_key(
        &mut self,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.windows.in_window()
            && self.stories.viewer.is_open()
            && !self.story_text_input_open(window, cx)
        {
            self.step_story_viewer(delta, cx);
            true
        } else if self.viewer.state.is_open() {
            self.step_media_viewer(delta, cx);
            true
        } else {
            false
        }
    }
}

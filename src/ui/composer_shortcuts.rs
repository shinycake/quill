//! Composer keyboard shortcuts, after Telegram Desktop's `InputField`
//! (lib_ui/ui/widgets/fields/input_field.cpp): the formatting chords, the
//! Cmd/Ctrl+K link dialog that shares its chord with quick switch, the
//! "Formatting" entries of the right-click menu, and Cmd/Ctrl+Shift+V
//! plain-text paste.

use super::actions::{
    ComposerEditCodeLanguage, ComposerEditLink, ComposerPastePlain, FormatBlockQuote, FormatBold,
    FormatClear, FormatItalic, FormatMonospace, FormatSpoiler, FormatStrikethrough,
    FormatUnderline,
};
use super::app::QuillApp;
use super::chat_theme::{accent, bg_canvas};
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer::{
    ComposerShortcut, FormatAction, LinkChord, apply_format_markup, link_chord_target,
    normalize_link_url,
};
use std::ops::Range;

/// The link dialog above the composer (tdesktop's "Edit link" box): one URL
/// field for the text that was selected when the chord was pressed.
pub(super) struct ComposerLinkDialog {
    /// The selected range of the draft when the dialog opened.
    range: Range<usize>,
    /// The text in that range, to detect a draft that changed meanwhile.
    selected: String,
    input: Entity<TextareaState>,
}

/// The "Code Language" box (tdesktop `EditCodeLanguageBox`): one field,
/// empty for auto-detect, for the fenced block the caret was in.
pub(super) struct CodeLanguageDialog {
    /// Where the block was when the box opened, to notice a changed draft.
    block: Range<usize>,
    input: Entity<TextareaState>,
    error: Option<&'static str>,
}

impl CodeLanguageDialog {
    /// The box's field, for demo fixtures.
    pub(super) fn input_for_demo(&self) -> &Entity<TextareaState> {
        &self.input
    }
}

/// The chord for a shortcut as shown in menus: "⌘B" on macOS, "Ctrl+B"
/// elsewhere (tdesktop appends `QKeySequence::NativeText` after a tab).
pub(super) fn shortcut_hint(shortcut: ComposerShortcut) -> String {
    let Some(spec) = quill::composer::COMPOSER_SHORTCUTS
        .iter()
        .find(|spec| spec.shortcut == shortcut)
    else {
        return String::new();
    };
    let key = spec.key.to_uppercase();
    if cfg!(target_os = "macos") {
        format!("{}⌘{key}", if spec.shift { "⇧" } else { "" })
    } else {
        format!("Ctrl+{}{key}", if spec.shift { "Shift+" } else { "" })
    }
}

/// Menu label with its shortcut, as tdesktop writes it: `Label<TAB>chord`.
fn labelled(label: &str, shortcut: ComposerShortcut) -> String {
    format!("{label}\t{}", shortcut_hint(shortcut))
}

impl QuillApp {
    /// Whether the composer Textarea has keyboard focus.
    fn composer_focused(&self, window: &Window, cx: &App) -> bool {
        self.composer.read(cx).focus_handle(cx).is_focused(window)
    }

    /// A formatting chord was pressed (ignored unless the composer has
    /// focus, so the same chords stay free for other inputs).
    pub(super) fn run_composer_shortcut(
        &mut self,
        shortcut: ComposerShortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.composer_focused(window, cx) {
            return;
        }
        if shortcut == ComposerShortcut::ClearFormatting {
            self.clear_composer_format(window, cx);
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let selected = text.get(range).unwrap_or_default();
        if let Some(action) = shortcut.format_action(selected) {
            self.apply_composer_format(action, window, cx);
        }
    }

    /// Cmd/Ctrl+K inside the composer: with a selection open the link
    /// dialog; otherwise the chord keeps its app-wide meaning, quick switch.
    pub(super) fn composer_link_chord(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_selection = !self.composer.read(cx).selected_range().is_empty();
        match link_chord_target(self.composer_focused(window, cx), has_selection) {
            LinkChord::EditLink => self.open_composer_link_dialog(window, cx),
            LinkChord::QuickSwitch => self.open_search_ui(window, cx),
        }
    }

    /// Open the link dialog for the current selection.
    pub(super) fn open_composer_link_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let Some(selected) = text.get(range.clone()).filter(|s| !s.is_empty()) else {
            return;
        };
        let selected = selected.to_string();
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Enter a link")
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.apply_composer_link(window, cx);
            }
        })
        .detach();
        input.update(cx, |input, cx| input.focus(window, cx));
        self.composer_ui.link_dialog = Some(ComposerLinkDialog {
            range,
            selected,
            input,
        });
        cx.notify();
    }

    /// Close the dialog and return focus to the composer.
    pub(super) fn close_composer_link_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composer_ui.link_dialog.take().is_some() {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    /// Wrap the remembered selection in `[text](url)`.
    fn apply_composer_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.composer_ui.link_dialog.as_ref() else {
            return;
        };
        let typed = dialog.input.read(cx).value().to_string();
        let range = dialog.range.clone();
        let selected = dialog.selected.clone();
        let Some(url) = normalize_link_url(&typed) else {
            // Nothing (or something unusable) typed: leave the dialog open.
            self.status_note = "enter a valid link".into();
            cx.notify();
            return;
        };
        self.close_composer_link_dialog(window, cx);
        let text = self.composer.read(cx).value().to_string();
        if text.get(range.clone()) != Some(selected.as_str()) {
            // The draft changed under the dialog; don't link the wrong text.
            self.status_note = "the text changed; select it again to add the link".into();
            cx.notify();
            return;
        }
        if !self.composer_ui.rich_editor_open {
            // The selected text becomes a link, shown in the link colour.
            self.composer.update(cx, |input, cx| {
                input.set_selected_range(range.clone(), cx);
            });
            self.toggle_composer_tag(quill::composer_doc::ComposerTag::Link(url), window, cx);
            self.composer.update(cx, |input, cx| {
                input.set_selected_range(range.end..range.end, cx);
                input.focus(window, cx);
            });
            cx.notify();
            return;
        }
        let (new_text, new_selection) = apply_format_markup(&text, range, &FormatAction::Link(url));
        self.composer.update(cx, |input, cx| {
            input.set_value(&new_text, window, cx);
            input.set_selected_range(new_selection, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// The dialog panel, above the composer input row.
    pub(super) fn composer_link_dialog_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.composer_ui.link_dialog.as_ref()?;
        Some(
            div()
                .id("composer-link-dialog")
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(accent())
                .bg(bg_canvas())
                .child(div().text_sm().font_semibold().child("Add link"))
                .child(
                    div()
                        .text_xs()
                        .opacity(0.7)
                        .child(format!("Link for “{}”", dialog.selected)),
                )
                .child(Textarea::new(&dialog.input).aria_label("Link address"))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Button::new("composer-link-save").label("Save").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.apply_composer_link(window, cx);
                            }),
                        ))
                        .child(
                            Button::new("composer-link-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_composer_link_dialog(window, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Open the "Code Language" box for the block under the caret.
    pub(super) fn open_code_language_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        let caret = self.composer.read(cx).selected_range().start;
        // A code block is formatting in the field (codex:composer-input);
        // the rich editor still writes fences.
        let (block, current) = if self.composer_ui.rich_editor_open {
            let Some(fence) = quill::code_language::fence_at(&text, caret) else {
                self.status_note = "Put the cursor inside a code block first.".into();
                cx.notify();
                return;
            };
            (fence.block.clone(), fence.current(&text).to_string())
        } else {
            let Some(found) = self.composer_code_block_at_caret(cx) else {
                self.status_note = "Put the cursor inside a code block first.".into();
                cx.notify();
                return;
            };
            found
        };
        let input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("Auto-Detect")
                .auto_grow(1, 1)
                .submit_on_enter(true);
            state.set_value(&current, window, cx);
            state
        });
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.apply_code_language(window, cx);
            }
        })
        .detach();
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.composer_ui.code_language = Some(CodeLanguageDialog {
            block,
            input,
            error: None,
        });
        cx.notify();
    }

    pub(super) fn close_code_language_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.composer_ui.code_language.take().is_some() {
            self.composer
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        }
    }

    /// Save: rewrite the language after the block's opening fence.
    fn apply_code_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.composer_ui.code_language.as_ref() else {
            return;
        };
        let typed = dialog.input.read(cx).value().to_string();
        let block = dialog.block.clone();
        if !self.composer_ui.rich_editor_open {
            let still_there = self
                .composer_code_block_at_caret(cx)
                .is_some_and(|(range, _)| range == block);
            if !still_there {
                self.close_code_language_dialog(window, cx);
                self.status_note = "The text changed. Open Code Language again.".into();
                cx.notify();
                return;
            }
            match quill::code_language::validate_language(&typed) {
                Ok(language) => {
                    self.close_code_language_dialog(window, cx);
                    self.set_composer_code_language(block, language, window, cx);
                }
                Err(error) => {
                    if let Some(dialog) = self.composer_ui.code_language.as_mut() {
                        dialog.error = Some(error.note());
                    }
                }
            }
            cx.notify();
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let fence =
            quill::code_language::fence_at(&text, block.start).filter(|fence| fence.block == block);
        let Some(fence) = fence else {
            // The draft changed under the box; do not edit the wrong block.
            self.close_code_language_dialog(window, cx);
            self.status_note = "The text changed. Open Code Language again.".into();
            cx.notify();
            return;
        };
        match quill::code_language::with_language(&text, &fence, &typed) {
            Ok((new_text, new_block)) => {
                self.close_code_language_dialog(window, cx);
                self.composer.update(cx, |input, cx| {
                    input.set_value(&new_text, window, cx);
                    input.set_selected_range(new_block.end..new_block.end, cx);
                    input.focus(window, cx);
                });
            }
            Err(error) => {
                if let Some(dialog) = self.composer_ui.code_language.as_mut() {
                    dialog.error = Some(error.note());
                }
            }
        }
        cx.notify();
    }

    /// The box, above the composer input row.
    pub(super) fn code_language_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.composer_ui.code_language.as_ref()?;
        let typed = dialog.input.read(cx).value().chars().count();
        let over = typed > quill::code_language::CODE_LANGUAGE_LIMIT;
        Some(
            div()
                .id("composer-code-language")
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(accent())
                .bg(bg_canvas())
                .child(div().text_sm().font_semibold().child("Code Language"))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Language for syntax highlighting."),
                )
                .child(Textarea::new(&dialog.input).aria_label("Code language"))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_xs()
                        .child(
                            div()
                                .text_color(cx.theme().danger)
                                .child(dialog.error.unwrap_or("")),
                        )
                        .child(
                            div()
                                .text_color(if over {
                                    cx.theme().danger
                                } else {
                                    cx.theme().muted_foreground
                                })
                                .child(format!(
                                    "{typed}/{}",
                                    quill::code_language::CODE_LANGUAGE_LIMIT
                                )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("composer-code-language-save")
                                .label("Save")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_code_language(window, cx);
                                })),
                        )
                        .child(
                            Button::new("composer-code-language-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_code_language_dialog(window, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Cmd/Ctrl+Shift+V: insert the clipboard's text only. Unlike the plain
    /// paste, images and copied files never become attachments, and
    /// Windows line endings are normalized.
    pub(super) fn paste_plain_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.composer_focused(window, cx) {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return;
        }
        self.composer
            .update(cx, |input, cx| input.replace(text, window, cx));
        cx.notify();
    }

    /// The "Formatting" group of the composer's right-click menu. Items need
    /// a selection (tdesktop disables them without one); each dispatches the
    /// same action as its chord, so the menu and the shortcut cannot drift.
    pub(super) fn formatting_menu_items(
        menu: NativeMenu,
        has_selection: bool,
        in_code_block: bool,
    ) -> NativeMenu {
        let item = |label: &str, shortcut: ComposerShortcut, action: Box<dyn Action>| {
            (labelled(label, shortcut), action)
        };
        let entries = [
            item("Bold", ComposerShortcut::Bold, Box::new(FormatBold)),
            item("Italic", ComposerShortcut::Italic, Box::new(FormatItalic)),
            item(
                "Underline",
                ComposerShortcut::Underline,
                Box::new(FormatUnderline),
            ),
            item(
                "Strikethrough",
                ComposerShortcut::Strikethrough,
                Box::new(FormatStrikethrough),
            ),
            item(
                "Quote",
                ComposerShortcut::BlockQuote,
                Box::new(FormatBlockQuote),
            ),
            item(
                "Monospace",
                ComposerShortcut::Monospace,
                Box::new(FormatMonospace),
            ),
            item(
                "Spoiler",
                ComposerShortcut::Spoiler,
                Box::new(FormatSpoiler),
            ),
        ];
        let mut submenu = NativeMenu::new();
        for (label, action) in entries {
            submenu = submenu.menu_with_disabled(label, !has_selection, action);
        }
        submenu = submenu
            .separator()
            .menu_with_disabled(
                labelled("Create link", ComposerShortcut::EditLink),
                !has_selection,
                Box::new(ComposerEditLink),
            )
            .menu_with_disabled(
                "Code Language…",
                !in_code_block,
                Box::new(ComposerEditCodeLanguage),
            )
            .menu(
                labelled("Clear formatting", ComposerShortcut::ClearFormatting),
                Box::new(FormatClear),
            );
        menu.submenu("Formatting", submenu)
    }

    /// Paste as plain text, listed next to Paste (tdesktop has
    /// "Paste without formatting" there).
    pub(super) fn paste_plain_menu_item(menu: NativeMenu) -> NativeMenu {
        let chord = if cfg!(target_os = "macos") {
            "⇧⌘V"
        } else {
            "Ctrl+Shift+V"
        };
        menu.menu(
            format!("Paste as Plain Text\t{chord}"),
            Box::new(ComposerPastePlain),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::shortcut_hint;
    use quill::composer::ComposerShortcut;

    #[test]
    fn hints_use_the_platform_modifier() {
        let bold = shortcut_hint(ComposerShortcut::Bold);
        let strike = shortcut_hint(ComposerShortcut::Strikethrough);
        if cfg!(target_os = "macos") {
            assert_eq!(bold, "⌘B");
            assert_eq!(strike, "⇧⌘X");
        } else {
            assert_eq!(bold, "Ctrl+B");
            assert_eq!(strike, "Ctrl+Shift+X");
        }
        assert!(shortcut_hint(ComposerShortcut::BlockQuote).ends_with('.'));
    }
}

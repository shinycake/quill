//! Keyboard shortcuts reference dialog
//! (parity:platform-shortcuts-reference). Read-only: renders every row of
//! `keybindings::shortcut_rows()` — the same table `bind_keys` installs —
//! grouped by section, so the reference can never drift from the real
//! bindings. Opened from Help → Keyboard Shortcuts.

use super::QuillApp;
use super::keybindings::shortcut_rows;
use super::{DialogKind, QuillShell};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::*;
use gpui_kit::*;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// kit Phase 2 (redo) pattern: the shortcuts reference hosted in a kit
    /// `Dialog` via `window.open_dialog` (see `QuillShell::sync_kit_dialogs`).
    /// Esc / backdrop / ✕ clear state via `on_close`; there is nothing to
    /// apply, so the footer is a single Close button.
    pub(crate) fn build_shortcuts_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Shortcuts, |this, _, cx| {
                this.shortcuts_open = false;
                cx.notify();
            });
        app.update(cx, |_this, cx| {
            // Group rows by (section, label); rows sharing a label
            // (`cmd-q` / `ctrl-q`) render as one entry with both chips.
            // `shortcut_rows()` is written in section order, so first-seen
            // order is the display order.
            let bindings = cx.key_bindings();
            let bindings = bindings.borrow();
            let mut groups: Vec<(&'static str, &'static str, Vec<Keystroke>)> = Vec::new();
            for row in shortcut_rows() {
                if groups
                    .iter()
                    .any(|(section, label, _)| *section == row.section && *label == row.label)
                {
                    continue;
                }
                let keys = bindings
                    .bindings()
                    .filter(|binding| binding.action().name() == row.binding.action().name())
                    .flat_map(|binding| binding.keystrokes().iter().map(|key| key.inner().clone()))
                    .collect();
                groups.push((row.section, row.label, keys));
            }
            drop(bindings);
            let mut body = div()
                .id("shortcuts-body")
                .max_h(px(480.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_4();
            let mut current_section = "";
            for (section, label, keys) in &groups {
                if *section != current_section {
                    current_section = section;
                    body = body.child(div().font_semibold().text_sm().child(section.to_string()));
                }
                let mut chips = div().flex().gap_1();
                for keystroke in keys {
                    chips = chips.child(Kbd::new(keystroke.clone()));
                }
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_4()
                        .child(div().text_sm().child(label.to_string()))
                        .child(chips),
                );
            }
            let footer = div().flex().justify_end().child(
                Button::new("close-shortcuts")
                    .label("Close")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.shortcuts_open = false;
                        cx.notify();
                        this.close_kit_dialog_if_done(DialogKind::Shortcuts, window, cx);
                    })),
            );
            dialog
                .overlay(true)
                .title("Keyboard Shortcuts")
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

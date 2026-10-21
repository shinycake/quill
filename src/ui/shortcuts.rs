//! Keyboard shortcuts reference on the Keyboard Shortcuts settings page
//! (parity:platform-shortcuts-reference). Read-only: renders every row of
//! `keybindings::shortcut_rows()` — the same table `bind_keys` installs —
//! grouped by section, so the reference can never drift from the real
//! bindings. Help → Keyboard Shortcuts opens that page.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::keybindings::shortcut_rows;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::*;
use gpui_kit::*;

impl QuillApp {
    /// Every shortcut, grouped by section, with its keys.
    pub(crate) fn shortcuts_reference(&self, cx: &mut Context<Self>) -> AnyElement {
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
            // `partial_eq`, not the name: the eight pinned-chat chords
            // share one action type and differ by index. Composer
            // scoped twins of a chord list it once.
            let mut keys: Vec<Keystroke> = Vec::new();
            for binding in bindings
                .bindings()
                .filter(|binding| binding.action().partial_eq(row.binding.action()))
            {
                for key in binding.keystrokes() {
                    if !keys.contains(key.inner()) {
                        keys.push(key.inner().clone());
                    }
                }
            }
            groups.push((row.section, row.label, keys));
        }
        drop(bindings);
        let mut body = div().id("shortcuts-body").flex().flex_col().gap_3();
        let mut current_section = "";
        for (section, label, keys) in &groups {
            if *section != current_section {
                current_section = section;
                body = body.child(
                    div()
                        .pt_3()
                        .font_semibold()
                        .text_sm()
                        .child(section.to_string()),
                );
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
        body.into_any_element()
    }
}

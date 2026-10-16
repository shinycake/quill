//! The sidebar header row: main menu, optional lock and proxy buttons, and
//! the Chats / Contacts / Calls tabs, fitted to any sidebar width.
//!
//! The side buttons never shrink. The tabs take the rest of the row and drop
//! to icon-only (with tooltips) when the labelled strip would not fit. The
//! decision is a pure function of the sidebar width and the visible buttons,
//! so it is recomputed on every render.

use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;

/// Sidebar padding (`p_3`) on both sides.
const SIDEBAR_PADDING: f32 = 24.;
/// Width of one ghost icon button in the header.
const SIDE_BUTTON_WIDTH: f32 = 32.;
/// Gap between header children (`gap_1`).
const HEADER_GAP: f32 = 4.;
/// Width the labelled tab strip needs.
const LABELLED_TABS_WIDTH: f32 = 228.;

/// Whether the tab strip must show icons instead of labels.
pub(super) fn tabs_icon_only(sidebar_width: f32, side_buttons: usize) -> bool {
    let side = side_buttons as f32 * (SIDE_BUTTON_WIDTH + HEADER_GAP);
    sidebar_width - SIDEBAR_PADDING - side < LABELLED_TABS_WIDTH
}

impl QuillApp {
    /// The header row above the chat list.
    pub(super) fn sidebar_header(&self, mode: PaneMode, cx: &mut Context<Self>) -> AnyElement {
        let ready = mode == PaneMode::Ready;
        let lock = ready && self.account.passcode.enabled;
        let proxy = self.proxy_shield_button(cx);
        let side_buttons = usize::from(ready) + usize::from(lock) + usize::from(proxy.is_some());
        let icon_only = tabs_icon_only(f32::from(self.frame.sidebar_width), side_buttons);
        div()
            .flex()
            .items_center()
            .gap_1()
            .w_full()
            .min_w_0()
            .when(ready, |this| {
                this.child(div().flex_none().child(self.main_navigation_menu(cx)))
            })
            .when(lock, |this| {
                this.child(div().flex_none().child(self.lock_button(cx)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .truncate()
                    .child(self.list_tabs(icon_only, cx)),
            )
            .children(proxy.map(|button| div().flex_none().child(button)))
            .into_any_element()
    }

    /// The Chats / Contacts / Calls strip as a kit segmented `TabBar`. Other
    /// modes keep the plain "Chats" title.
    fn list_tabs(&self, icon_only: bool, cx: &mut Context<Self>) -> AnyElement {
        if self.pane_mode() != PaneMode::Ready {
            return div()
                .font_semibold()
                .child(tr(keys::RECENT_CHATS))
                .into_any_element();
        }
        let selected = if self.chat_list.contacts_tab_open {
            1
        } else if self.chat_list.calls_tab_open {
            2
        } else {
            0
        };
        let weak = cx.weak_entity();
        let tab = |icon: IconName, label: &'static str| {
            if icon_only {
                Tab::new()
                    .icon(icon)
                    .aria_label(label)
                    .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            } else {
                Tab::new().label(label)
            }
        };
        TabBar::new("list-tabs")
            .segmented()
            .selected_index(selected)
            .child(tab(IconName::MessageCircle, tr(keys::RECENT_CHATS)))
            .child(tab(IconName::Users, tr(keys::MENU_CONTACTS)))
            // Recent-calls tab (server-side `searchCallMessages` history +
            // call settings).
            .child(tab(IconName::Phone, tr(keys::MENU_CALLS)))
            .on_click(move |ix, _window, cx| {
                let _ = weak.update(cx, |this, cx| match ix {
                    0 => this.open_chats_tab(cx),
                    1 => this.open_contacts_tab(cx),
                    _ => this.open_calls_tab(cx),
                });
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::tabs_icon_only;
    use quill::settings::{MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH};

    #[test]
    fn minimum_width_with_every_side_button_uses_icons() {
        assert!(tabs_icon_only(MIN_SIDEBAR_WIDTH, 3));
        assert!(tabs_icon_only(MIN_SIDEBAR_WIDTH, 1));
    }

    #[test]
    fn wide_sidebar_keeps_labels_even_with_every_side_button() {
        assert!(!tabs_icon_only(MAX_SIDEBAR_WIDTH, 3));
    }

    #[test]
    fn more_side_buttons_never_make_labels_fit_again() {
        let mut width = MIN_SIDEBAR_WIDTH;
        while width <= MAX_SIDEBAR_WIDTH {
            for buttons in 0..3 {
                if tabs_icon_only(width, buttons) {
                    assert!(tabs_icon_only(width, buttons + 1), "{width} {buttons}");
                }
            }
            width += 1.;
        }
    }

    #[test]
    fn widening_never_hides_labels_again() {
        let mut was_labelled = false;
        let mut width = MIN_SIDEBAR_WIDTH;
        while width <= MAX_SIDEBAR_WIDTH {
            let labelled = !tabs_icon_only(width, 3);
            assert!(labelled || !was_labelled, "{width}");
            was_labelled = labelled;
            width += 1.;
        }
    }
}

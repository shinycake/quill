//! The sidebar header row: main menu, optional lock and proxy buttons, and
//! the Chats / Contacts / Calls tabs, fitted to any sidebar width.
//!
//! The side buttons never shrink. The tabs take the rest of the row and drop
//! to icon-only (with tooltips) when the labelled strip would not fit. The
//! decision is a pure function of the sidebar width and the visible buttons,
//! so it is recomputed on every render.

use super::*;
use crate::ui::chat_list_state::SidebarTab;
use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use quill::density::DensityMetrics;

/// Gap between header children (`gap_1`).
const HEADER_GAP: f32 = 4.;
/// Estimated width of one label character in the tab strip (`text_sm`, at
/// either density: Small tabs keep it).
const TAB_CHAR_WIDTH: f32 = 7.;

/// Width the labelled tab strip needs for these (translated) labels. Tabs grow
/// from their natural width, so the estimate is the sum over all of them; language
/// packs with longer words push the icon-only breakpoint up.
pub(super) fn labelled_tabs_width(labels: &[&str], m: &DensityMetrics) -> f32 {
    labels
        .iter()
        .map(|label| label.chars().count() as f32 * TAB_CHAR_WIDTH + m.tab_padding)
        .sum()
}

/// Whether the tab strip must show icons instead of labels. The sidebar's
/// insets and the header's icon buttons come from the density.
fn tabs_icon_only(
    sidebar_width: f32,
    side_buttons: usize,
    labelled_width: f32,
    m: &DensityMetrics,
) -> bool {
    let side = side_buttons as f32 * (m.icon_button + HEADER_GAP);
    sidebar_width - 2. * m.sidebar_inset_x - side < labelled_width
}

impl QuillApp {
    /// The header row above the chat list.
    pub(super) fn sidebar_header(&self, mode: PaneMode, cx: &mut Context<Self>) -> AnyElement {
        let ready = mode == PaneMode::Ready;
        let lock = ready && self.account.passcode.enabled;
        let proxy = self.proxy_shield_button(cx);
        let side_buttons = usize::from(ready) + usize::from(lock) + usize::from(proxy.is_some());
        let m = self.density();
        let labelled_width = labelled_tabs_width(
            &[
                tr(keys::RECENT_CHATS),
                tr(keys::MENU_CONTACTS),
                tr(keys::MENU_CALLS),
            ],
            m,
        );
        let icon_only = tabs_icon_only(
            f32::from(self.list_width()),
            side_buttons,
            labelled_width,
            m,
        );
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
        let selected = match self.chat_list.tab {
            SidebarTab::Chats => 0,
            SidebarTab::Contacts => 1,
            SidebarTab::Calls => 2,
        };
        let weak = cx.weak_entity();
        let tab = |icon: IconName, label: &'static str| {
            if icon_only {
                // Spread the icons evenly across the segmented bar.
                Tab::new()
                    .flex_1()
                    .icon(icon)
                    .aria_label(label)
                    .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            } else {
                // Each label keeps its natural width and the tabs share the
                // rest of the bar, so it never ends in empty space.
                Tab::new().flex_grow(1.).label(label)
            }
        };
        TabBar::new("list-tabs")
            .w_full()
            .segmented()
            .glass_selection()
            .with_size(self.control_size())
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
    use super::{labelled_tabs_width, tabs_icon_only};
    use quill::density::{COMPACT, DensityMetrics, STANDARD};
    use quill::settings::{MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH};

    const S: &DensityMetrics = &STANDARD;
    const C: &DensityMetrics = &COMPACT;

    /// "Chats", "Contacts", "Calls".
    fn english(m: &DensityMetrics) -> f32 {
        labelled_tabs_width(&["Chats", "Contacts", "Calls"], m)
    }

    #[test]
    fn english_labels_need_228_or_216_in_small_tabs() {
        assert_eq!(english(S), 228.);
        assert_eq!(english(C), 216.);
    }

    #[test]
    fn minimum_width_with_every_side_button_uses_icons() {
        for m in [S, C] {
            assert!(tabs_icon_only(MIN_SIDEBAR_WIDTH, 3, english(m), m));
            assert!(tabs_icon_only(MIN_SIDEBAR_WIDTH, 1, english(m), m));
        }
    }

    #[test]
    fn wide_sidebar_keeps_labels_even_with_every_side_button() {
        for m in [S, C] {
            assert!(!tabs_icon_only(MAX_SIDEBAR_WIDTH, 3, english(m), m));
        }
    }

    #[test]
    fn one_button_breakpoint_is_pinned() {
        // 24 padding + 36 for the menu button + 228 of labels.
        assert!(tabs_icon_only(240., 1, english(S), S));
        assert!(tabs_icon_only(287., 1, english(S), S));
        assert!(!tabs_icon_only(288., 1, english(S), S));
        // 16 padding + 28 for the Small menu button + 216 of Small tabs.
        assert!(tabs_icon_only(259., 1, english(C), C));
        assert!(!tabs_icon_only(260., 1, english(C), C));
    }

    #[test]
    fn longer_translations_push_the_breakpoint_up() {
        let german = labelled_tabs_width(&["Unterhaltungen", "Kontakte", "Anrufliste"], S);
        assert!(german > english(S));
        assert!(!tabs_icon_only(288., 1, english(S), S));
        assert!(tabs_icon_only(288., 1, german, S));
        assert!(!tabs_icon_only(288. + (german - english(S)), 1, german, S));
    }

    #[test]
    fn more_side_buttons_never_make_labels_fit_again() {
        for m in [S, C] {
            let mut width = MIN_SIDEBAR_WIDTH;
            while width <= MAX_SIDEBAR_WIDTH {
                for buttons in 0..3 {
                    if tabs_icon_only(width, buttons, english(m), m) {
                        assert!(
                            tabs_icon_only(width, buttons + 1, english(m), m),
                            "{width} {buttons}"
                        );
                    }
                }
                width += 1.;
            }
        }
    }

    #[test]
    fn widening_never_hides_labels_again() {
        for m in [S, C] {
            let mut was_labelled = false;
            let mut width = MIN_SIDEBAR_WIDTH;
            while width <= MAX_SIDEBAR_WIDTH {
                let labelled = !tabs_icon_only(width, 3, english(m), m);
                assert!(labelled || !was_labelled, "{width}");
                was_labelled = labelled;
                width += 1.;
            }
        }
    }
}

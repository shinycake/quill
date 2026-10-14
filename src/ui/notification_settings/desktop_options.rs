//! The notification settings' "System integration" options
//! (`parity:notify-desktop-options`, tdesktop `settings_notifications.cpp`):
//! "Use native notifications" where the system's notifications are not
//! enforced, "Respect system Focus mode" on Windows, and, while Quill's own
//! windows are in use, the display, the screen corner (hovering a corner
//! shows sample windows there) and the count. The windows are
//! `ui::notify_windows`.

use super::*;
use gpui_kit::component::tab::{Tab, TabBar};
use quill::notify::custom::{self, NotifyPlatform, ScreenCorner};

/// The screen drawing's size (tdesktop `st::notificationsBoxMonitor`).
const MONITOR_WIDTH: f32 = 224.0;
const MONITOR_HEIGHT: f32 = 140.0;

fn corner_name(corner: ScreenCorner) -> &'static str {
    match corner {
        ScreenCorner::TopLeft => "Top left",
        ScreenCorner::TopRight => "Top right",
        ScreenCorner::BottomRight => "Bottom right",
        ScreenCorner::BottomLeft => "Bottom left",
    }
}

fn corner_id(corner: ScreenCorner) -> usize {
    ScreenCorner::ALL
        .iter()
        .position(|c| *c == corner)
        .unwrap_or_default()
}

impl QuillApp {
    pub(in crate::ui) fn notify_options_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let support = self.native_notify_support();
        let prefs = quill::notify_prefs::current();
        let platform = self.notify.windows.platform;
        let custom_active = self.custom_notifications_active();
        let mut section = div()
            .id("notify-options-section")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .child(div().font_semibold().text_sm().child("System integration"));
        if custom::native_switch_shown(support) {
            let label = if platform == NotifyPlatform::Windows {
                "Use Windows notifications"
            } else {
                "Use native notifications"
            };
            let native = prefs.native.unwrap_or(support.by_default);
            section = section.child(switch_row(
                "notify-native-toggle",
                label,
                native,
                cx.listener(|this, &on: &bool, _, cx| {
                    quill::notify_prefs::update(|prefs| prefs.native = Some(on));
                    if on {
                        this.close_all_notify_windows(cx);
                    }
                    cx.notify();
                }),
            ));
        }
        if platform == NotifyPlatform::Windows {
            section = section.child(switch_row(
                "notify-focus-toggle",
                "Respect system Focus mode",
                prefs.skip_in_focus,
                cx.listener(|_, &on: &bool, _, cx| {
                    quill::notify_prefs::update(|prefs| prefs.skip_in_focus = on);
                    cx.notify();
                }),
            ));
        }
        if !custom_active {
            return section.into_any_element();
        }
        section = section.children(self.notify_display_choice(prefs.display, cx));
        section
            .child(sub_heading("Location on the screen", cx))
            .child(self.notify_corner_picker(prefs.corner, cx))
            .child(sub_heading("Notifications count", cx))
            .child(
                (1..=custom::MAX_COUNT)
                    .fold(TabBar::new("notify-count").segmented(), |bar, count| {
                        bar.child(Tab::new().label(count.to_string()))
                    })
                    .selected_index(custom::clamp_count(prefs.count).saturating_sub(1))
                    .on_click(cx.listener(|this, &ix: &usize, _, cx| {
                        let count = u8::try_from(ix + 1).unwrap_or(custom::DEFAULT_COUNT);
                        quill::notify_prefs::update(|prefs| prefs.count = count);
                        this.custom_notify_settings_changed(cx);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// tdesktop "Display for notifications": shown with two or more
    /// displays. Default follows the main window.
    fn notify_display_choice(&self, chosen: u64, cx: &mut Context<Self>) -> Option<AnyElement> {
        let displays = cx.displays();
        if displays.len() < 2 {
            return None;
        }
        let mut ids = vec![0u64];
        let mut labels = vec!["Default".to_string()];
        for display in &displays {
            let bounds = display.bounds();
            ids.push(crate::ui::notify_windows::display_checksum(
                display.as_ref(),
            ));
            labels.push(custom::display_label(
                bounds.size.width.as_f32(),
                bounds.size.height.as_f32(),
            ));
        }
        let selected = ids.iter().position(|id| *id == chosen).unwrap_or(0);
        let group = RadioGroup::vertical("notify-display")
            .selected_index(Some(selected))
            .children(
                labels
                    .into_iter()
                    .enumerate()
                    .map(|(i, label)| Radio::new(("notify-display-item", i)).label(label)),
            )
            .on_click(cx.listener(move |this, &ix: &usize, _, cx| {
                let display = ids.get(ix).copied().unwrap_or(0);
                quill::notify_prefs::update(|prefs| prefs.display = display);
                this.custom_notify_settings_changed(cx);
                cx.notify();
            }));
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(sub_heading("Display for notifications", cx))
                .child(group)
                .into_any_element(),
        )
    }

    /// tdesktop's screen drawing with a slot in each corner: the chosen
    /// one is filled; hovering one shows sample windows in that corner of
    /// the real screen.
    fn notify_corner_picker(&self, chosen: ScreenCorner, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let mut screen = div()
            .id("notify-corner-screen")
            .relative()
            .w(px(MONITOR_WIDTH))
            .h(px(MONITOR_HEIGHT))
            .rounded_md()
            .border_2()
            .border_color(theme.border)
            .bg(theme.muted);
        for corner in ScreenCorner::ALL {
            let slot = Button::new(("notify-corner", corner_id(corner)))
                .xsmall()
                .w(px(64.0))
                .tooltip(corner_name(corner))
                .map(|button| {
                    if corner == chosen {
                        button.primary()
                    } else {
                        button.outline()
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    quill::notify_prefs::update(|prefs| prefs.corner = corner);
                    this.custom_notify_settings_changed(cx);
                    this.notify_corner_preview(Some(corner), cx);
                    cx.notify();
                }));
            let place = div()
                .id(("notify-corner-slot", corner_id(corner)))
                .absolute()
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    this.notify_corner_preview(hovered.then_some(corner), cx);
                }))
                .child(slot);
            let place = if corner.is_top() {
                place.top(px(6.0))
            } else {
                place.bottom(px(6.0))
            };
            let place = if corner.is_left() {
                place.left(px(6.0))
            } else {
                place.right(px(6.0))
            };
            screen = screen.child(place);
        }
        div()
            .flex()
            .justify_center()
            .py_1()
            .child(screen)
            .into_any_element()
    }
}

fn sub_heading(text: &'static str, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn switch_row(
    id: &'static str,
    label: &'static str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(div().text_sm().child(label))
        .child(
            Switch::new(id)
                .checked(checked)
                .accessibility_label(label)
                .on_click(on_click),
        )
}

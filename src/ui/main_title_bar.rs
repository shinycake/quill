//! The main window's title bar: Quill's frame (`window_frame::FrameTitleBar`)
//! with the caption, the update button, Search, and in the offline demos the
//! auth-cycling button.
#![deny(clippy::indexing_slicing)]

use super::app::{PaneMode, QuillApp};
use super::window_frame::FrameTitleBar;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::{Sizable, Size, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::layout_style::Look;

/// `control` sizes the bar's buttons with the density
/// (`QuillApp::control_size`); the bar's own height follows it through the
/// kit theme (`Theme::title_bar_height`).
pub(super) fn main_title_bar(
    mode: PaneMode,
    control: Size,
    search_open: bool,
    update: Option<AnyElement>,
    menu_bar: Option<Entity<AppMenuBar>>,
    strip: Look,
    cx: &mut Context<QuillApp>,
) -> impl IntoElement {
    let title = "Quill";
    let show_cycle = mode == PaneMode::Synthetic;
    // kit Phase 7: kit `TitleBar` — native-feel chrome (drag, double-click
    // zoom, Linux min/max/close, macOS traffic-light inset) in theme tokens.
    // The caption + action buttons ride as its children.
    FrameTitleBar::new()
        .strip(strip)
        .on_close_window(cx.listener(|this, _, window, cx| {
            this.exit_after_exports(super::export_quit::quit_app, window, cx);
        }))
        .caption(super::chrome::frames::app_caption(
            div()
                .font_semibold()
                .text_sm()
                .child(title)
                .into_any_element(),
            strip,
        ))
        .when_some(menu_bar, |bar, menu| bar.menu(menu))
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .flex_none()
                .children(update)
                .when(mode == PaneMode::Ready, |this| {
                    let label = if search_open {
                        tr(keys::Q_MISC_CLOSE_SEARCH)
                    } else {
                        tr(keys::Q_MISC_SEARCH)
                    };
                    this.child(
                        Button::new("search")
                            .icon(if search_open {
                                IconName::X
                            } else {
                                IconName::Search
                            })
                            .ghost()
                            .with_size(control)
                            .tooltip(label)
                            .accessibility_label(label)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.search_is_open() {
                                    this.close_search_ui(window, cx);
                                } else {
                                    this.open_search_ui(window, cx);
                                }
                            })),
                    )
                })
                .when(show_cycle, |this| {
                    this.child(
                        Button::new("cycle-auth")
                            .icon(IconName::RotateCcw)
                            .ghost()
                            .with_size(control)
                            .tooltip(tr(keys::Q_MISC_CYCLE_AUTH_STATE))
                            .accessibility_label(tr(keys::Q_MISC_CYCLE_AUTH_STATE))
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_auth(cx))),
                    )
                }),
        )
}

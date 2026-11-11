//! The chooser's own window: opened once the sources are listed, and
//! shown by the stage that holds the task which opens it.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::screen_share_state::Chooser;
use super::{ScreenChooserView, window_size};
use crate::ui::frame_clock::follow_app;
use gpui_kit::component::Root;
use gpui_kit::*;

impl QuillApp {
    /// Make the chooser's window once this update is over. The stage holds
    /// the task: a chooser closed meanwhile cancels it, and no window is made.
    pub(super) fn open_screen_chooser_window(&mut self, cx: &mut Context<Self>) -> Task<()> {
        let owner = cx.entity();
        let screen = self.screen_epoch();
        self.spawn_screen(cx, async move |this, cx| {
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let result = cx.update(|cx| {
                let bounds = Bounds::centered(None, window_size(), cx);
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: Some(TitlebarOptions {
                            title: None,
                            appears_transparent: true,
                            traffic_light_position: Some(point(px(10.), px(8.))),
                        }),
                        is_resizable: false,
                        focus: true,
                        show: true,
                        ..Default::default()
                    },
                    move |window, cx| {
                        window.activate_window();
                        crate::ui::window_control::set_always_on_top(window, true);
                        window.on_window_should_close(cx, move |_, cx| {
                            let _ = weak.update(cx, |app, cx| {
                                app.in_screen_scope(screen, cx, |app, cx| {
                                    app.screen_chooser_closed(cx)
                                })
                            });
                            true
                        });
                        let view = cx.new(|cx| ScreenChooserView {
                            owner: render_owner.downgrade(),
                            _observe: follow_app(&render_owner, cx),
                        });
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
            });
            let _ = this.update(cx, |app, cx| {
                app.screen_chooser_window_made(result, cx);
                cx.notify();
            });
        })
    }

    /// The window opened (or could not): the chooser being made is shown in
    /// it, or goes.
    fn screen_chooser_window_made(
        &mut self,
        result: gpui::Result<WindowHandle<Root>>,
        cx: &mut Context<Self>,
    ) {
        let Ok(handle) = result else {
            self.screen_chooser_closed(cx);
            return;
        };
        let handle: AnyWindowHandle = handle.into();
        match std::mem::take(&mut self.screen_share.chooser) {
            Chooser::Opening { sheet, .. } => {
                self.screen_share.chooser = Chooser::Open {
                    sheet,
                    window: handle,
                };
            }
            // Not the chooser this window was made for (the task is dropped
            // with it, so this does not happen): the window goes too.
            other => {
                self.screen_share.chooser = other;
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                });
            }
        }
    }
}

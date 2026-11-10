//! Opening the 1:1 call's window (`QuillApp::sync_call_window` asks for it).
use crate::ui::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::Root;
use gpui_kit::*;
use quill::lang::{keys, tr};

impl QuillApp {
    pub(in crate::ui) fn open_call_window(&mut self, call_id: i32, cx: &mut Context<Self>) {
        if self.calls.window_opening {
            return;
        }
        self.calls.window_opening = true;
        self.calls.raised_for = None;
        let owner = cx.entity();
        // screen-scope: the call window is the app's; its failure note goes to whichever account is on screen.
        cx.defer(move |cx| {
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let panel_view = owner.read(cx).calls.panel_view.clone();
            let bounds = Bounds::centered(None, size(px(720.), px(540.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(380.), px(520.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some(tr(keys::PROFILE_ACTION_SHORT_CALL).into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(12.))),
                    }),
                    focus: true,
                    show: true,
                    ..Default::default()
                },
                move |window, cx| {
                    window.activate_window();
                    let gone = weak.clone();
                    window.on_window_should_close(cx, move |_, cx| {
                        let _ = weak.update(cx, |app, cx| {
                            app.calls.forget_window();
                            // The window is reused for later calls: it
                            // closes on the one it shows now.
                            let (shown, ..) = app.call_window_target();
                            app.call_screen.window_closed_by_user = shown.or(Some(call_id));
                            cx.notify();
                        });
                        true
                    });
                    let from = window.window_handle();
                    crate::ui::window_control::when_window_goes(window, cx, move |cx| {
                        let _ = gone.update(cx, |app, cx| {
                            if app.calls.window == Some(from) {
                                app.calls.forget_window();
                                cx.notify();
                            }
                        });
                    });
                    let view = cx.new(|cx| {
                        super::call_video::CallWindowRoot::new(&render_owner, &panel_view, cx)
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            owner.update(cx, |app, cx| {
                app.calls.window_opening = false;
                match result {
                    Ok(handle) => app.calls.window = Some(handle.into()),
                    Err(_) => {
                        // Kept shut for this call, like a window the user
                        // closed: the next sync does not try (and note) it
                        // again; the call bar or a ring reopens it.
                        app.call_screen.window_closed_by_user = Some(call_id);
                        app.set_note(StatusNote::error(tr(
                            keys::Q_OPS_COULDNT_OPEN_THE_CALL_WINDOW,
                        )));
                    }
                }
                cx.notify();
            });
        });
    }
}

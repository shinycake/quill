//! Picture-in-Picture draws the viewer's player in a floating window: the
//! same `video_frame` element (a GPU surface on macOS), pacing itself on
//! this window's frames, and the same transport.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::video_player::video_frame;
use gpui_kit::component::Root;
use gpui_kit::component::button::*;
use gpui_kit::*;

struct VideoPip {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
}
impl Render for VideoPip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.owner.upgrade().map(|owner| {
            let app = owner.read(cx);
            (app.viewer.player.clone(), app.viewer_video_playing())
        });
        let Some((Some(player), playing)) = snapshot else {
            window.remove_window();
            return div().into_any_element();
        };
        // The play button follows the transport (an ended clip).
        player.watch(cx.entity_id());
        let owner = self.owner.clone();
        let return_owner = self.owner.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x111111))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    // A floating window rarely has focus: keep drawing.
                    .child(
                        video_frame(&player)
                            .object_fit(ObjectFit::Contain)
                            .keep_alive(true),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(
                        Button::new("pip-play")
                            .label(if playing { "Pause" } else { "Play" })
                            .accessibility_label(if playing { "Pause video" } else { "Play video" })
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |app, cx| app.toggle_viewer_video(cx));
                            }),
                    )
                    .child(
                        Button::new("pip-return")
                            .label("Return to viewer")
                            .accessibility_label("Return to viewer")
                            .on_click(move |_, window, cx| {
                                let _ = return_owner.update(cx, |app, cx| {
                                    app.viewer.pip_window = None;
                                    cx.notify();
                                });
                                window.remove_window();
                            }),
                    ),
            )
            .into_any_element()
    }
}
impl QuillApp {
    pub(super) fn open_video_pip(&mut self, cx: &mut Context<Self>) {
        if !cfg!(target_os = "macos") {
            self.connection.status_note =
                "Picture-in-Picture requires native floating-window support.".into();
            cx.notify();
            return;
        }
        if self.viewer.player.is_none() {
            return;
        }
        if let Some(handle) = self.viewer.pip_window {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                return;
            }
            self.viewer.pip_window = None;
        }
        let weak_owner = cx.entity().downgrade();
        // Window creation renders synchronously; release the owner lease first.
        cx.defer(move |cx| {
            let Some(owner) = weak_owner.upgrade() else {
                return;
            };
            if owner.read(cx).viewer.player.is_none() {
                return;
            }
            if let Some(handle) = owner.read(cx).viewer.pip_window {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                return;
            }
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let result = cx.open_window(
                WindowOptions {
                    kind: WindowKind::Floating,
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: point(px(80.), px(80.)),
                        size: size(px(420.), px(290.)),
                    })),
                    window_min_size: Some(size(px(280.), px(210.))),
                    // GPUI caps an unfocused window's animation frames at
                    // 30 a second; Picture-in-Picture is unfocused by design
                    // and plays at the clip's own rate.
                    inactive_frame_interval: None,
                    titlebar: Some(TitlebarOptions {
                        title: Some("Quill — Picture-in-Picture".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    #[cfg(target_os = "macos")]
                    configure_pip_window(window);
                    // tdesktop's PiP carries `Qt::WindowStaysOnTopHint`.
                    super::window_control::set_always_on_top(window, true);
                    window.on_window_should_close(cx, move |_, cx| {
                        let _ = weak.update(cx, |app, cx| {
                            app.viewer.pip_window = None;
                            cx.notify();
                        });
                        true
                    });
                    let view = cx.new(|cx| VideoPip {
                        owner: render_owner.downgrade(),
                        _observe: cx.observe(&render_owner, |_, _, cx| cx.notify()),
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            owner.update(cx, |app, cx| {
                match result {
                    Ok(handle) => app.viewer.pip_window = Some(handle),
                    Err(_) => {
                        app.connection.status_note = "Could not open Picture-in-Picture".into()
                    }
                }
                cx.notify();
            });
        });
    }
}

#[cfg(target_os = "macos")]
fn configure_pip_window(window: &Window) {
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::AppKit(handle) = handle.as_raw()
    {
        // SAFETY: GPUI owns this NSView; its raw handle is valid during the window callback.
        let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        if let Some(panel) = view.window() {
            panel.setHidesOnDeactivate(false);
            panel.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
        }
    }
}

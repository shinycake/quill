//! Picture-in-Picture shares the viewer's decoded frames and playback clock.
use super::app::QuillApp;
use gpui_kit::component::Root;
use gpui_kit::component::button::*;
use gpui_kit::*;
use std::sync::Arc;

struct VideoPip {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
}
impl Render for VideoPip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.owner.upgrade().map(|owner| {
            let app = owner.read(cx);
            (
                app.viewer_render_frame(),
                app.viewer.clock.as_ref().is_some_and(|c| c.is_playing()),
            )
        });
        let Some((Some(frame), playing)) = snapshot else {
            window.remove_window();
            return div().into_any_element();
        };
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
                    .child(img(frame).size_full().object_fit(ObjectFit::Contain)),
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
    pub(super) fn viewer_render_frame(&self) -> Option<Arc<RenderImage>> {
        let elapsed = self
            .viewer
            .clock
            .as_ref()
            .map(|c| c.elapsed_secs())
            .unwrap_or(0.0);
        let at = (elapsed * self.viewer.video_fps) as usize;
        // A looping animation wraps; a video holds its last frame.
        let index = if self.viewer_loops() {
            at % self.viewer.video_frames.len().max(1)
        } else {
            at.min(self.viewer.video_frames.len().saturating_sub(1))
        };
        self.viewer.video_frames.get(index).cloned()
    }
    pub(super) fn open_video_pip(&mut self, cx: &mut Context<Self>) {
        if !cfg!(target_os = "macos") {
            self.status_note = "Picture-in-Picture requires native floating-window support.".into();
            cx.notify();
            return;
        }
        if self.viewer_render_frame().is_none() {
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
            if owner.read(cx).viewer_render_frame().is_none() {
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
                    titlebar: Some(TitlebarOptions {
                        title: Some("Quill — Picture-in-Picture".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    #[cfg(target_os = "macos")]
                    configure_pip_window(window);
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
                    Err(_) => app.status_note = "Could not open Picture-in-Picture".into(),
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
        // GPUI owns this NSView; its raw handle is valid during the window callback.
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

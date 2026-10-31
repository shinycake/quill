//! The What's New window: every Quill release with its notes, newest first,
//! in a window of its own. It opens from the settings sidebar, About and the
//! update banner, and never on its own.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use super::frame_clock::follow_app;
use super::window_frame::{FrameTitleBar, titlebar_hidden, window_decorations};
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use quill::release_notes::{ReleaseNote, Standing};
use std::rc::Rc;

#[cfg(feature = "demo-capture")]
#[path = "whats_new_demo.rs"]
mod demo;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "whats_new_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) enum Notes {
    #[default]
    Idle,
    Loading,
    Loaded(Rc<Vec<ReleaseNote>>),
    Failed,
}

#[derive(Default)]
pub(crate) struct WhatsNewUi {
    window: Option<AnyWindowHandle>,
    opening: bool,
    pub(crate) notes: Notes,
}

impl QuillApp {
    /// Shows the window, raising it when it is already open, and loads the
    /// notes the first time (or again after a failure).
    pub(crate) fn open_whats_new(&mut self, cx: &mut Context<Self>) {
        if matches!(self.settings.whats_new.notes, Notes::Idle | Notes::Failed) {
            self.load_release_notes(cx);
        }
        if let Some(handle) = self.settings.whats_new.window {
            let raised = handle.update(cx, |_, window, _| window.activate_window());
            if raised.is_ok() {
                return;
            }
            self.settings.whats_new.window = None;
        }
        if self.settings.whats_new.opening {
            return;
        }
        self.settings.whats_new.opening = true;
        let owner = cx.entity();
        cx.defer(move |cx| {
            let handle = open_window(owner.clone(), cx);
            owner.update(cx, |app, cx| {
                app.settings.whats_new.opening = false;
                app.settings.whats_new.window = handle;
                cx.notify();
            });
        });
    }

    fn load_release_notes(&mut self, cx: &mut Context<Self>) {
        if self.demo_session.is_some() {
            return;
        }
        self.settings.whats_new.notes = Notes::Loading;
        let fetch = cx
            .background_executor()
            .spawn(async move { quill::release_notes::fetch() });
        cx.spawn(async move |this, cx| {
            let result = fetch.await;
            let _ = this.update(cx, |app, cx| {
                app.settings.whats_new.notes = match result {
                    Ok(notes) => Notes::Loaded(Rc::new(notes)),
                    Err(_) => Notes::Failed,
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn whats_new_closed(&mut self, cx: &mut Context<Self>) {
        self.settings.whats_new.window = None;
        cx.notify();
    }
}

fn open_window(owner: Entity<QuillApp>, cx: &mut App) -> Option<AnyWindowHandle> {
    let bounds = Bounds::centered(None, size(px(620.), px(760.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(420.), px(420.))),
        titlebar: Some(TitlebarOptions {
            title: Some(tr(keys::Q_WHATS_NEW_TITLE).into()),
            appears_transparent: titlebar_hidden(),
            ..TitleBar::title_bar_options()
        }),
        app_owns_titlebar_drag: true,
        window_decorations: window_decorations(),
        app_id: Some("org.shinycake.quill".into()),
        focus: true,
        show: true,
        ..Default::default()
    };
    let handle = cx.open_window(options, move |window, cx| {
        let weak = owner.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let _ = weak.update(cx, |app, cx| app.whats_new_closed(cx));
            true
        });
        window.activate_window();
        let view = cx.new(|cx| WhatsNewView {
            _observe: follow_app(&owner, cx),
            owner: owner.downgrade(),
            focus: cx.focus_handle(),
        });
        window.focus(&view.focus_handle(cx), cx);
        cx.new(|cx| Root::new(view, window, cx))
    });
    match handle {
        Ok(handle) => Some(handle.into()),
        Err(error) => {
            eprintln!("quill: could not open the What's New window: {error}");
            None
        }
    }
}

pub(crate) struct WhatsNewView {
    owner: WeakEntity<QuillApp>,
    focus: FocusHandle,
    _observe: Subscription,
}

impl Focusable for WhatsNewView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn close(owner: &WeakEntity<QuillApp>, window: &mut Window, cx: &mut App) {
    let _ = owner.update(cx, |app, cx| app.whats_new_closed(cx));
    window.remove_window();
}

impl Render for WhatsNewView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = self.owner.clone();
        super::window_frame::sync(
            window,
            owner
                .upgrade()
                .is_some_and(|app| app.read(cx).appearance.native_window_frame),
        );
        let body = owner
            .upgrade()
            .map(|app| app.update(cx, |app, cx| app.whats_new_body(cx)));
        let theme = cx.theme();
        div()
            .id("whats-new-window")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action({
                let owner = owner.clone();
                move |_: &super::actions::CloseWindow, window, cx| close(&owner, window, cx)
            })
            .on_key_down({
                let owner = owner.clone();
                move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        close(&owner, window, cx);
                    }
                }
            })
            .child(FrameTitleBar::new().on_close_window({
                let owner = owner.clone();
                move |_, window, cx| close(&owner, window, cx)
            }))
            .child(
                div()
                    .id("whats-new-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(body),
            )
    }
}

impl QuillApp {
    /// The window's content, from the loaded notes alone.
    fn whats_new_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let content = match &self.settings.whats_new.notes {
            Notes::Idle | Notes::Loading => div()
                .id("whats-new-loading")
                .flex()
                .items_center()
                .gap_2()
                .py_8()
                .text_sm()
                .text_color(muted)
                .child(Spinner::new().small())
                .child(tr(keys::Q_WHATS_NEW_LOADING))
                .into_any_element(),
            Notes::Failed => div()
                .id("whats-new-failed")
                .flex()
                .flex_col()
                .items_start()
                .gap_3()
                .py_8()
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(keys::Q_WHATS_NEW_FAILED)),
                )
                .child(
                    Button::new("whats-new-retry")
                        .label(tr(keys::Q_WHATS_NEW_RETRY))
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| this.load_release_notes(cx))),
                )
                .into_any_element(),
            Notes::Loaded(notes) => {
                let notes = notes.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .children(notes.iter().map(|note| release_section(note, cx)))
                    .into_any_element()
            }
        };
        div()
            .w_full()
            .max_w(px(640.))
            .mx_auto()
            .px_8()
            .pt_6()
            .pb_10()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_2xl()
                            .font_semibold()
                            .child(tr(keys::Q_WHATS_NEW_TITLE)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(quill::settings_account::current_version_line()),
                    ),
            )
            .child(content)
            .child(
                div().flex().child(
                    Button::new("whats-new-all-releases")
                        .label(tr(keys::Q_WHATS_NEW_ALL_RELEASES))
                        .link()
                        .small()
                        .on_click(|_, _, cx| cx.open_url(quill::updater::RELEASES_URL)),
                ),
            )
            .into_any_element()
    }
}

/// One release: its version, how it relates to this Quill, the date it came
/// out, and its notes.
fn release_section(note: &ReleaseNote, cx: &mut Context<QuillApp>) -> AnyElement {
    let theme = cx.theme();
    let standing = note.standing(quill::version::APP);
    let date = note.published.map(|unix| {
        quill::local_time::day_label(
            &quill::local_time::civil_local(unix),
            &quill::local_time::civil_local(quill::local_time::now_unix()),
        )
    });
    let url = note.url.clone();
    let id = SharedString::from(format!("whats-new-{}", note.version));
    let selector = format!("whats-new-release-{}", note.version);
    div()
        .id(id.clone())
        .debug_selector(move || selector.clone())
        .flex()
        .flex_col()
        .gap_3()
        .pt_6()
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().text_lg().font_semibold().child(tr_with(
                    keys::Q_OPS_QUILL,
                    &[("quill_version_app", note.version.as_str())],
                )))
                .when(standing == Standing::Installed, |row| {
                    row.child(
                        div().debug_selector(|| "whats-new-installed".into()).child(
                            Tag::secondary()
                                .small()
                                .child(tr(keys::Q_WHATS_NEW_INSTALLED)),
                        ),
                    )
                })
                .when(standing == Standing::Newer, |row| {
                    row.child(
                        Tag::primary()
                            .small()
                            .child(tr(keys::Q_WHATS_NEW_AVAILABLE)),
                    )
                })
                .when(note.prerelease, |row| {
                    row.child(
                        Tag::secondary()
                            .outline()
                            .small()
                            .child(tr(keys::Q_WHATS_NEW_BETA)),
                    )
                })
                .child(div().flex_1())
                .children(date.map(|date| {
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(date)
                })),
        )
        .child(if note.notes.is_empty() {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(tr(keys::Q_OPS_NO_RELEASE_NOTES_WERE_PROVIDED))
                .into_any_element()
        } else {
            TextView::markdown(
                SharedString::from(format!("{id}-notes")),
                note.notes.clone(),
            )
            .text_sm()
            // The notes' own headings sit under the version, not above it.
            .style(
                TextViewStyle::default()
                    .heading_font_size(|level, _| px(if level <= 2 { 15. } else { 14. })),
            )
            .selectable(true)
            .into_any_element()
        })
        .child(
            div().flex().child(
                Button::new(SharedString::from(format!("{id}-github")))
                    .label(tr(keys::Q_OPS_VIEW_RELEASE_ON_GITHUB))
                    .link()
                    .small()
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            ),
        )
        .into_any_element()
}

#[cfg(feature = "demo-capture")]
impl QuillApp {
    /// Screenshot demos and tests: the notes as if just loaded.
    pub(crate) fn use_release_notes(&mut self, notes: Vec<ReleaseNote>) {
        self.settings.whats_new.notes = Notes::Loaded(Rc::new(notes));
    }

    #[cfg(test)]
    pub(crate) fn whats_new_window(&self) -> Option<AnyWindowHandle> {
        self.settings.whats_new.window
    }
}

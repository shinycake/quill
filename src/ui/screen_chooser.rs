//! Choosing what to share, as tdesktop does it
//! (`calls/group/ui/desktop_capture_choose_source.cpp`):
//! - A window of its own, above the others, sized for two and a half rows
//!   of a three-column grid. Screens come first ("Screen 1", "Screen 2"),
//!   then windows by title ("Window N" when untitled).
//! - Each source shows one picture, taken when the chooser opens.
//! - A click selects. The first screen starts selected, or the source you
//!   are sharing. The bottom bar has Cancel and Share Screen, or Stop
//!   Sharing when the selection (and the audio box) is what you share.
//! - Where system audio can be shared (Windows), a "Share System Audio"
//!   box sits bottom left, starting as the running share has it.
//! - Without screen recording access, a box over the call window asks for
//!   it and opens the OS privacy settings (tdesktop
//!   `ScreenSharingPrivacyRequestBox`).
//!
//! - Where the OS shows its own picker (the Wayland screen cast portal),
//!   there is no chooser: the share button starts or stops sharing.
//!
//! Only builds with a screen backend (`calls::capture::screen`) show it.
//! Elsewhere calls cannot share a screen.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::group_call_panel::{MEMBER_MUTED_ICON, MEMBERS_BG, MEMBERS_BG_OVER};
use super::screen_share_state::{ChooserSource, ScreenFor};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, Root};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::capture::screen::{
    SYSTEM_PICKER_SOURCE, ScreenAccess, ScreenBackend, ScreenSource, SourceId, SourceKind,
    SourceLabel, Thumbnail, native_backend, source_labels,
};
use quill::lang::{keys, tr, tr_with};
use smallvec::SmallVec;
use std::sync::Arc;

/// tdesktop `desktopCaptureMargins`.
const MARGIN_LEFT: f32 = 12.;
const MARGIN_TOP: f32 = 8.;
const MARGIN_BOTTOM: f32 = 6.;
/// tdesktop `desktopCaptureSourceSize` and `desktopCaptureSourceSkips`.
const SOURCE_W: f32 = 235.;
const SOURCE_H: f32 = 165.;
const SKIP_X: f32 = 2.;
const SKIP_Y: f32 = 10.;
const COLUMNS: usize = 3;
/// tdesktop `desktopCapturePadding` (left, top, right, bottom).
const PAD: (f32, f32, f32, f32) = (7., 7., 7., 33.);
const LABEL_BOTTOM: f32 = 7.;
/// tdesktop box buttons are 34 px tall, 19 px (`margins.right +
/// padding.right`) from the edges.
const BUTTON_H: f32 = 34.;
const BOTTOM_SKIP: f32 = 19.;
const BOTTOM_H: f32 = 2. * BOTTOM_SKIP + BUTTON_H;
/// The window title strip (`desktopCaptureSourceTitle`).
const TITLE_H: f32 = 28.;
/// `groupCallMuted1` (the selected source, Share Screen) and
/// `groupCallActiveFg` (Cancel).
const SELECTED_BG: u32 = 0x0992ef;
const ACTIVE_FG: u32 = 0x4db8ff;
/// Thumbnails are taken at twice the source's picture area.
const THUMB_MAX: (u32, u32) = (2 * 221, 2 * 125);

/// The chooser window's size: two and a half rows of three columns.
fn window_size() -> Size<Pixels> {
    let width = 2. * MARGIN_LEFT + COLUMNS as f32 * SOURCE_W + (COLUMNS as f32 - 1.) * SKIP_X;
    let height = MARGIN_TOP + 2. * SOURCE_H + SKIP_Y + SOURCE_H / 2. + BOTTOM_H + TITLE_H;
    size(px(width), px(height))
}

/// What the chooser's tdesktop rules pick first: the shared source when it
/// is listed, otherwise (not sharing) the first screen.
pub(super) fn initial_selection(
    sources: &[ScreenSource],
    active: Option<&SourceId>,
) -> Option<SourceId> {
    match active {
        Some(active) => sources
            .iter()
            .find(|source| &source.id == active)
            .map(|source| source.id.clone()),
        None => sources
            .iter()
            .find(|source| source.kind == SourceKind::Screen)
            .map(|source| source.id.clone()),
    }
}

/// Whether the bottom bar shows Stop Sharing instead of Share Screen
/// (tdesktop `updateButtonsVisibility`): the shared source is selected
/// and the audio box matches the running share.
pub(super) fn shows_stop(
    selected: Option<&SourceId>,
    active: Option<&SourceId>,
    with_audio: bool,
    active_with_audio: bool,
) -> bool {
    active.is_some()
        && (selected.is_none() || selected == active)
        && with_audio == active_with_audio
}

fn label_text(label: SourceLabel) -> String {
    match label {
        SourceLabel::Screen(index) => tr_with(
            keys::GROUP_CALL_SCREEN_TITLE,
            &[("index", &index.to_string())],
        ),
        SourceLabel::Window(title) => title,
        SourceLabel::UntitledWindow(index) => {
            tr_with(keys::Q_OPS_WINDOW_INDEX, &[("index", &index.to_string())])
        }
    }
}

/// A thumbnail as GPUI wants it (BGRA).
fn thumbnail_image(thumbnail: Thumbnail) -> Option<Arc<RenderImage>> {
    let mut buffer = image::RgbaImage::from_raw(thumbnail.width, thumbnail.height, thumbnail.rgba)?;
    for pixel in buffer.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(SmallVec::from_buf([
        image::Frame::new(buffer),
    ]))))
}

impl QuillApp {
    /// The screen backend the chooser lists from: the injected one (demos,
    /// tests) or, when the driver captures the screen itself, the OS one.
    pub(super) fn screen_backend(&self) -> Option<Arc<dyn ScreenBackend>> {
        if let Some(backend) = self.screen_share.backend.clone() {
            return Some(backend);
        }
        let enabled = self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.call_screen().enabled());
        native_backend().filter(|_| enabled)
    }

    /// Whether sharing goes through the chooser.
    pub(super) fn uses_screen_chooser(&self) -> bool {
        self.screen_backend().is_some()
    }

    /// The source being shared now (tdesktop
    /// `chooseSourceActiveDeviceId`).
    pub(super) fn active_screen_source(&self) -> Option<SourceId> {
        match self.live.as_ref() {
            Some(live) => {
                let screen = live.driver.call_screen();
                screen.source().cloned().or_else(|| {
                    let session = &live.driver.session;
                    let sharing = session
                        .calls
                        .active_call
                        .as_ref()
                        .is_some_and(|call| call.screen_sharing)
                        || session
                            .calls
                            .active_group_call
                            .as_ref()
                            .is_some_and(|call| call.screen_sharing || call.screen_share_pending);
                    screen.chosen().filter(|_| sharing).cloned()
                })
            }
            None => self.screen_share.demo_active.clone(),
        }
    }

    /// Whether the chooser offers "Share System Audio" (tdesktop
    /// `LoopbackAudioCaptureSupported`).
    pub(super) fn offers_system_audio(&self) -> bool {
        match self.live.as_ref() {
            Some(live) => live.driver.call_screen().system_audio_enabled(),
            None => self.screen_share.demo_system_audio,
        }
    }

    /// Whether the running share includes system audio (tdesktop
    /// `chooseSourceActiveWithAudio`).
    fn active_screen_audio(&self) -> bool {
        match self.live.as_ref() {
            Some(live) => live.driver.call_screen().with_audio(),
            None => self.screen_share.demo_active_audio,
        }
    }

    pub(super) fn set_screen_audio(&mut self, on: bool, cx: &mut Context<Self>) {
        self.screen_share.with_audio = on;
        cx.notify();
    }

    /// The share button: the privacy box, or the chooser. Returns false
    /// when this build has no chooser (the caller shares directly).
    pub(super) fn open_screen_chooser(
        &mut self,
        for_call: ScreenFor,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(backend) = self.screen_backend() else {
            return false;
        };
        if backend.access() == ScreenAccess::Denied {
            self.screen_share.privacy_for = Some(for_call);
            cx.notify();
            return true;
        }
        if backend.system_picker() {
            // The OS asks what to share (Wayland): no chooser, the button
            // starts or stops (tdesktop `uniqueDesktopCaptureSource`).
            self.share_with_system_picker(for_call, cx);
            return true;
        }
        self.screen_share.for_call = Some(for_call);
        if let Some(handle) = self.screen_share.window {
            // tdesktop `activate()`: one chooser per call, brought forward.
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return true;
        }
        if self.screen_share.opening {
            return true;
        }
        self.screen_share.opening = true;
        self.screen_share.generation = self.screen_share.generation.wrapping_add(1);
        let generation = self.screen_share.generation;
        cx.spawn(async move |this, cx| {
            let listing = backend.clone();
            let sources = cx
                .background_executor()
                .spawn(async move { listing.sources() })
                .await;
            let opened = this.update(cx, |app, cx| {
                app.screen_chooser_listed(generation, sources, cx)
            });
            if opened.unwrap_or(false) {
                load_thumbnails(this, backend, generation, cx).await;
            }
        })
        .detach();
        true
    }

    /// Start the system picker's share, or stop the running one.
    fn share_with_system_picker(&mut self, for_call: ScreenFor, cx: &mut Context<Self>) {
        self.screen_share.for_call = Some(for_call);
        if self.active_screen_source().is_some() {
            self.stop_screen_source(cx);
        } else {
            self.screen_share.selected = Some(SourceId(SYSTEM_PICKER_SOURCE.into()));
            self.screen_share.with_audio = false;
            self.accept_screen_source(cx);
        }
    }

    /// The sources are in: an empty list shows nothing (tdesktop destroys
    /// the chooser), otherwise the window opens.
    fn screen_chooser_listed(
        &mut self,
        generation: u64,
        sources: Result<Vec<ScreenSource>, quill::calls::capture::CaptureError>,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.screen_share.generation {
            return false;
        }
        self.screen_share.opening = false;
        let sources = match sources {
            Ok(sources) => sources,
            Err(err) => {
                self.connection.set_note(StatusNote::error(err.to_string()));
                cx.notify();
                return false;
            }
        };
        if sources.is_empty() {
            self.screen_share.for_call = None;
            cx.notify();
            return false;
        }
        let active = self.active_screen_source();
        self.screen_share.selected = initial_selection(&sources, active.as_ref());
        self.screen_share.with_audio = active.is_some() && self.active_screen_audio();
        let labels = source_labels(&sources);
        self.screen_share.sources = sources
            .into_iter()
            .zip(labels)
            .map(|(source, label)| ChooserSource {
                source,
                label: label_text(label),
                thumbnail: None,
            })
            .collect();
        self.open_screen_chooser_window(cx);
        true
    }

    fn open_screen_chooser_window(&mut self, cx: &mut Context<Self>) {
        let owner = cx.entity();
        cx.defer(move |cx| {
            let weak = owner.downgrade();
            let render_owner = owner.clone();
            let bounds = Bounds::centered(None, window_size(), cx);
            let result = cx.open_window(
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
                    super::window_control::set_always_on_top(window, true);
                    window.on_window_should_close(cx, move |_, cx| {
                        let _ = weak.update(cx, |app, cx| app.screen_chooser_closed(cx));
                        true
                    });
                    let view = cx.new(|cx| ScreenChooserView {
                        owner: render_owner.downgrade(),
                        _observe: cx.observe(&render_owner, |_, _, cx| cx.notify()),
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            );
            owner.update(cx, |app, cx| {
                match result {
                    Ok(handle) => app.screen_share.window = Some(handle.into()),
                    Err(_) => app.screen_chooser_closed(cx),
                }
                cx.notify();
            });
        });
    }

    /// The window is gone: forget the chooser.
    fn screen_chooser_closed(&mut self, cx: &mut Context<Self>) {
        let share = &mut self.screen_share;
        share.window = None;
        share.opening = false;
        share.for_call = None;
        share.sources.clear();
        share.selected = None;
        share.with_audio = false;
        share.generation = share.generation.wrapping_add(1);
        cx.notify();
    }

    /// Cancel, and after Share Screen or Stop Sharing.
    pub(super) fn close_screen_chooser(&mut self, cx: &mut Context<Self>) {
        let handle = self.screen_share.window.take();
        self.screen_chooser_closed(cx);
        if let Some(handle) = handle {
            // The click that closes it runs inside that window: remove it
            // once the update is over.
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    pub(super) fn select_screen_source(&mut self, id: SourceId, cx: &mut Context<Self>) {
        self.screen_share.selected = Some(id);
        cx.notify();
    }

    /// Share Screen: share the selection, or switch a running share to it
    /// (tdesktop `chooseSourceAccepted`).
    pub(super) fn accept_screen_source(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(for_call)) = (
            self.screen_share.selected.clone(),
            self.screen_share.for_call,
        ) else {
            return;
        };
        let with_audio = self.screen_share.with_audio && self.offers_system_audio();
        if let Some(live) = self.live.as_mut() {
            live.driver.choose_call_screen_source(id, with_audio);
            let session = &live.driver.session;
            let sharing = match for_call {
                ScreenFor::Group => session
                    .calls
                    .active_group_call
                    .as_ref()
                    .is_some_and(|call| call.screen_sharing || call.screen_share_pending),
                ScreenFor::Private(_) => session
                    .calls
                    .active_call
                    .as_ref()
                    .is_some_and(|call| call.screen_sharing),
            };
            if !sharing {
                self.start_screen_share(for_call, cx);
            }
        } else {
            self.screen_share.demo_active = Some(id);
            self.screen_share.demo_active_audio = with_audio;
            self.set_demo_screen_sharing(for_call, true);
        }
        self.close_screen_chooser(cx);
    }

    /// Stop Sharing (tdesktop `chooseSourceStop`).
    pub(super) fn stop_screen_source(&mut self, cx: &mut Context<Self>) {
        let Some(for_call) = self.screen_share.for_call else {
            return;
        };
        if self.live.is_some() {
            self.stop_screen_share(for_call, cx);
        } else {
            self.screen_share.demo_active = None;
            self.screen_share.demo_active_audio = false;
            self.set_demo_screen_sharing(for_call, false);
        }
        self.close_screen_chooser(cx);
    }

    fn start_screen_share(&mut self, for_call: ScreenFor, cx: &mut Context<Self>) {
        match for_call {
            ScreenFor::Group => self.share_group_call_screen(cx),
            ScreenFor::Private(_) => self.share_call_screen(true, cx),
        }
    }

    fn stop_screen_share(&mut self, for_call: ScreenFor, cx: &mut Context<Self>) {
        let sharing = self.session().is_some_and(|session| match for_call {
            ScreenFor::Group => session
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.screen_sharing || call.screen_share_pending),
            ScreenFor::Private(_) => session
                .calls
                .active_call
                .as_ref()
                .is_some_and(|call| call.screen_sharing),
        });
        if !sharing {
            return;
        }
        match for_call {
            ScreenFor::Group => self.share_group_call_screen(cx),
            ScreenFor::Private(_) => self.share_call_screen(false, cx),
        }
    }

    fn set_demo_screen_sharing(&mut self, for_call: ScreenFor, on: bool) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        match for_call {
            ScreenFor::Group => {
                if let Some(call) = session.calls.active_group_call.as_mut() {
                    call.screen_sharing = on;
                    call.screen_share_paused = false;
                }
            }
            ScreenFor::Private(_) => {
                if let Some(call) = session.calls.active_call.as_mut() {
                    call.screen_sharing = on;
                }
            }
        }
    }

    /// The chooser window's content.
    fn screen_chooser_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.active_screen_source();
        let selected = self.screen_share.selected.clone();
        let offers_audio = self.offers_system_audio();
        let with_audio = offers_audio && self.screen_share.with_audio;
        let stop = shows_stop(
            selected.as_ref(),
            active.as_ref(),
            with_audio,
            active.is_some() && self.active_screen_audio(),
        );
        let mut grid = div()
            .flex()
            .flex_wrap()
            .gap_x(px(SKIP_X))
            .gap_y(px(SKIP_Y))
            .pt(px(MARGIN_TOP))
            .pb(px(MARGIN_BOTTOM))
            .px(px(MARGIN_LEFT));
        for (index, entry) in self.screen_share.sources.iter().enumerate() {
            let chosen = selected.as_ref() == Some(&entry.source.id);
            grid = grid.child(source_view(index, entry, chosen, cx));
        }
        let cancel_look = ButtonCustomVariant::new(cx)
            .color(rgb(MEMBERS_BG).into())
            .foreground(rgb(ACTIVE_FG).into())
            .hover(rgb(MEMBERS_BG_OVER).into())
            .active(rgb(MEMBERS_BG_OVER).into());
        let main = if stop {
            Button::new("screen-chooser-stop")
                .label(tr(keys::GROUP_CALL_SCREEN_SHARE_STOP).to_uppercase())
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(rgb(MEMBERS_BG).into())
                        .foreground(rgb(MEMBER_MUTED_ICON).into())
                        .hover(rgb(MEMBERS_BG_OVER).into())
                        .active(rgb(MEMBERS_BG_OVER).into()),
                )
                .on_click(cx.listener(|this, _, _, cx| this.stop_screen_source(cx)))
        } else {
            Button::new("screen-chooser-share")
                .label(tr(keys::GROUP_CALL_SCREEN_SHARE_START).to_uppercase())
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(rgb(SELECTED_BG).into())
                        .foreground(white())
                        .hover(rgb(SELECTED_BG).into())
                        .active(rgb(SELECTED_BG).into()),
                )
                .on_click(cx.listener(|this, _, _, cx| this.accept_screen_source(cx)))
        };
        let audio_box = offers_audio.then(|| {
            Checkbox::new("screen-chooser-audio")
                .checked(with_audio)
                // The kit label is the theme's foreground; the chooser is
                // dark like the call window (tdesktop `groupCallBoxCheckbox`).
                .child(
                    div()
                        .text_color(white())
                        .child(tr(keys::GROUP_CALL_SCREEN_SHARE_AUDIO)),
                )
                .on_click(
                    cx.listener(|this, checked: &bool, _, cx| this.set_screen_audio(*checked, cx)),
                )
        });
        let bottom = div()
            .h(px(BOTTOM_H))
            .flex_none()
            .px(px(BOTTOM_SKIP))
            .flex()
            .items_center()
            .gap(px(BOTTOM_SKIP))
            .child(div().flex_1().min_w_0().children(audio_box))
            .child(
                Button::new("screen-chooser-cancel")
                    .label(tr(keys::CANCEL).to_uppercase())
                    .custom(cancel_look)
                    .h(px(BUTTON_H))
                    .on_click(cx.listener(|this, _, _, cx| this.close_screen_chooser(cx))),
            )
            .child(main.h(px(BUTTON_H)));
        div()
            .id("screen-chooser")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(MEMBERS_BG))
            .text_color(white())
            .child(div().h(px(TITLE_H)).flex_none().bg(rgb(MEMBERS_BG_OVER)))
            .child(
                div()
                    .id("screen-chooser-sources")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(grid),
            )
            .child(bottom)
            .into_any_element()
    }

    /// The box over a call window when Quill may not record the screen.
    /// `group` picks the voice chat window or the 1:1 call window.
    pub(super) fn screen_privacy_overlay(
        &self,
        group: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let shown = match self.screen_share.privacy_for? {
            ScreenFor::Group => group,
            ScreenFor::Private(_) => !group,
        };
        if !shown {
            return None;
        }
        Some(
            div()
                .id("screen-privacy")
                .absolute()
                .inset_0()
                .bg(cx.theme().overlay)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(320.))
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(div().text_sm().child(tr(keys::Q_OPS_SCREEN_ACCESS_NEEDED)))
                        .child(bold_marked(tr(keys::Q_OPS_SCREEN_ACCESS_ALLOW)))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("screen-privacy-cancel")
                                        .label(tr(keys::CANCEL))
                                        .ghost()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_screen_privacy(cx)
                                        })),
                                )
                                .child(
                                    Button::new("screen-privacy-settings")
                                        .label(tr(keys::GROUP_CALL_MAC_SETTINGS))
                                        .primary()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.open_screen_privacy_settings(cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(super) fn close_screen_privacy(&mut self, cx: &mut Context<Self>) {
        self.screen_share.privacy_for = None;
        cx.notify();
    }

    /// Open Settings: ask the OS, open its privacy pane, close the box.
    fn open_screen_privacy_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(backend) = self.screen_backend() {
            cx.background_executor()
                .spawn(async move { backend.open_privacy_settings() })
                .detach();
        }
        self.close_screen_privacy(cx);
    }
}

/// `text` with its `**bold**` runs in semibold (tdesktop marked text).
pub(super) fn bold_marked(text: &str) -> Div {
    let (plain, bold) = split_bold(text);
    let style = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let highlights = bold
        .into_iter()
        .map(|range| (range, style))
        .collect::<Vec<_>>();
    div()
        .text_sm()
        .child(StyledText::new(plain).with_highlights(highlights))
}

/// `text` without its `**` marks, and the byte ranges they enclosed.
pub(super) fn split_bold(text: &str) -> (String, Vec<std::ops::Range<usize>>) {
    let mut plain = String::with_capacity(text.len());
    let mut bold = Vec::new();
    for (index, part) in text.split("**").enumerate() {
        let start = plain.len();
        plain.push_str(part);
        if index % 2 == 1 && !part.is_empty() {
            bold.push(start..plain.len());
        }
    }
    (plain, bold)
}

/// One source: its picture over the label; blue when selected, lighter
/// on hover (tdesktop `Source::paint`).
fn source_view(
    index: usize,
    entry: &ChooserSource,
    chosen: bool,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let id = entry.source.id.clone();
    let picture = entry.thumbnail.clone().map(|image| {
        img(ImageSource::Render(image))
            .size_full()
            .object_fit(ObjectFit::Contain)
    });
    div()
        .id(("screen-source", index))
        .relative()
        .w(px(SOURCE_W))
        .h(px(SOURCE_H))
        .rounded(px(10.))
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(SharedString::from(entry.label.clone()))
        .when(chosen, |this| this.bg(rgb(SELECTED_BG)))
        .when(!chosen, |this| {
            this.hover(|style| style.bg(rgb(MEMBERS_BG_OVER)))
        })
        .on_click(cx.listener(move |this, _, _, cx| this.select_screen_source(id.clone(), cx)))
        .child(
            div()
                .absolute()
                .left(px(PAD.0))
                .top(px(PAD.1))
                .w(px(SOURCE_W - PAD.0 - PAD.2))
                .h(px(SOURCE_H - PAD.1 - PAD.3))
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .children(picture),
        )
        .child(
            div()
                .absolute()
                .left(px(PAD.0))
                .right(px(PAD.2))
                .bottom(px(LABEL_BOTTOM))
                .flex()
                .justify_center()
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(entry.label.clone()),
                ),
        )
        .into_any_element()
}

/// One picture per source, in order, each shown as it arrives.
async fn load_thumbnails(
    this: WeakEntity<QuillApp>,
    backend: Arc<dyn ScreenBackend>,
    generation: u64,
    cx: &mut AsyncApp,
) {
    let Ok(sources) = this.update(cx, |app, _| {
        app.screen_share
            .sources
            .iter()
            .map(|entry| entry.source.clone())
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    for source in sources {
        let backend = backend.clone();
        let wanted = source.id.clone();
        let image = cx
            .background_executor()
            .spawn(async move {
                backend
                    .thumbnail(&source, THUMB_MAX)
                    .and_then(thumbnail_image)
            })
            .await;
        let current = this.update(cx, |app, cx| {
            if app.screen_share.generation != generation {
                return false;
            }
            if let Some(entry) = app
                .screen_share
                .sources
                .iter_mut()
                .find(|entry| entry.source.id == wanted)
            {
                entry.thumbnail = image;
                cx.notify();
            }
            true
        });
        if !current.unwrap_or(false) {
            return;
        }
    }
}

/// The chooser window's view: the app builds its content.
struct ScreenChooserView {
    owner: WeakEntity<QuillApp>,
    _observe: Subscription,
}

impl Render for ScreenChooserView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            window.remove_window();
            return div().into_any_element();
        };
        owner.update(cx, |app, cx| app.screen_chooser_body(cx))
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "screen_chooser_tests.rs"]
mod tests;

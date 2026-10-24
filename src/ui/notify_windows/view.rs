//! What one notification window draws: tdesktop's `Default::Notification`
//! (photo, chat name, two lines of preview, close button, Reply on hover
//! and the reply field), its `HideAllButton`, and the settings' sample
//! (`NotificationsCount::SampleWidget`).

use super::super::app::QuillApp;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::notify::custom::{MIN_HEIGHT, PHOTO_POS, PHOTO_SIZE};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// tdesktop `st::notifyTextLeft`, `st::notifyTextTop`, `st::notifyItemTop`.
const TEXT_LEFT: f32 = PHOTO_POS + PHOTO_SIZE + 12.0;
const TEXT_TOP: f32 = 7.0;

/// The message's photo beside the text ("Show photo previews"): a square
/// left of the close button, top-aligned with the chat name.
const MEDIA_SIZE: f32 = 48.0;
const MEDIA_RIGHT: f32 = 28.0;
/// Where the text ends: before the close button, or before the photo.
const TEXT_RIGHT: f32 = 28.0;
const TEXT_RIGHT_WITH_MEDIA: f32 = MEDIA_RIGHT + MEDIA_SIZE + 8.0;

/// The Quill logo for a notification that hides its sender (tdesktop
/// `hiddenUserpicPlaceholder`, the app logo).
static LOGO_PNG: &[u8] = include_bytes!("../../../assets/icons/hicolor/128x128/apps/quill.png");

#[derive(Clone)]
pub(super) enum Content {
    Message {
        title: SharedString,
        body: SharedString,
        /// The chat's picture; `None` draws initials, or the logo when
        /// the sender is hidden.
        photo: Option<PathBuf>,
        /// The message's photo, drawn beside the text.
        media: Option<PathBuf>,
        hide_sender: bool,
        can_reply: bool,
        /// The reply field opens in the window; otherwise Reply opens the
        /// chat (X11: a pop-up window there never gets the keyboard).
        reply_inline: bool,
    },
    HideAll,
    Sample,
}

/// What the window tells the manager (`super::NotifyWindows`).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ViewEvent {
    Hover(bool),
    Activate,
    Close,
    ReplyOpened,
    /// Reply where the window can't take typing: open the chat to answer.
    ReplyInApp,
    ReplySent(String),
    ReplyHeight(f32),
    HideAll,
}

pub(super) struct NotifyView {
    pub(super) id: u64,
    pub(super) content: Content,
    pub(super) opacity: f32,
    /// The Reply button is showing (tdesktop `_actionsVisible`).
    actions: bool,
    hovered: bool,
    reply: Option<Entity<TextareaState>>,
    reply_height: Rc<Cell<f32>>,
    owner: WeakEntity<QuillApp>,
}

impl NotifyView {
    pub(super) fn new(id: u64, content: Content, owner: WeakEntity<QuillApp>) -> Self {
        Self {
            id,
            content,
            opacity: 0.0,
            actions: false,
            hovered: false,
            reply: None,
            reply_height: Rc::new(Cell::new(0.0)),
            owner,
        }
    }

    /// Hand an event to the manager after this update ends, so the
    /// manager may update this view in turn.
    fn send(&self, event: ViewEvent, cx: &mut Context<Self>) {
        let owner = self.owner.clone();
        let id = self.id;
        cx.defer(move |cx| {
            // A notification acts as the main window does (its chat opens
            // there, its reply goes from there).
            let _ = owner.update(cx, |app, cx| {
                app.in_window_scope(None, cx, |app, cx| app.notify_window_event(id, event, cx))
            });
        });
    }

    fn can_reply(&self) -> bool {
        matches!(
            self.content,
            Content::Message {
                can_reply: true,
                ..
            }
        )
    }

    fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        self.actions = hovered && self.reply.is_none() && self.can_reply();
        match self.content {
            Content::Message { .. } => self.send(ViewEvent::Hover(hovered), cx),
            Content::HideAll | Content::Sample => {}
        }
        cx.notify();
    }

    fn open_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reply.is_some() || !self.can_reply() {
            return;
        }
        if !matches!(
            self.content,
            Content::Message {
                reply_inline: true,
                ..
            }
        ) {
            self.send(ViewEvent::ReplyInApp, cx);
            return;
        }
        let state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_WRITE_A_MESSAGE))
                .auto_grow(1, 5)
                .submit_on_enter(true)
        });
        cx.subscribe_in(&state, window, |this, state, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = state.read(cx).value().trim().to_string();
                if !text.is_empty() {
                    this.send(ViewEvent::ReplySent(text), cx);
                }
            }
        })
        .detach();
        state.update(cx, |input, cx| input.focus(window, cx));
        self.reply = Some(state);
        self.actions = false;
        window.activate_window();
        self.send(ViewEvent::ReplyOpened, cx);
        cx.notify();
    }

    fn send_reply(&mut self, cx: &mut Context<Self>) {
        let Some(state) = &self.reply else {
            return;
        };
        let text = state.read(cx).value().trim().to_string();
        if !text.is_empty() {
            self.send(ViewEvent::ReplySent(text), cx);
        }
    }

    fn render_message(
        &self,
        title: SharedString,
        body: SharedString,
        photo: Option<PathBuf>,
        media: Option<PathBuf>,
        hide_sender: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let media = media.filter(|_| !hide_sender);
        let text_right = if media.is_some() {
            TEXT_RIGHT_WITH_MEDIA
        } else {
            TEXT_RIGHT
        };
        let theme = cx.theme();
        let picture = if hide_sender {
            logo().into_any_element()
        } else {
            super::super::chat_row::chat_avatar(&title, photo.as_deref(), PHOTO_SIZE)
                .into_any_element()
        };
        let title = if hide_sender {
            SharedString::from(tr(keys::Q_MISC_QUILL))
        } else {
            title
        };
        div()
            .relative()
            .h(px(MIN_HEIGHT))
            .w_full()
            .flex_none()
            .child(
                div()
                    .absolute()
                    .left(px(PHOTO_POS))
                    .top(px(PHOTO_POS))
                    .child(picture),
            )
            .child(
                div()
                    .absolute()
                    .left(px(TEXT_LEFT))
                    .right(px(text_right))
                    .top(px(TEXT_TOP))
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .line_clamp(2)
                            .text_ellipsis()
                            .child(body),
                    ),
            )
            .when_some(media, |this, media| {
                this.child(
                    img(media)
                        .absolute()
                        .top(px(TEXT_TOP + 2.0))
                        .right(px(MEDIA_RIGHT))
                        .size(px(MEDIA_SIZE))
                        .rounded(px(4.0))
                        .object_fit(ObjectFit::Cover),
                )
            })
            .child(
                div().absolute().top(px(2.0)).right(px(1.0)).child(
                    Button::new("notify-close")
                        .ghost()
                        .xsmall()
                        .icon(gpui_kit::assets::IconName::X)
                        .tooltip(tr(keys::CLOSE))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.send(ViewEvent::Close, cx);
                        })),
                ),
            )
            .when(self.actions, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(PHOTO_POS))
                        .bottom(px(PHOTO_POS))
                        .child(
                            Button::new("notify-reply")
                                .small()
                                .outline()
                                .label(tr(keys::Q_MISC_REPLY))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.open_reply(window, cx);
                                })),
                        ),
                )
            })
    }

    fn render_reply(&self, state: &Entity<TextareaState>, cx: &mut Context<Self>) -> Div {
        let measured = self.reply_height.clone();
        let owner_view = cx.entity().downgrade();
        div()
            .relative()
            .w_full()
            .flex()
            .items_end()
            .gap_1()
            .px_1()
            .pb_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().child(Textarea::new(state).appearance(false)))
            .child(
                Button::new("notify-send")
                    .ghost()
                    .small()
                    .icon(gpui_kit::assets::IconName::Send)
                    .tooltip(tr(keys::Q_MISC_SEND))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.send_reply(cx);
                    })),
            )
            .child(
                // Follows the field's height so the window grows with it
                // (tdesktop `replyResized`).
                canvas(
                    move |bounds, _, cx| {
                        let height = bounds.size.height.as_f32();
                        if (measured.get() - height).abs() > 0.5 {
                            measured.set(height);
                            let view = owner_view.clone();
                            cx.defer(move |cx| {
                                let _ = view.update(cx, |view, cx| {
                                    view.send(ViewEvent::ReplyHeight(height), cx);
                                });
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
    }
}

fn logo() -> impl IntoElement {
    static LOGO: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    let image = LOGO
        .get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())))
        .clone();
    img(ImageSource::Image(image))
        .size(px(PHOTO_SIZE))
        .rounded_full()
}

impl Render for NotifyView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let root = div()
            .id(("notify-window", self.id))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .opacity(self.opacity.clamp(0.0, 1.0));
        match self.content.clone() {
            Content::Message {
                title,
                body,
                photo,
                media,
                hide_sender,
                ..
            } => {
                let message = self.render_message(title, body, photo, media, hide_sender, cx);
                let reply = self
                    .reply
                    .clone()
                    .map(|state| self.render_reply(&state, cx));
                root.cursor_pointer()
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.set_hovered(*hovered, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| this.send(ViewEvent::Close, cx)),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.reply.is_none() {
                            this.send(ViewEvent::Activate, cx);
                        }
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "escape" && this.reply.is_some() {
                            this.send(ViewEvent::Close, cx);
                        }
                    }))
                    .child(message)
                    .children(reply)
                    .into_any_element()
            }
            Content::HideAll => root
                .cursor_pointer()
                .items_center()
                .justify_center()
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .text_sm()
                .text_color(theme.primary)
                .child(tr(keys::Q_MISC_HIDE_ALL))
                .on_click(cx.listener(|this, _, _, cx| this.send(ViewEvent::HideAll, cx)))
                .into_any_element(),
            Content::Sample => root
                .child(self.render_message(
                    tr(keys::Q_MISC_QUILL).into(),
                    tr(keys::NOTIFICATION_SAMPLE).into(),
                    None,
                    None,
                    true,
                    cx,
                ))
                .into_any_element(),
        }
    }
}

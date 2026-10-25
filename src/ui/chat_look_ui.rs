//! A chat's own look: the emoji chat theme and the chat wallpaper, picked in
//! one dialog with a live preview (tdesktop `ui/chat/choose_theme_controller`
//! and `boxes/background_preview_box`). Account-wide previews (gallery,
//! `bg/` links, files) are in `wallpaper_preview`.
//!
//! tdesktop offers a chat theme or wallpaper in private chats (`addThemeEdit`
//! in `window_peer_menu.cpp`); groups and channels need boosts there and are
//! not covered. The wallpaper is set for the user only unless the account is
//! Premium and chooses "also for the other person"
//! (`setChatBackground.only_for_self`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::bubble_theme::{OutgoingFill, outgoing_bubble_color};
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use super::wallpaper::{paint_wallpaper, session_wallpaper};
use super::wallpaper_cache::Slot;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod apply;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod apply_tests;
mod demo;

/// What the dialog is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LookTarget {
    /// Theme and wallpaper of one chat.
    Chat(i64),
}

/// The dialog's pending choices; nothing is sent until Apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChatLookDialog {
    pub target: LookTarget,
    /// The emoji theme that will be applied (`None` = no theme).
    pub theme: Option<String>,
    /// An installed background picked for the chat.
    pub background: Option<i64>,
    /// An image file picked for the chat (tdesktop `uploadForPeer`).
    pub file: Option<PathBuf>,
    /// Dark-mode dimming of a photo wallpaper, 0-100
    /// (`setChatBackground.dark_theme_dimming`).
    pub dimming: i32,
    /// The dimming slider (`None` in tests).
    pub dimming_slider: Option<Entity<SliderState>>,
    /// Remove the chat's own wallpaper.
    pub remove_wallpaper: bool,
    /// Also set the wallpaper for the other person (Premium only).
    pub both: bool,
    /// Requests Apply has sent and is waiting on; the dialog stays open
    /// until they are acknowledged, and shows the error if one fails.
    pub awaiting: Option<LookWait>,
}

/// Apply's in-flight requests: `sent` of them, counted from the session's
/// acknowledgement counter `base`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LookWait {
    pub base: u32,
    pub sent: u32,
}

/// Where an Apply stands, from the session's counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LookProgress {
    Waiting,
    Done,
    Failed(String),
}

/// Whether Apply's requests are acknowledged, still pending or failed. An
/// error wins over acknowledgements: the dialog stays open to show it.
pub(super) fn look_progress(wait: LookWait, oks: u32, error: Option<&str>) -> LookProgress {
    if let Some(error) = error {
        return LookProgress::Failed(error.to_owned());
    }
    if oks.wrapping_sub(wait.base) >= wait.sent {
        LookProgress::Done
    } else {
        LookProgress::Waiting
    }
}

/// One change `Apply` sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LookChange {
    /// `setChatTheme` (an empty name removes the theme).
    Theme(String),
    /// `setChatBackground` with an installed background (or the chat's own,
    /// for a new dimming).
    Wallpaper { id: i64, dimming: i32 },
    /// `setChatBackground` with an image file.
    WallpaperFile { path: PathBuf, dimming: i32 },
    /// `deleteChatBackground`.
    RemoveWallpaper,
}

/// Whether a chat can have its own theme and wallpaper here: a private chat
/// with another user.
pub(super) fn chat_look_allowed(is_private: bool, is_self: bool) -> bool {
    is_private && !is_self
}

/// The requests that turn the chat's current look into the pending one.
/// `own` is the chat's own wallpaper and its dimming.
pub(super) fn plan_changes(
    current_theme: Option<&str>,
    own: Option<(i64, i32)>,
    pending: &ChatLookDialog,
) -> Vec<LookChange> {
    let mut changes = Vec::new();
    if pending.theme.as_deref() != current_theme {
        changes.push(LookChange::Theme(pending.theme.clone().unwrap_or_default()));
    }
    let dimming = pending.dimming.clamp(0, 100);
    if let Some(path) = &pending.file {
        changes.push(LookChange::WallpaperFile {
            path: path.clone(),
            dimming,
        });
    } else if let Some(id) = pending.background {
        changes.push(LookChange::Wallpaper { id, dimming });
    } else if pending.remove_wallpaper && own.is_some() {
        changes.push(LookChange::RemoveWallpaper);
    } else if let Some((id, current)) = own
        && current != dimming
    {
        changes.push(LookChange::Wallpaper { id, dimming });
    }
    changes
}

/// The chat's own wallpaper id and dimming.
pub(super) fn own_wallpaper(session: &quill::state::Session, chat_id: i64) -> Option<(i64, i32)> {
    session
        .chats_state
        .chat_backgrounds
        .get(&chat_id)
        .map(|b| (b.background.id, b.dark_theme_dimming))
}

/// Image types Telegram accepts as a wallpaper file (a photo).
pub(super) fn is_wallpaper_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| matches!(e.as_str(), "jpg" | "jpeg" | "png" | "webp"))
}

impl QuillApp {
    /// Open the picker for a private chat.
    pub(super) fn open_chat_look_dialog(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let is_private = session.private_chat_user_id(ChatId(chat_id));
        let is_self = is_private.is_some() && is_private == session.my_user_id;
        if !chat_look_allowed(is_private.is_some(), is_self) && !self.boosted_look_allowed(chat_id)
        {
            return;
        }
        let theme = session.chats_state.chat_theme_names.get(&chat_id).cloned();
        let dimming = session
            .chats_state
            .chat_backgrounds
            .get(&chat_id)
            .map_or(0, |b| b.dark_theme_dimming);
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(dimming as f32)
        });
        cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            if let Some(d) = this.dialogs.chat_look_dialog.as_mut() {
                d.dimming = value.end().round() as i32;
            }
            cx.notify();
        })
        .detach();
        self.dialogs.chat_look_dialog = Some(ChatLookDialog {
            target: LookTarget::Chat(chat_id),
            theme,
            background: None,
            file: None,
            dimming,
            dimming_slider: Some(slider),
            remove_wallpaper: false,
            both: false,
            awaiting: None,
        });
        self.ensure_wallpapers_loaded(cx);
        let boosted = self.is_boosted_look(chat_id);
        if let Some(live) = self.live.as_mut() {
            live.driver.download_chat_look_files(chat_id, true);
            if boosted {
                let _ = live.driver.refresh_chat_boost_level(ChatId(chat_id));
            }
        }
        cx.notify();
    }

    /// "From file…" in the chat colors dialog: the picked image previews
    /// and is uploaded for this chat on Apply.
    fn choose_chat_wallpaper_file(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_MISC_CHOOSE_WALLPAPER).into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    if !is_wallpaper_image(&path) {
                        this.connection
                            .set_note(StatusNote::silent(tr(keys::Q_MISC_CHOOSE_A_JPEG_PNG_OR)));
                    } else if let Some(d) = this.dialogs.chat_look_dialog.as_mut() {
                        d.file = Some(path);
                        d.background = None;
                        d.remove_wallpaper = false;
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn close_chat_look_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.chat_look_dialog = None;
        if let Some(session) = match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        } {
            session.chats_state.background_error = None;
        }
        cx.notify();
    }

    /// Send (or, in the screenshot demo, apply) the pending changes.
    fn apply_chat_look(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.chat_look_dialog.clone() else {
            return;
        };
        let LookTarget::Chat(chat_id) = dialog.target;
        let Some(session) = self.session() else {
            return;
        };
        let current_theme = session.chats_state.chat_theme_names.get(&chat_id).cloned();
        let own = own_wallpaper(session, chat_id);
        let changes = plan_changes(current_theme.as_deref(), own, &dialog);
        let boosted = self.is_boosted_look(chat_id);
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        // Boosted chats set the wallpaper for everyone, never for self only.
        let only_for_self = !boosted && !(dialog.both && premium);
        if dialog.awaiting.is_some() || (boosted && self.boosted_look_blocked(chat_id, &changes)) {
            cx.notify();
            return;
        }
        match self.live.as_mut() {
            Some(live) => {
                if changes.is_empty() {
                    self.close_chat_look_dialog(cx);
                    return;
                }
                live.driver.session.chats_state.background_error = None;
                let base = live.driver.session.chats_state.chat_look_oks;
                let mut sent = 0u32;
                for change in changes {
                    let result =
                        match change {
                            LookChange::Theme(name) => {
                                live.driver.set_chat_look_theme(ChatId(chat_id), &name)
                            }
                            LookChange::Wallpaper { id, dimming } => live
                                .driver
                                .set_chat_background(ChatId(chat_id), id, dimming, only_for_self),
                            LookChange::WallpaperFile { path, dimming } => {
                                live.driver.set_chat_background_local(
                                    ChatId(chat_id),
                                    &path.to_string_lossy(),
                                    false,
                                    dimming,
                                    only_for_self,
                                )
                            }
                            LookChange::RemoveWallpaper => {
                                live.driver.delete_chat_background(ChatId(chat_id), false)
                            }
                        };
                    match result {
                        Ok(_) => sent += 1,
                        Err(err) => {
                            live.driver.session.chats_state.background_error =
                                Some(user_error(&err));
                        }
                    }
                }
                if let Some(d) = self.dialogs.chat_look_dialog.as_mut() {
                    d.awaiting = (sent > 0).then_some(LookWait { base, sent });
                }
                // Nothing in flight and no error: nothing to wait for.
                if sent == 0 && live.driver.session.chats_state.background_error.is_none() {
                    self.close_chat_look_dialog(cx);
                    return;
                }
                cx.notify();
                return;
            }
            None => {
                // Screenshot demo: apply locally.
                if let Some(session) = self.demo_session.as_mut() {
                    for change in changes {
                        match change {
                            LookChange::Theme(name) => session
                                .set_chat_theme_name(chat_id, (!name.is_empty()).then_some(name)),
                            LookChange::WallpaperFile { .. } => {}
                            LookChange::Wallpaper { id, dimming } => {
                                let found = session
                                    .chats_state
                                    .installed_backgrounds
                                    .iter()
                                    .flatten()
                                    .find(|b| b.id == id)
                                    .cloned();
                                if let Some(background) = found {
                                    session.set_chat_background(
                                        chat_id,
                                        Some(quill::telegram::envelope::ChatBackground {
                                            background,
                                            dark_theme_dimming: dimming,
                                        }),
                                    );
                                }
                            }
                            LookChange::RemoveWallpaper => {
                                session.set_chat_background(chat_id, None)
                            }
                        }
                    }
                }
            }
        }
        self.close_chat_look_dialog(cx);
    }

    /// Poll-loop half of Apply: close the dialog once its requests are
    /// acknowledged; on a failure keep it open (the error shows inline).
    pub(super) fn drain_chat_look(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(wait) = self
            .dialogs
            .chat_look_dialog
            .as_ref()
            .and_then(|d| d.awaiting)
        else {
            return false;
        };
        let Some(session) = self.session() else {
            return false;
        };
        match look_progress(
            wait,
            session.chats_state.chat_look_oks,
            session.chats_state.background_error.as_deref(),
        ) {
            LookProgress::Waiting => false,
            LookProgress::Done => {
                self.close_chat_look_dialog(cx);
                true
            }
            LookProgress::Failed(_) => {
                if let Some(d) = self.dialogs.chat_look_dialog.as_mut() {
                    d.awaiting = None;
                }
                cx.notify();
                true
            }
        }
    }

    /// A miniature conversation showing the look being picked.
    pub(super) fn chat_look_preview(
        &self,
        wallpaper: Option<&super::wallpaper::Wallpaper>,
        dimming: u8,
        outgoing: Option<OutgoingFill>,
        cx: &App,
    ) -> AnyElement {
        use super::chat_theme::{bg_bubble_incoming, text_bright, text_on_fill};
        // The outgoing bubble sits low in the preview: its slice of a
        // gradient fill runs from about 60% to 85% down.
        let outgoing_bg: Background = match outgoing {
            Some(fill) if fill.is_gradient() => linear_gradient(
                180.,
                linear_color_stop(rgb(fill.color_at(0.6)), 0.),
                linear_color_stop(rgb(fill.color_at(0.85)), 1.),
            ),
            other => outgoing_bubble_color(other).into(),
        };
        let outgoing_text = outgoing.map_or_else(|| Hsla::from(text_on_fill()), |f| f.text());
        let bubble = |text: &'static str| div().px_3().py_1p5().rounded_xl().text_sm().child(text);
        paint_wallpaper(
            div()
                .id("chat-look-preview")
                .relative()
                .w_full()
                .h(px(168.))
                .rounded_lg()
                .overflow_hidden()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .aria_label(tr(keys::Q_MISC_CHAT_PREVIEW)),
            wallpaper,
            dimming,
            Slot::Preview,
        )
        .child(
            div()
                .absolute()
                .size_full()
                .flex()
                .flex_col()
                .justify_end()
                .gap_2()
                .p_3()
                .child(
                    div().flex().child(
                        bubble(tr(keys::Q_MISC_GOOD_MORNING_WHICH_LOOK_DO))
                            .bg(bg_bubble_incoming())
                            .text_color(text_bright()),
                    ),
                )
                .child(
                    div().flex().justify_end().child(
                        bubble(tr(keys::Q_MISC_THIS_ONE_I_THINK))
                            .bg(outgoing_bg)
                            .text_color(outgoing_text),
                    ),
                ),
        )
        .into_any_element()
    }

    fn chat_look_theme_row(&self, dialog: &ChatLookDialog, cx: &mut Context<Self>) -> AnyElement {
        let dark = cx.theme().is_dark();
        let muted = cx.theme().muted_foreground;
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let none_selected = dialog.theme.is_none();
        let mut row = div().flex().flex_wrap().gap_2().child(
            div()
                .id("chat-look-theme-none")
                .w(px(52.))
                .h(px(52.))
                .rounded_lg()
                .border_2()
                .border_color(if none_selected {
                    cx.theme().primary
                } else {
                    cx.theme().border
                })
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .role(gpui_kit::Role::Button)
                .aria_label(tr(keys::Q_MISC_NO_CHAT_THEME))
                .tab_index(0)
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(d) = this.dialogs.chat_look_dialog.as_mut() {
                        d.theme = None;
                    }
                    cx.notify();
                }))
                .child(tr(keys::Q_MISC_NONE)),
        );
        if session.chats_state.emoji_chat_themes.is_empty() {
            return div()
                .flex()
                .flex_col()
                .gap_1()
                .child(row)
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(keys::Q_MISC_CHAT_THEMES_LOAD_FROM_TELEGRAM)),
                )
                .into_any_element();
        }
        for (index, theme) in session.chats_state.emoji_chat_themes.iter().enumerate() {
            let selected = dialog.theme.as_deref() == Some(theme.name.as_str());
            let settings = theme.settings(dark);
            let name = theme.name.clone();
            row = row.child(
                div()
                    .id(("chat-look-theme", index as u64))
                    .w(px(52.))
                    .h(px(52.))
                    .rounded_lg()
                    .border_2()
                    .border_color(if selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .bg(rgb(OutgoingFill::from_theme(settings, dark).solid()).opacity(0.22))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_0p5()
                    .role(gpui_kit::Role::Button)
                    .aria_label(tr_with(
                        keys::Q_MISC_CHAT_THEME,
                        &[("name", &theme.name.to_string())],
                    ))
                    .tab_index(0)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.dialogs.chat_look_dialog.as_mut() {
                            d.theme = Some(name.clone());
                        }
                        cx.notify();
                    }))
                    .child(div().text_xl().child(theme.name.clone()))
                    .child(
                        div()
                            .w(px(18.))
                            .h(px(4.))
                            .rounded_full()
                            .bg(rgb(OutgoingFill::from_theme(settings, dark).solid())),
                    ),
            );
        }
        row.into_any_element()
    }

    fn chat_look_wallpaper_grid(
        &self,
        dialog: &ChatLookDialog,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let Some(list) = self
            .session()
            .and_then(|s| s.chats_state.installed_backgrounds.clone())
        else {
            return div()
                .text_xs()
                .text_color(muted)
                .child(tr(keys::Q_MISC_LOADING_WALLPAPERS))
                .into_any_element();
        };
        if list.is_empty() {
            return div()
                .text_xs()
                .text_color(muted)
                .child(tr(keys::Q_MISC_NO_WALLPAPERS_INSTALLED_ON_THIS))
                .into_any_element();
        }
        let mut grid = div().flex().flex_wrap().gap_2();
        for background in &list {
            let id = background.id;
            grid = grid.child(
                self.wallpaper_tile(
                    ("chat-look-wallpaper", id as u64),
                    background,
                    dialog.background == Some(id),
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    // The grid shows thumbnails; the preview needs the photo.
                    if let Some(live) = this.live.as_mut()
                        && let Some(bg) = live
                            .driver
                            .session
                            .chats_state
                            .installed_backgrounds
                            .iter()
                            .flatten()
                            .find(|b| b.id == id)
                            .cloned()
                    {
                        live.driver.download_background_file(&bg);
                    }
                    if let Some(d) = this.dialogs.chat_look_dialog.as_mut() {
                        d.background = Some(id);
                        d.file = None;
                        d.remove_wallpaper = false;
                    }
                    cx.notify();
                })),
            );
        }
        grid.into_any_element()
    }

    pub(super) fn build_chat_look_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ChatLook, |this, _, cx| {
                this.close_chat_look_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some(state) = this.dialogs.chat_look_dialog.clone() else {
                return dialog
                    .overlay(true)
                    .title(super::shell::dialog_title(tr(keys::Q_MISC_CHAT_LOOK)))
                    .on_close(on_close);
            };
            let dark = cx.theme().is_dark();
            let (title, body, footer) = match &state.target {
                LookTarget::Chat(chat_id) => {
                    let chat_id = *chat_id;
                    let session = this.session();
                    let own = session.and_then(|s| own_wallpaper(s, chat_id));
                    let has_own = own.is_some();
                    // The preview: pending wallpaper, else the chat's own,
                    // else the pending theme's, else the account's.
                    let pending_theme = session.and_then(|s| {
                        state.theme.as_ref().and_then(|n| {
                            s.chats_state
                                .emoji_chat_themes
                                .iter()
                                .find(|t| &t.name == n)
                        })
                    });
                    let outgoing_fill =
                        pending_theme.map(|t| OutgoingFill::from_theme(t.settings(dark), dark));
                    let mut wallpaper = None;
                    if let Some(path) = &state.file {
                        wallpaper = Some(super::wallpaper::Wallpaper::Image {
                            path: path.clone(),
                            blurred: false,
                            tiled: false,
                        });
                    } else if let Some(id) = state.background {
                        wallpaper = session.and_then(|s| {
                            let bg = s
                                .chats_state
                                .installed_backgrounds
                                .iter()
                                .flatten()
                                .find(|b| b.id == id)?;
                            session_wallpaper(s, bg)
                        });
                    } else if !state.remove_wallpaper
                        && let Some(s) = session
                        && let Some(bg) = s
                            .chats_state
                            .chat_backgrounds
                            .get(&chat_id)
                            .map(|c| &c.background)
                    {
                        wallpaper = session_wallpaper(s, bg);
                    }
                    // tdesktop dims photo wallpapers only, in dark mode.
                    let photo =
                        matches!(wallpaper, Some(super::wallpaper::Wallpaper::Image { .. }));
                    let dimming = if dark && photo {
                        state.dimming.clamp(0, 100) as u8
                    } else {
                        0
                    };
                    if wallpaper.is_none() {
                        wallpaper = pending_theme
                            .and_then(|t| t.settings(dark).background.as_ref())
                            .and_then(|bg| session.and_then(|s| session_wallpaper(s, bg)))
                            .or_else(|| this.current_wallpaper(cx));
                    }
                    let mut body = div().flex().flex_col().gap_3();
                    body = body.child(this.chat_look_preview(
                        wallpaper.as_ref(),
                        dimming,
                        outgoing_fill,
                        cx,
                    ));
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_semibold()
                                    .text_sm()
                                    .child(tr(keys::Q_MISC_THEME)),
                            )
                            .child(this.chat_look_theme_row(&state, cx)),
                    );
                    let mut wall = div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .font_semibold()
                                .text_sm()
                                .child(tr(keys::Q_MISC_WALLPAPER)),
                        )
                        .child(this.chat_look_wallpaper_grid(&state, cx))
                        .child(
                            div().flex().child(
                                Button::new("chat-look-file")
                                    .label(tr(keys::Q_MISC_FROM_FILE))
                                    .small()
                                    .tooltip(tr(keys::Q_MISC_USE_A_JPEG_PNG_OR))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.choose_chat_wallpaper_file(cx)
                                    })),
                            ),
                        );
                    if dark
                        && photo
                        && let Some(slider) = &state.dimming_slider
                    {
                        wall = wall.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_sm().child(tr_with(
                                    keys::Q_LEFTOVERS_WALLPAPER_DIMMING,
                                    &[("percent", &(state.dimming.clamp(0, 100)).to_string())],
                                )))
                                .child(Slider::new(slider).bg(cx.theme().primary)),
                        );
                    }
                    if has_own {
                        wall = wall.child(
                            action_row(
                                "chat-look-remove-wallpaper",
                                Some(gpui_kit::assets::IconName::Trash),
                                if state.remove_wallpaper {
                                    tr(keys::Q_MISC_WALLPAPER_WILL_BE_REMOVED)
                                } else {
                                    tr(keys::Q_MISC_REMOVE_THIS_CHATS_WALLPAPER)
                                },
                                true,
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = this.dialogs.chat_look_dialog.as_mut() {
                                    d.remove_wallpaper = !d.remove_wallpaper;
                                    d.background = None;
                                    d.file = None;
                                }
                                cx.notify();
                            })),
                        );
                    }
                    if let Some(error) =
                        session.and_then(|s| s.chats_state.background_error.clone())
                    {
                        wall = wall.child(
                            div()
                                .id("chat-look-error")
                                .text_sm()
                                .text_color(super::danger_dark())
                                .child(tr_with(
                                    keys::Q_MISC_COULDNT_CHANGE_THE_CHAT_LOOK,
                                    &[("error", &error.to_string())],
                                )),
                        );
                    }
                    let applying = state.awaiting.is_some();
                    let changed = {
                        let current_theme = this
                            .session()
                            .and_then(|s| s.chats_state.chat_theme_names.get(&chat_id).cloned());
                        !plan_changes(current_theme.as_deref(), own, &state).is_empty()
                    };
                    let footer = this.chat_look_footer(
                        apply::FooterState {
                            chat_id,
                            changed,
                            applying,
                        },
                        &state,
                        cx,
                    );
                    (
                        tr(keys::Q_MISC_CHAT_COLORS_AND_WALLPAPER),
                        body.child(wall).into_any_element(),
                        footer,
                    )
                }
            };
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .width(px(440.))
                .title(super::shell::dialog_title(title))
                .content(super::shell::scrollable_dialog_content(
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    },
                ))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    ChatLook => DialogSpec::new(
        2100,
        |app| app.dialogs.chat_look_dialog.is_some(),
        QuillApp::build_chat_look_dialog,
    ),
}

#[cfg(test)]
mod tests {
    use super::{
        ChatLookDialog, LookChange, LookProgress, LookTarget, LookWait, chat_look_allowed,
        is_wallpaper_image, look_progress, plan_changes,
    };
    use std::path::Path;

    fn dialog() -> ChatLookDialog {
        ChatLookDialog {
            target: LookTarget::Chat(1),
            theme: None,
            background: None,
            file: None,
            dimming: 0,
            dimming_slider: None,
            remove_wallpaper: false,
            both: false,
            awaiting: None,
        }
    }

    #[test]
    fn only_private_chats_with_someone_else_get_a_look() {
        assert!(chat_look_allowed(true, false));
        assert!(!chat_look_allowed(true, true), "Saved Messages");
        assert!(!chat_look_allowed(false, false), "groups and channels");
    }

    #[test]
    fn unchanged_choices_send_nothing() {
        assert!(plan_changes(None, None, &dialog()).is_empty());
        let mut same = dialog();
        same.theme = Some("🏠".into());
        same.dimming = 30;
        assert!(plan_changes(Some("🏠"), Some((5, 30)), &same).is_empty());
    }

    #[test]
    fn theme_and_wallpaper_changes_are_planned_separately() {
        let mut pending = dialog();
        pending.theme = Some("🌷".into());
        pending.background = Some(9);
        assert_eq!(
            plan_changes(None, None, &pending),
            vec![
                LookChange::Theme("🌷".into()),
                LookChange::Wallpaper { id: 9, dimming: 0 }
            ]
        );
        // Dropping the theme sends Telegram's empty name.
        let mut remove = dialog();
        remove.remove_wallpaper = true;
        assert_eq!(
            plan_changes(Some("🌷"), Some((5, 0)), &remove),
            vec![
                LookChange::Theme(String::new()),
                LookChange::RemoveWallpaper
            ]
        );
        // Nothing to remove: no request.
        assert!(plan_changes(None, None, &remove).is_empty());
    }

    #[test]
    fn a_file_or_a_new_dimming_sets_the_chat_wallpaper() {
        let mut file = dialog();
        file.file = Some("/a/b.jpg".into());
        file.dimming = 140;
        assert_eq!(
            plan_changes(None, Some((5, 0)), &file),
            vec![LookChange::WallpaperFile {
                path: "/a/b.jpg".into(),
                dimming: 100
            }]
        );
        // Only the dimming of the chat's own wallpaper changed.
        let mut dim = dialog();
        dim.dimming = 40;
        assert_eq!(
            plan_changes(None, Some((5, 10)), &dim),
            vec![LookChange::Wallpaper { id: 5, dimming: 40 }]
        );
        assert!(plan_changes(None, None, &dim).is_empty(), "nothing to dim");
    }

    #[test]
    fn wallpaper_files_are_photos() {
        assert!(is_wallpaper_image(Path::new("/a/b.JPG")));
        assert!(is_wallpaper_image(Path::new("c.webp")));
        assert!(!is_wallpaper_image(Path::new("c.gif")));
        assert!(!is_wallpaper_image(Path::new("noext")));
    }

    #[test]
    fn apply_waits_for_every_acknowledgement() {
        let wait = LookWait { base: 4, sent: 2 };
        assert_eq!(look_progress(wait, 4, None), LookProgress::Waiting);
        assert_eq!(look_progress(wait, 5, None), LookProgress::Waiting);
        assert_eq!(look_progress(wait, 6, None), LookProgress::Done);
    }

    #[test]
    fn a_failure_keeps_the_dialog_open_with_the_reason() {
        let wait = LookWait { base: 0, sent: 1 };
        assert_eq!(
            look_progress(wait, 0, Some("BACKGROUND_INVALID")),
            LookProgress::Failed("BACKGROUND_INVALID".into())
        );
        assert_eq!(
            look_progress(wait, 1, Some("CHAT_NOT_MODIFIED")),
            LookProgress::Failed("CHAT_NOT_MODIFIED".into())
        );
    }
}

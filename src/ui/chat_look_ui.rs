//! A chat's own look: the emoji chat theme and the chat wallpaper, picked in
//! one dialog with a live preview (tdesktop `ui/chat/choose_theme_controller`
//! and `boxes/background_preview_box`). The same dialog previews a `bg/`
//! link and the "wallpaper from a file" picker lives here too.
//!
//! tdesktop offers a chat theme or wallpaper in private chats (`addThemeEdit`
//! in `window_peer_menu.cpp`); groups and channels need boosts there and are
//! not covered. The wallpaper is set for the user only unless the account is
//! Premium and chooses "also for the other person"
//! (`setChatBackground.only_for_self`).

use super::app::QuillApp;
use super::pressable::action_row;
use super::shell::{DialogKind, QuillShell};
use super::synthetic::fill_is_light;
use super::wallpaper::{paint_wallpaper, session_wallpaper};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// What the dialog is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LookTarget {
    /// Theme and wallpaper of one chat.
    Chat(i64),
    /// Preview of a `bg/` link (`searchBackground`), applied account-wide.
    Link { name: String },
}

/// The dialog's pending choices; nothing is sent until Apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ChatLookDialog {
    pub target: LookTarget,
    /// The emoji theme that will be applied (`None` = no theme).
    pub theme: Option<String>,
    /// An installed background picked for the chat.
    pub background: Option<i64>,
    /// Remove the chat's own wallpaper.
    pub remove_wallpaper: bool,
    /// Also set the wallpaper for the other person (Premium only).
    pub both: bool,
}

/// One change `Apply` sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LookChange {
    /// `setChatTheme` (an empty name removes the theme).
    Theme(String),
    /// `setChatBackground` with an installed background.
    Wallpaper(i64),
    /// `deleteChatBackground`.
    RemoveWallpaper,
}

/// Whether a chat can have its own theme and wallpaper here: a private chat
/// with another user.
pub(super) fn chat_look_allowed(is_private: bool, is_self: bool) -> bool {
    is_private && !is_self
}

/// The requests that turn the chat's current look into the pending one.
pub(super) fn plan_changes(
    current_theme: Option<&str>,
    has_own_wallpaper: bool,
    pending: &ChatLookDialog,
) -> Vec<LookChange> {
    let mut changes = Vec::new();
    if pending.theme.as_deref() != current_theme {
        changes.push(LookChange::Theme(pending.theme.clone().unwrap_or_default()));
    }
    if let Some(id) = pending.background {
        changes.push(LookChange::Wallpaper(id));
    } else if pending.remove_wallpaper && has_own_wallpaper {
        changes.push(LookChange::RemoveWallpaper);
    }
    changes
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
        if !chat_look_allowed(is_private.is_some(), is_self) {
            return;
        }
        let theme = session.chat_theme_names.get(&chat_id).cloned();
        self.chat_look_dialog = Some(ChatLookDialog {
            target: LookTarget::Chat(chat_id),
            theme,
            background: None,
            remove_wallpaper: false,
            both: false,
        });
        self.ensure_wallpapers_loaded(cx);
        if let Some(live) = self.live.as_mut() {
            live.driver.download_chat_look_files(chat_id, true);
        }
        cx.notify();
    }

    /// `bg/<name>` links: search the background and preview it.
    pub(super) fn open_background_link(&mut self, name: String, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                live.driver.session.background_error = None;
                if live.driver.search_background(&name).is_err() {
                    self.status_note = "could not open the wallpaper link".into();
                    return;
                }
            }
            None => {}
        }
        self.chat_look_dialog = Some(ChatLookDialog {
            target: LookTarget::Link { name },
            theme: None,
            background: None,
            remove_wallpaper: false,
            both: false,
        });
        cx.notify();
    }

    pub(super) fn close_chat_look_dialog(&mut self, cx: &mut Context<Self>) {
        self.chat_look_dialog = None;
        if let Some(session) = match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_mut(),
        } {
            session.searched_background = None;
            session.background_error = None;
        }
        cx.notify();
    }

    /// Send (or, in the screenshot demo, apply) the pending changes.
    fn apply_chat_look(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.chat_look_dialog.clone() else {
            return;
        };
        let LookTarget::Chat(chat_id) = dialog.target else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let current_theme = session.chat_theme_names.get(&chat_id).cloned();
        let has_own = session.chat_backgrounds.contains_key(&chat_id);
        let changes = plan_changes(current_theme.as_deref(), has_own, &dialog);
        let premium = session.my_is_premium();
        let only_for_self = !(dialog.both && premium);
        match self.live.as_mut() {
            Some(live) => {
                for change in changes {
                    let sent = match change {
                        LookChange::Theme(name) => {
                            live.driver.set_chat_theme(ChatId(chat_id), &name)
                        }
                        LookChange::Wallpaper(id) => {
                            live.driver
                                .set_chat_background(ChatId(chat_id), id, 0, only_for_self)
                        }
                        LookChange::RemoveWallpaper => {
                            live.driver.delete_chat_background(ChatId(chat_id), false)
                        }
                    };
                    if let Err(err) = sent {
                        self.status_note = format!("could not change the chat look: {err:?}");
                    }
                }
            }
            None => {
                // Screenshot demo: apply locally.
                if let Some(session) = self.demo_session.as_mut() {
                    for change in changes {
                        match change {
                            LookChange::Theme(name) => session
                                .set_chat_theme_name(chat_id, (!name.is_empty()).then_some(name)),
                            LookChange::Wallpaper(id) => {
                                let found = session
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
                                            dark_theme_dimming: 0,
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

    /// Apply the previewed `bg/` link as the account wallpaper for the
    /// current theme.
    fn apply_background_link(&mut self, cx: &mut Context<Self>) {
        let Some(background) = self.session().and_then(|s| s.searched_background.clone()) else {
            return;
        };
        let dark = cx.theme().is_dark();
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.set_default_background(background.id, dark) {
                    self.status_note = format!("could not set the wallpaper: {err:?}");
                    return;
                }
            }
            None => {
                if let Some(session) = self.demo_session.as_mut() {
                    session.default_backgrounds.insert(dark, background);
                }
            }
        }
        self.set_appearance(cx, |a| {
            a.telegram_wallpaper = true;
            a.wallpaper_rgb = None;
        });
        self.close_chat_look_dialog(cx);
    }

    /// "From file…": pick an image and make it the account wallpaper for
    /// the current theme (`inputBackgroundLocal`).
    pub(super) fn choose_wallpaper_file(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose Wallpaper".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picker.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.set_wallpaper_from_file(&path, cx));
            }
        })
        .detach();
    }

    pub(super) fn set_wallpaper_from_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        if !is_wallpaper_image(path) {
            self.status_note = "choose a JPEG, PNG or WebP image for the wallpaper".into();
            cx.notify();
            return;
        }
        let dark = cx.theme().is_dark();
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if let Err(err) = live
            .driver
            .set_default_background_local(&path.to_string_lossy(), dark)
        {
            self.status_note = format!("could not set the wallpaper: {err:?}");
            cx.notify();
            return;
        }
        self.set_appearance(cx, |a| {
            a.telegram_wallpaper = true;
            a.wallpaper_rgb = None;
        });
    }

    /// A miniature conversation showing the look being picked.
    fn chat_look_preview(
        &self,
        wallpaper: Option<&super::wallpaper::Wallpaper>,
        dimming: u8,
        outgoing_fill: Option<u32>,
        cx: &App,
    ) -> AnyElement {
        use super::chat_theme::{accent_strong, bg_bubble_incoming, text_bright, text_on_fill};
        let outgoing_bg = outgoing_fill.map_or_else(accent_strong, rgb);
        let outgoing_text = match outgoing_fill {
            Some(fill) if fill_is_light(fill) => Hsla::from(rgb(0x1a1a1a)),
            _ => Hsla::from(text_on_fill()),
        };
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
                .aria_label("Chat preview"),
            wallpaper,
            dimming,
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
                        bubble("Good morning! Which look do you like?")
                            .bg(bg_bubble_incoming())
                            .text_color(text_bright()),
                    ),
                )
                .child(
                    div().flex().justify_end().child(
                        bubble("This one, I think.")
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
                .aria_label("No chat theme")
                .tab_index(0)
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(d) = this.chat_look_dialog.as_mut() {
                        d.theme = None;
                    }
                    cx.notify();
                }))
                .child("None"),
        );
        if session.emoji_chat_themes.is_empty() {
            return div()
                .flex()
                .flex_col()
                .gap_1()
                .child(row)
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Chat themes load from Telegram after sign-in."),
                )
                .into_any_element();
        }
        for (index, theme) in session.emoji_chat_themes.iter().enumerate() {
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
                    .bg(rgb(settings.outgoing_bubble_color()).opacity(0.22))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_0p5()
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Chat theme {}", theme.name))
                    .tab_index(0)
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.chat_look_dialog.as_mut() {
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
                            .bg(rgb(settings.outgoing_bubble_color())),
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
        let Some(list) = self.session().and_then(|s| s.installed_backgrounds.clone()) else {
            return div()
                .text_xs()
                .text_color(muted)
                .child("Loading wallpapers…")
                .into_any_element();
        };
        if list.is_empty() {
            return div()
                .text_xs()
                .text_color(muted)
                .child("No wallpapers installed on this account.")
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
                    if let Some(d) = this.chat_look_dialog.as_mut() {
                        d.background = Some(id);
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
            let Some(state) = this.chat_look_dialog.clone() else {
                return dialog
                    .overlay(true)
                    .title(super::shell::dialog_title("Chat look"))
                    .on_close(on_close);
            };
            let dark = cx.theme().is_dark();
            let muted = cx.theme().muted_foreground;
            let (title, body, footer) = match &state.target {
                LookTarget::Chat(chat_id) => {
                    let chat_id = *chat_id;
                    let session = this.session();
                    let name = session
                        .and_then(|s| s.chats.get(&chat_id))
                        .map(|c| c.title.clone())
                        .unwrap_or_default();
                    let premium = session.is_some_and(|s| s.my_is_premium());
                    let has_own = session.is_some_and(|s| s.chat_backgrounds.contains_key(&chat_id));
                    // The preview: pending wallpaper, else the chat's own,
                    // else the pending theme's, else the account's.
                    let pending_theme = session.and_then(|s| {
                        state.theme.as_ref().and_then(|n| {
                            s.emoji_chat_themes.iter().find(|t| &t.name == n)
                        })
                    });
                    let outgoing_fill = pending_theme.map(|t| t.settings(dark).outgoing_bubble_color());
                    let mut wallpaper = None;
                    let mut dimming = 0;
                    if let Some(id) = state.background {
                        wallpaper = session.and_then(|s| {
                            let bg = s.installed_backgrounds.iter().flatten().find(|b| b.id == id)?;
                            session_wallpaper(s, bg)
                        });
                    } else if !state.remove_wallpaper
                        && let Some(s) = session
                        && let Some((bg, dim)) = s.chat_backgrounds.get(&chat_id).map(|c| (&c.background, c.dark_theme_dimming))
                    {
                        wallpaper = session_wallpaper(s, bg);
                        dimming = if dark { dim.clamp(0, 100) as u8 } else { 0 };
                    }
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
                            .child(div().font_semibold().text_sm().child("Theme"))
                            .child(this.chat_look_theme_row(&state, cx)),
                    );
                    let mut wall = div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().font_semibold().text_sm().child("Wallpaper"))
                        .child(this.chat_look_wallpaper_grid(&state, cx));
                    if has_own {
                        wall = wall.child(
                            action_row(
                                "chat-look-remove-wallpaper",
                                Some(gpui_kit::assets::IconName::Trash),
                                if state.remove_wallpaper {
                                    "Wallpaper will be removed"
                                } else {
                                    "Remove this chat's wallpaper"
                                },
                                true,
                                cx,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = this.chat_look_dialog.as_mut() {
                                    d.remove_wallpaper = !d.remove_wallpaper;
                                    d.background = None;
                                }
                                cx.notify();
                            })),
                        );
                    }
                    if state.background.is_some() {
                        wall = wall.child(if premium {
                            Checkbox::new("chat-look-both")
                                .checked(state.both)
                                .label(format!("Also set for {name}"))
                                .on_click(cx.listener(|this, &on, _, cx| {
                                    if let Some(d) = this.chat_look_dialog.as_mut() {
                                        d.both = on;
                                    }
                                    cx.notify();
                                }))
                                .into_any_element()
                        } else {
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(format!(
                                    "The wallpaper is set for you only. Telegram Premium can set it for {name} too."
                                ))
                                .into_any_element()
                        });
                    }
                    let changed = {
                        let current_theme = this
                            .session()
                            .and_then(|s| s.chat_theme_names.get(&chat_id).cloned());
                        !plan_changes(current_theme.as_deref(), has_own, &state).is_empty()
                    };
                    let footer = div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("chat-look-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_chat_look_dialog(cx);
                                    this.close_kit_dialog_if_done(DialogKind::ChatLook, window, cx);
                                })),
                        )
                        .child(
                            Button::new("chat-look-apply")
                                .label("Apply")
                                .primary()
                                .disabled(!changed)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_chat_look(cx);
                                    this.close_kit_dialog_if_done(DialogKind::ChatLook, window, cx);
                                })),
                        )
                        .into_any_element();
                    ("Chat colors and wallpaper", body.child(wall).into_any_element(), footer)
                }
                LookTarget::Link { name } => {
                    let session = this.session();
                    let found = session.and_then(|s| s.searched_background.clone());
                    let error = session.and_then(|s| s.background_error.clone());
                    let wallpaper = found.as_ref().and_then(|bg| {
                        session.and_then(|s| session_wallpaper(s, bg))
                    });
                    let mut body = div().flex().flex_col().gap_3();
                    match (&found, error) {
                        (Some(_), _) => {
                            body = body.child(this.chat_look_preview(
                                wallpaper.as_ref(),
                                0,
                                None,
                                cx,
                            ));
                            body = body.child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child("Sets this wallpaper for your account in the current light or dark theme."),
                            );
                        }
                        (None, Some(error)) => {
                            body = body.child(
                                div()
                                    .id("chat-look-link-error")
                                    .text_sm()
                                    .text_color(super::danger_dark())
                                    .child(format!("This wallpaper link is invalid or has expired ({error}).")),
                            );
                        }
                        (None, None) => {
                            body = body.child(
                                div().text_xs().text_color(muted).child(format!("Looking up “{name}”…")),
                            );
                        }
                    }
                    let can_apply = found.is_some();
                    let footer = div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("chat-look-link-cancel")
                                .label("Cancel")
                                .ghost()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_chat_look_dialog(cx);
                                    this.close_kit_dialog_if_done(DialogKind::ChatLook, window, cx);
                                })),
                        )
                        .child(
                            Button::new("chat-look-link-apply")
                                .label("Set as wallpaper")
                                .primary()
                                .disabled(!can_apply)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_background_link(cx);
                                    this.close_kit_dialog_if_done(DialogKind::ChatLook, window, cx);
                                })),
                        )
                        .into_any_element();
                    ("Wallpaper", body.into_any_element(), footer)
                }
            };
            let body = Rc::new(RefCell::new(Some(body)));
            dialog
                .overlay(true)
                .width(px(440.))
                .title(super::shell::dialog_title(title))
                .content(super::shell::scrollable_dialog_content(move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChatLookDialog, LookChange, LookTarget, chat_look_allowed, is_wallpaper_image, plan_changes,
    };
    use std::path::Path;

    fn dialog() -> ChatLookDialog {
        ChatLookDialog {
            target: LookTarget::Chat(1),
            theme: None,
            background: None,
            remove_wallpaper: false,
            both: false,
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
        assert!(plan_changes(None, false, &dialog()).is_empty());
        let mut same = dialog();
        same.theme = Some("🏠".into());
        assert!(plan_changes(Some("🏠"), true, &same).is_empty());
    }

    #[test]
    fn theme_and_wallpaper_changes_are_planned_separately() {
        let mut pending = dialog();
        pending.theme = Some("🌷".into());
        pending.background = Some(9);
        assert_eq!(
            plan_changes(None, false, &pending),
            vec![LookChange::Theme("🌷".into()), LookChange::Wallpaper(9)]
        );
        // Dropping the theme sends Telegram's empty name.
        let mut remove = dialog();
        remove.remove_wallpaper = true;
        assert_eq!(
            plan_changes(Some("🌷"), true, &remove),
            vec![
                LookChange::Theme(String::new()),
                LookChange::RemoveWallpaper
            ]
        );
        // Nothing to remove: no request.
        assert!(plan_changes(None, false, &remove).is_empty());
    }

    #[test]
    fn wallpaper_files_are_photos() {
        assert!(is_wallpaper_image(Path::new("/a/b.JPG")));
        assert!(is_wallpaper_image(Path::new("c.webp")));
        assert!(!is_wallpaper_image(Path::new("c.gif")));
        assert!(!is_wallpaper_image(Path::new("noext")));
    }
}

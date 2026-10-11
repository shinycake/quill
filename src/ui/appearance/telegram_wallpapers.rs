//! Settings > Appearance: the account's Telegram wallpapers (tdesktop
//! `settings_chat.cpp` `BackgroundRow` and `boxes/background_box.cpp`).
//! A wallpaper opens the preview; the × on one that is not in use and not
//! a Telegram default asks before removing it; "From file…" previews an
//! image; "Tile background" shows for photo wallpapers.

use super::*;
use crate::ui::wallpaper::Wallpaper;
use crate::ui::wallpaper_preview::PreviewSource;
use gpui_kit::component::checkbox::Checkbox;
use quill::telegram::envelope::Background;

/// Whether the gallery offers removing `background`: not the one in use
/// and not one of Telegram's defaults (tdesktop shows the delete corner
/// only for those).
pub(super) fn removable(background: &Background, in_use: Option<i64>) -> bool {
    !background.is_default && in_use != Some(background.id)
}

impl QuillApp {
    pub(super) fn appearance_telegram_wallpapers_section(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dark = cx.theme().is_dark();
        let session = self.session();
        let list = session.and_then(|s| s.chats_state.installed_backgrounds.clone());
        let in_use = session
            .and_then(|s| s.chats_state.default_backgrounds.get(&dark))
            .map(|b| b.id)
            .filter(|_| self.appearance.telegram_wallpaper);
        let error = session.and_then(|s| s.chats_state.background_error.clone());
        let muted = cx.theme().muted_foreground;
        let mut grid = div().flex().flex_wrap().gap_2();
        match &list {
            None => {
                grid = grid.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading wallpapers…"),
                );
            }
            Some(list) if list.is_empty() => {
                grid = grid.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("No wallpapers installed on this account."),
                );
            }
            Some(list) => {
                for background in list {
                    let id = background.id;
                    let mut tile = self
                        .wallpaper_tile(
                            ("appearance-tg-wallpaper", id as u64),
                            background,
                            in_use == Some(id),
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_wallpaper_preview(PreviewSource::Installed(id), cx)
                        }));
                    if removable(background, in_use) {
                        tile = tile.child(
                            div().absolute().top_0().right_0().child(
                                Button::new(("appearance-tg-wallpaper-remove", id as u64))
                                    .icon(gpui_kit::assets::IconName::X)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Remove from installed wallpapers")
                                    .accessibility_label("Remove wallpaper")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.dialogs.wallpaper_remove = Some(id);
                                        cx.notify();
                                    })),
                            ),
                        );
                    }
                    grid = grid.child(tile);
                }
            }
        }
        let mut body = div().flex().flex_col().gap_2().child(grid);
        if let Some(id) = self.dialogs.wallpaper_remove {
            body = body.child(self.wallpaper_remove_confirm(id, cx));
        }
        body = body.child(
            div().flex().child(
                Button::new("appearance-tg-wallpaper-file")
                    .label("From file…")
                    .small()
                    .tooltip("Preview a JPEG, PNG or WebP image as your wallpaper")
                    .on_click(cx.listener(|this, _, _, cx| this.choose_wallpaper_file(cx))),
            ),
        );
        // tdesktop shows "Tile background" only for photo wallpapers.
        if self.appearance.telegram_wallpaper
            && matches!(
                self.account_telegram_wallpaper(dark),
                Some(Wallpaper::Image { .. })
            )
        {
            body = body.child(
                Checkbox::new("appearance-tg-wallpaper-tile")
                    .checked(self.appearance.wallpaper_tile(dark))
                    .label("Tile background")
                    .on_click(cx.listener(move |this, &on, _, cx| {
                        this.set_appearance(cx, |a| a.set_wallpaper_tile(dark, on));
                    })),
            );
        }
        if let Some(error) = error {
            body = body.child(
                div()
                    .id("appearance-tg-wallpaper-error")
                    .text_xs()
                    .text_color(super::super::danger_dark())
                    .child(format!("Couldn’t update wallpapers: {error}")),
            );
        }
        self.appearance_section(
            cx,
            "Telegram wallpapers",
            "Your installed wallpapers: colors, gradients, patterns and photos. Pick one to preview it. Chat wallpapers and themes are set per chat from its info panel.",
            body.into_any_element(),
        )
    }

    /// tdesktop `lng_background_sure_delete`, inline under the gallery.
    fn wallpaper_remove_confirm(&self, id: i64, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .child("Delete this wallpaper from your installed wallpapers?"),
            )
            .child(
                Button::new("appearance-tg-wallpaper-remove-cancel")
                    .label("Cancel")
                    .ghost()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dialogs.wallpaper_remove = None;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("appearance-tg-wallpaper-remove-confirm")
                    .label("Delete")
                    .danger()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dialogs.wallpaper_remove = None;
                        this.remove_telegram_wallpaper(id, cx);
                    })),
            )
            .into_any_element()
    }

    pub(super) fn remove_telegram_wallpaper(&mut self, background_id: i64, cx: &mut Context<Self>) {
        match self.live.as_mut() {
            Some(live) => {
                if let Err(err) = live.driver.remove_installed_background(background_id) {
                    self.connection.status_note =
                        format!("could not remove the wallpaper: {err:?}");
                }
            }
            None => {
                if let Some(session) = self.demo_session.as_mut()
                    && let Some(list) = session.chats_state.installed_backgrounds.as_mut()
                {
                    list.retain(|b| b.id != background_id);
                }
            }
        }
        cx.notify();
    }

    /// Fetch the installed wallpapers when Appearance opens, and start the
    /// photo and pattern downloads.
    pub(crate) fn ensure_wallpapers_loaded(&mut self, cx: &mut Context<Self>) {
        let dark = cx.theme().is_dark();
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .session
                .chats_state
                .installed_backgrounds
                .is_none()
            {
                let _ = live.driver.fetch_installed_backgrounds(dark);
            }
            live.driver.download_background_files();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::removable;
    use quill::telegram::envelope::{Background, BackgroundFill, BackgroundType};

    fn bg(id: i64, is_default: bool) -> Background {
        Background {
            id,
            is_default,
            is_dark: false,
            name: String::new(),
            file: None,
            kind: BackgroundType::Fill(BackgroundFill::Solid(0)),
        }
    }

    #[test]
    fn only_unused_custom_wallpapers_can_be_removed() {
        assert!(removable(&bg(1, false), Some(2)));
        assert!(removable(&bg(1, false), None));
        assert!(!removable(&bg(1, false), Some(1)), "in use");
        assert!(!removable(&bg(1, true), None), "a Telegram default");
    }
}

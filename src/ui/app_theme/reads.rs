//! Custom themes read off the UI thread: a theme's background image is
//! decoded for the colour standing for it (`image_color`), which takes tens
//! of milliseconds for a large picture.
#![deny(clippy::indexing_slicing)]

use super::{CustomTheme, QuillApp, file_palette};
use gpui_kit::*;
use quill::app_theme::{Palette, ThemeId, store};
use std::path::Path;

/// Every readable custom theme in `dir`, for the chooser's tiles.
fn read_custom_themes(dir: &Path) -> Vec<CustomTheme> {
    store::list(dir)
        .into_iter()
        .filter_map(|name| {
            let file = store::load(dir, &name).ok()?;
            Some(CustomTheme::of(name, &file_palette(&file)))
        })
        .collect()
}

fn read_custom_palette(dir: &Path, name: &str) -> Option<Palette> {
    store::load(dir, name).ok().map(|file| file_palette(&file))
}

/// A custom theme's palette as [`QuillApp::custom_theme_palette`] has it.
pub(super) enum PaletteRead {
    /// Read: `None` when the file could not be.
    Read(Option<Palette>),
    /// Being read: what to show meanwhile.
    Reading((ThemeId, Palette)),
}

impl QuillApp {
    /// The custom themes for the chooser, read off the UI thread the first
    /// time; empty until then. A file that cannot be read is left out.
    pub(crate) fn custom_themes(&mut self, cx: &mut Context<Self>) -> &[CustomTheme] {
        let themes = &mut self.settings.app_theme;
        if themes.custom.is_none() && themes.custom_read.is_none() {
            let dir = Self::themes_dir();
            let read = cx.background_spawn(async move { read_custom_themes(&dir) });
            themes.custom_read = Some(cx.spawn(async move |this, cx| {
                let read = read.await;
                let _ = this.update(cx, |this, cx| {
                    this.settings.app_theme.custom = Some(read);
                    this.settings.app_theme.custom_read = None;
                    cx.notify();
                });
            }));
        }
        self.settings
            .app_theme
            .custom
            .as_deref()
            .unwrap_or_default()
    }

    /// A custom theme's palette, read once per theme. While a theme is read
    /// off the UI thread the theme applied before stays, and the appearance
    /// is applied again once it is in. With nothing applied yet (at
    /// startup) it is read at once, as tdesktop applies its theme before
    /// the first frame.
    pub(super) fn custom_theme_palette(
        &mut self,
        name: &str,
        cx: &mut Context<Self>,
    ) -> PaletteRead {
        let themes = &mut self.settings.app_theme;
        if let Some(palette) = themes.cached_palette(name) {
            return PaletteRead::Read(palette.clone());
        }
        let Some(shown) = themes.applied.clone() else {
            let palette = read_custom_palette(&Self::themes_dir(), name);
            themes.cache_palette(name.to_string(), palette.clone());
            return PaletteRead::Read(palette);
        };
        if !themes
            .palette_read
            .as_ref()
            .is_some_and(|(reading, _)| reading == name)
        {
            let dir = Self::themes_dir();
            let wanted = name.to_string();
            let read = cx.background_spawn(async move { read_custom_palette(&dir, &wanted) });
            let wanted = name.to_string();
            let task = cx.spawn(async move |this, cx| {
                let palette = read.await;
                let _ = this.update(cx, |this, cx| {
                    let themes = &mut this.settings.app_theme;
                    themes.palette_read = None;
                    themes.cache_palette(wanted, palette);
                    this.apply_appearance(cx);
                });
            });
            themes.palette_read = Some((name.to_string(), task));
        }
        PaletteRead::Reading(shown)
    }
}

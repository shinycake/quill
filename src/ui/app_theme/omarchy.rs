//! Following Omarchy's theme: read once when appearance is first applied,
//! then re-read whenever Omarchy's theme generation changes. Off Omarchy
//! nothing is found and nothing is watched.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::*;
use quill::app_theme::Palette;
use quill::app_theme::omarchy::{OmarchyTheme, current_dir, stamp};
use std::path::PathBuf;
use std::time::Duration;

/// Between looks at the generation file: a theme switch is done by hand,
/// and one small read costs nothing.
const POLL: Duration = Duration::from_secs(2);

pub(crate) struct OmarchyState {
    dir: PathBuf,
    stamp: Option<String>,
    pub(crate) theme: OmarchyTheme,
    pub(crate) palette: Palette,
}

impl OmarchyState {
    fn read(dir: PathBuf) -> Option<Self> {
        let theme = OmarchyTheme::read(&dir)?;
        Some(Self {
            stamp: stamp(&dir),
            palette: theme.palette(),
            theme,
            dir,
        })
    }
}

impl QuillApp {
    /// Omarchy's palette, looked for on first use.
    pub(super) fn omarchy_palette(&mut self) -> Option<Palette> {
        if !self.settings.app_theme.omarchy_probed {
            self.settings.app_theme.omarchy_probed = true;
            self.settings.app_theme.omarchy = current_dir().and_then(OmarchyState::read);
        }
        self.settings
            .app_theme
            .omarchy
            .as_ref()
            .map(|state| state.palette.clone())
    }

    /// The theme Omarchy shows, when Quill runs on Omarchy.
    pub(crate) fn omarchy_theme(&self) -> Option<&OmarchyState> {
        self.settings.app_theme.omarchy.as_ref()
    }

    /// The tile's "follow Omarchy" choice.
    pub(crate) fn follow_omarchy(&mut self, cx: &mut Context<Self>) {
        self.set_appearance(cx, |a| a.follow_omarchy = true);
    }
}

impl QuillApp {
    /// Re-read the theme whenever Omarchy's generation stamp changes;
    /// started once, when a theme was found (not in screenshot demos).
    pub(crate) fn start_omarchy_watch(&mut self, cx: &mut Context<Self>) {
        if self.demo_session.is_some() || self.settings.app_theme.omarchy_watched {
            return;
        }
        let Some(dir) = self.omarchy_theme().map(|s| s.dir.clone()) else {
            return;
        };
        self.settings.app_theme.omarchy_watched = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let probe = dir.clone();
                let seen = cx
                    .background_executor()
                    .spawn(async move { stamp(&probe) })
                    .await;
                let Ok(()) = this.update(cx, |app, cx| {
                    let current = app
                        .settings
                        .app_theme
                        .omarchy
                        .as_ref()
                        .and_then(|s| s.stamp.clone());
                    if seen == current {
                        return;
                    }
                    if let Some(state) = OmarchyState::read(dir.clone()) {
                        app.settings.app_theme.omarchy = Some(state);
                        app.apply_appearance(cx);
                        cx.notify();
                    }
                }) else {
                    return;
                };
            }
        })
        .detach();
    }
}

#[cfg(feature = "demo-capture")]
impl QuillApp {
    /// Screenshot demos and tests: Omarchy found at `dir`, no watcher.
    pub(crate) fn use_omarchy_dir(&mut self, dir: PathBuf) {
        self.settings.app_theme.omarchy_probed = true;
        self.settings.app_theme.omarchy = OmarchyState::read(dir);
    }
}

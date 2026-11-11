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

/// Following Omarchy, by stage: not looked for, looked for and absent, or
/// found (and, once the watcher runs, watched).
#[derive(Default)]
pub(crate) enum Omarchy {
    #[default]
    Unprobed,
    Absent,
    Found(OmarchyState),
    /// Found, and the watcher re-reads it when the generation changes.
    Watching(OmarchyState),
}

impl Omarchy {
    fn probe(dir: Option<PathBuf>) -> Self {
        dir.and_then(OmarchyState::read)
            .map_or(Self::Absent, Self::Found)
    }

    fn state(&self) -> Option<&OmarchyState> {
        match self {
            Self::Unprobed | Self::Absent => None,
            Self::Found(state) | Self::Watching(state) => Some(state),
        }
    }

    /// The watcher started: `Found` becomes `Watching`.
    fn watch(&mut self) {
        if let Self::Found(_) = self
            && let Self::Found(state) = std::mem::take(self)
        {
            *self = Self::Watching(state);
        }
    }

    /// A re-read theme replaces the state, keeping the watcher's stage.
    fn replace(&mut self, state: OmarchyState) {
        *self = match self {
            Self::Watching(_) => Self::Watching(state),
            _ => Self::Found(state),
        };
    }
}

impl QuillApp {
    /// Omarchy's palette, looked for on first use.
    pub(super) fn omarchy_palette(&mut self) -> Option<Palette> {
        if matches!(self.preferences.app_theme.omarchy, Omarchy::Unprobed) {
            self.preferences.app_theme.omarchy = Omarchy::probe(current_dir());
        }
        self.omarchy_theme().map(|state| state.palette.clone())
    }

    /// The theme Omarchy shows, when Quill runs on Omarchy.
    pub(crate) fn omarchy_theme(&self) -> Option<&OmarchyState> {
        self.preferences.app_theme.omarchy.state()
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
        if self.demo_session.is_some() {
            return;
        }
        let Omarchy::Found(state) = &self.preferences.app_theme.omarchy else {
            return;
        };
        let dir = state.dir.clone();
        self.preferences.app_theme.omarchy.watch();
        // screen-scope: the Omarchy watcher re-applies the app's appearance for whichever account is on screen.
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
                        .preferences
                        .app_theme
                        .omarchy
                        .state()
                        .and_then(|s| s.stamp.clone());
                    if seen == current {
                        return;
                    }
                    if let Some(state) = OmarchyState::read(dir.clone()) {
                        app.preferences.app_theme.omarchy.replace(state);
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
        self.preferences.app_theme.omarchy = Omarchy::probe(Some(dir));
    }
}

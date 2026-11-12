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
    /// Found, and the watcher re-reads it when the generation changes. The
    /// watcher's task is owned here: leaving this stage stops it.
    Watching {
        state: OmarchyState,
        _task: Task<()>,
    },
}

impl Omarchy {
    fn probe(dir: Option<PathBuf>) -> Self {
        dir.and_then(OmarchyState::read)
            .map_or(Self::Absent, Self::Found)
    }

    fn state(&self) -> Option<&OmarchyState> {
        match self {
            Self::Unprobed | Self::Absent => None,
            Self::Found(state) | Self::Watching { state, .. } => Some(state),
        }
    }

    /// The watcher started: `Found` becomes `Watching`. Any other stage has
    /// no theme to watch, and the task is dropped (stopped).
    fn watch(&mut self, task: Task<()>) {
        if let Self::Found(_) = self
            && let Self::Found(state) = std::mem::take(self)
        {
            *self = Self::Watching { state, _task: task };
        }
    }

    /// The watcher is not wanted: `Watching` returns to `Found`, dropping
    /// (stopping) its task.
    fn unwatch(&mut self) {
        if let Self::Watching { .. } = self
            && let Self::Watching { state, .. } = std::mem::take(self)
        {
            *self = Self::Found(state);
        }
    }

    /// A re-read theme replaces the state, keeping the watcher's stage.
    fn replace(&mut self, new: OmarchyState) {
        match self {
            Self::Watching { state, .. } | Self::Found(state) => *state = new,
            Self::Unprobed | Self::Absent => *self = Self::Found(new),
        }
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
    /// Re-read the theme whenever Omarchy's generation stamp changes, only
    /// while Quill follows it (not in screenshot demos): the watcher starts
    /// when a found theme is followed and stops when it no longer is.
    pub(crate) fn sync_omarchy_watch(&mut self, cx: &mut Context<Self>) {
        if self.demo_session.is_some() {
            return;
        }
        if !self.appearance.follow_omarchy {
            self.preferences.app_theme.omarchy.unwatch();
            return;
        }
        let Omarchy::Found(state) = &self.preferences.app_theme.omarchy else {
            return;
        };
        let dir = state.dir.clone();
        // The theme may have changed while nobody watched.
        if let Some(fresh) = OmarchyState::read(dir.clone()) {
            self.preferences.app_theme.omarchy.replace(fresh);
        }
        // screen-scope: the Omarchy watcher re-applies the app's appearance for whichever account is on screen.
        let task = cx.spawn(async move |this, cx| {
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
        });
        self.preferences.app_theme.omarchy.watch(task);
    }
}

#[cfg(feature = "demo-capture")]
impl QuillApp {
    /// Screenshot demos and tests: Omarchy found at `dir`, no watcher.
    pub(crate) fn use_omarchy_dir(&mut self, dir: PathBuf) {
        self.preferences.app_theme.omarchy = Omarchy::probe(Some(dir));
    }
}

#[cfg(test)]
mod tests {
    use super::Omarchy;
    use gpui_kit::Task;

    fn found() -> Omarchy {
        let root = quill::scratch::process_dir("quill-omarchy-stage");
        let theme = root.join("theme");
        let _ = std::fs::create_dir_all(&theme);
        let _ = std::fs::write(
            theme.join("colors.toml"),
            "mode = \"dark\"\nbackground = \"#1a1b26\"\nforeground = \"#a9b1d6\"\n",
        );
        Omarchy::probe(Some(root))
    }

    #[test]
    fn the_watcher_runs_only_between_watch_and_unwatch() {
        let mut omarchy = found();
        assert!(matches!(omarchy, Omarchy::Found(_)));
        omarchy.watch(Task::ready(()));
        assert!(matches!(omarchy, Omarchy::Watching { .. }));
        omarchy.unwatch();
        assert!(matches!(omarchy, Omarchy::Found(_)));
        assert!(omarchy.state().is_some());
    }

    #[test]
    fn a_stage_without_a_theme_has_nothing_to_watch() {
        let mut omarchy = Omarchy::Absent;
        omarchy.watch(Task::ready(()));
        assert!(matches!(omarchy, Omarchy::Absent));
        omarchy.unwatch();
        assert!(matches!(omarchy, Omarchy::Absent));
    }
}

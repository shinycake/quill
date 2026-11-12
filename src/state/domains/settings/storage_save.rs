//! Data & Storage saves: which one failed, and which answers an editor waits
//! for. A failure belongs to the kind of save that failed, so a later `ok`
//! of another request never wipes it.

/// What a Data & Storage save was for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSaveKind {
    /// `setAutoDownloadSettings` (the editor's Save, the less-data toggle).
    AutoDownload,
    /// The storage limit options.
    Limits,
    /// The network statistics reset.
    ResetStatistics,
    /// Writing the settings to `data_storage.json` (the driver's write
    /// after a seed or a save).
    Persist,
}

/// The failure line of each kind of save. A failure stays until the next
/// save of its kind starts or the page is reopened; an `ok` clears nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageSaveFailures {
    lines: Vec<(StorageSaveKind, String)>,
}

impl StorageSaveFailures {
    /// A save of `kind` failed: `line` replaces its earlier failure.
    pub fn fail(&mut self, kind: StorageSaveKind, line: impl Into<String>) {
        self.begin(kind);
        self.lines.push((kind, line.into()));
    }

    /// A new save of `kind` starts: its earlier failure goes.
    pub fn begin(&mut self, kind: StorageSaveKind) {
        self.lines.retain(|(held, _)| *held != kind);
    }

    /// Every failure goes (the page is opened or left).
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// The failure of `kind`.
    pub fn error_of(&self, kind: StorageSaveKind) -> Option<&str> {
        self.lines
            .iter()
            .find(|(held, _)| *held == kind)
            .map(|(_, line)| line.as_str())
    }

    /// The oldest failure held, for the overview's line.
    pub fn error(&self) -> Option<&str> {
        self.lines.first().map(|(_, line)| line.as_str())
    }
}

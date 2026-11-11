//! Settings → Chat → "Choose emoji set" (parity:emoji-set-style), after
//! Telegram Desktop's `Ui::Emoji::ManageSetsBox`: one row per set with a
//! 2×2 preview, its name and state ("Download 4.3 MB", "42%, …",
//! "Downloaded", "Current set") and a radio. Clicking a set that is not
//! downloaded downloads it and switches to it once installed; clicking a
//! downloaded one switches at once. The download keeps going when the box
//! closes, as tdesktop's global loader does.
//!
//! The core (sets, packs, download) is `quill::emoji_sets_style`; the
//! pictures are drawn through `emoji_sprites`.
#![deny(clippy::indexing_slicing)]

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::app::QuillApp;
use super::folder_share::finish_dialog;
use super::pressable::PressableDiv;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::radio::Radio;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::emoji_sets_style::catalog::{
    EmojiSetId, SetState, has_saved_choice, load_current, save_current, set_dir,
};
use quill::emoji_sets_style::download::Progress;
use quill::emoji_sets_style::pack::{SpritePack, is_ready};
use quill::lang::{keys, tr, tr_with};

/// The four faces of tdesktop's set previews (`setN_preview.webp`).
const PREVIEW: [&str; 4] = ["\u{1f603}", "\u{1f609}", "\u{1f614}", "\u{1f630}"];

/// tdesktop `manageEmojiPreview` and `manageEmojiPreviewWidth`.
const PREVIEW_EMOJI: f32 = 22.;
const PREVIEW_BOX: f32 = 48.;

/// What the set choice is doing now, one value. A new choice replaces it,
/// which stops the one before: a dropped `Task` is cancelled, and a download
/// is told to stop. A task acts only while its own stage is the one in place,
/// so a download that lands after the user chose another set changes
/// nothing.
#[derive(Default)]
enum SetStage {
    #[default]
    Idle,
    /// Loading an installed pack to draw with. The task knows the set and
    /// whether it was just installed (a pack that does not load then is a
    /// failure to show, not a reason to download again).
    Switching { task: Task<()> },
    Downloading {
        id: EmojiSetId,
        progress: Arc<Progress>,
        task: Task<()>,
    },
    /// The last download (or a fresh install that does not load) failed.
    Failed { id: EmojiSetId, why: String },
}

impl SetStage {
    /// A new stage replaces this one and stops its work.
    fn replace(&mut self, next: SetStage) {
        if let Self::Downloading { progress, .. } = self {
            progress.cancel();
        }
        *self = next;
    }

    /// The stage's own task is done: leave it running to its end instead of
    /// dropping (cancelling) it from inside itself.
    fn finish(&mut self, next: SetStage) {
        match std::mem::replace(self, next) {
            Self::Switching { task, .. } | Self::Downloading { task, .. } => task.detach(),
            Self::Idle | Self::Failed { .. } => {}
        }
    }

    fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
}

/// The box and the set choice, on `SettingsUi`.
#[derive(Default)]
pub(super) struct EmojiStyleUi {
    pub(super) open: bool,
    /// The saved choice has been read and applied.
    started: bool,
    /// The set in use.
    current: EmojiSetId,
    /// Sets whose pack is installed.
    ready: HashSet<EmojiSetId>,
    stage: SetStage,
    /// The latest write of the choice; the next one waits for it.
    save_task: Option<Task<()>>,
    /// Reading the saved choice at startup.
    startup_task: Option<Task<()>>,
    /// "Install from file…": the task, and why the last file was refused.
    file_task: Option<Task<()>>,
    file_error: Option<String>,
}

/// `<app data>`, where `emoji-sets/` lives.
fn app_root() -> Option<PathBuf> {
    quill::settings::safe_app_root()
}

/// Screenshot demos only: install the packs in `QUILL_DEMO_EMOJI_PACKS`
/// (the builder's output folder) into the demo's throwaway data root and
/// pick `QUILL_DEMO_EMOJI_SET`.
fn demo_install(root: &std::path::Path) {
    let Ok(dir) = std::env::var("QUILL_DEMO_EMOJI_PACKS") else {
        return;
    };
    for id in EmojiSetId::ALL.into_iter().filter(|id| !id.is_system()) {
        if let Ok(zip) =
            std::fs::read(PathBuf::from(&dir).join(format!("emoji-set-{}.zip", id.key())))
        {
            let _ = quill::emoji_sets_style::download::install(root, id, &zip);
        }
    }
    if let Some(id) = std::env::var("QUILL_DEMO_EMOJI_SET")
        .ok()
        .and_then(|key| EmojiSetId::from_key(&key))
    {
        let _ = save_current(root, id);
    }
}

/// What startup and switching load off the UI thread.
struct Loaded {
    current: EmojiSetId,
    ready: HashSet<EmojiSetId>,
    pack: Option<(PathBuf, Arc<SpritePack>)>,
    /// No color emoji font and no saved choice: this pack should be
    /// downloaded and switched to.
    fallback: Option<EmojiSetId>,
}

fn load_state(root: &std::path::Path, wanted: Option<EmojiSetId>) -> Loaded {
    let ready: HashSet<EmojiSetId> = EmojiSetId::ALL
        .into_iter()
        .filter(|id| !id.is_system() && is_ready(&set_dir(root, *id)))
        .collect();
    let wanted = wanted.unwrap_or_else(|| load_current(root));
    let pack = (!wanted.is_system())
        .then(|| {
            let dir = set_dir(root, wanted);
            SpritePack::load(&dir).map(|pack| (dir, Arc::new(pack)))
        })
        .flatten();
    // A set whose files went missing falls back to the system font.
    let current = if pack.is_some() {
        wanted
    } else {
        EmojiSetId::System
    };
    Loaded {
        current,
        ready,
        pack,
        fallback: None,
    }
}

impl QuillApp {
    /// Apply the saved set once, at the first frame.
    pub(super) fn sync_emoji_style(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.preferences.emoji_style;
        if state.started {
            return;
        }
        state.started = true;
        let Some(root) = app_root() else {
            return;
        };
        let demo = self.demo_session.is_some();
        let load = cx.background_spawn(async move {
            if demo {
                demo_install(&root);
            }
            let fallback = quill::emoji_sets_style::font_probe::fallback_set(
                has_saved_choice(&root),
                quill::emoji_sets_style::font_probe::color_emoji_font_installed(),
            );
            let mut loaded = load_state(&root, fallback);
            if fallback.is_some() && loaded.current != EmojiSetId::Noto {
                loaded.fallback = fallback;
            }
            loaded
        });
        self.preferences.emoji_style.startup_task = Some(cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| this.apply_emoji_state(loaded, cx));
        }));
    }

    fn apply_emoji_state(&mut self, loaded: Loaded, cx: &mut Context<Self>) {
        let state = &mut self.preferences.emoji_style;
        state.ready = loaded.ready;
        // A choice made while this loaded wins over the saved one and over
        // the startup fallback download.
        let download_fallback = loaded.fallback.filter(|_| state.stage.is_idle());
        let changed =
            state.current != loaded.current || super::emoji_sprites::active_set() != loaded.current;
        state.current = loaded.current;
        if changed {
            super::emoji_sprites::install(
                loaded.pack.map(|(dir, pack)| (loaded.current, dir, pack)),
                cx,
            );
            cx.refresh_windows();
        }
        if let Some(id) = download_fallback {
            self.download_emoji_set(id, cx);
        }
        cx.notify();
    }

    pub(super) fn open_emoji_set_style(&mut self, cx: &mut Context<Self>) {
        self.preferences.emoji_style.open = true;
        self.open_box(DialogKind::EmojiSetStyle);
        self.preferences.emoji_style.file_error = None;
        cx.notify();
    }

    pub(super) fn close_emoji_set_style(&mut self, cx: &mut Context<Self>) {
        self.preferences.emoji_style.open = false;
        cx.notify();
    }

    fn emoji_set_state(&self, id: EmojiSetId) -> SetState {
        let state = &self.preferences.emoji_style;
        let loading = match &state.stage {
            SetStage::Downloading { id, progress, .. } => Some((*id, progress.received())),
            _ => None,
        };
        let failed = match &state.stage {
            SetStage::Failed { id, why } => Some((*id, why.as_str())),
            _ => None,
        };
        SetState::of(
            id,
            state.current,
            state.ready.contains(&id),
            loading,
            failed,
        )
    }

    /// A row was clicked (tdesktop `Row::setupHandler`).
    pub(super) fn choose_emoji_set(&mut self, id: EmojiSetId, cx: &mut Context<Self>) {
        match self.emoji_set_state(id) {
            SetState::Ready => self.switch_emoji_set(id, false, cx),
            SetState::Available(_) | SetState::Failed(_) => self.download_emoji_set(id, cx),
            // Choosing the set in use again leaves a download of another
            // one: the choice is what it was.
            SetState::Active => {
                let state = &mut self.preferences.emoji_style;
                if matches!(state.stage, SetStage::Downloading { .. }) {
                    state.stage.replace(SetStage::Idle);
                    cx.notify();
                }
            }
            SetState::Loading { .. } => {}
        }
    }

    /// tdesktop `SwitchToSet`: load the pack off the UI thread, then draw
    /// with it. A pack that no longer loads is downloaded again.
    fn switch_emoji_set(&mut self, id: EmojiSetId, after_download: bool, cx: &mut Context<Self>) {
        let Some(root) = app_root() else {
            return;
        };
        let saved_root = root.clone();
        let load = cx.background_spawn(async move { load_state(&root, Some(id)) });
        let task = cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| {
                if loaded.current == id {
                    this.preferences.emoji_style.stage.finish(SetStage::Idle);
                    // The choice is saved off the UI thread, and only by the
                    // switch still in place: an older one never writes over
                    // a newer choice.
                    // Saves run one after another, in the order the
                    // switches were applied, so a quick A then B never
                    // ends with A on disk.
                    let state = &mut this.preferences.emoji_style;
                    let earlier = state.save_task.take();
                    state.save_task = Some(cx.background_spawn(async move {
                        if let Some(earlier) = earlier {
                            earlier.await;
                        }
                        let _ = save_current(&saved_root, id);
                    }));
                    this.apply_emoji_state(loaded, cx);
                } else if after_download {
                    // A pack that was just installed and still does not load
                    // (a version mismatch, an unwritable folder): say so and
                    // stop, instead of downloading it again for ever.
                    this.preferences.emoji_style.ready.remove(&id);
                    this.preferences.emoji_style.stage.finish(SetStage::Failed {
                        id,
                        why: tr(keys::Q_MEDIA_COULDNT_LOAD_THE_EMOJI_SET).to_string(),
                    });
                } else {
                    // It loaded before and no longer does: install it again.
                    this.preferences.emoji_style.ready.remove(&id);
                    this.preferences.emoji_style.stage.finish(SetStage::Idle);
                    this.download_emoji_set(id, cx);
                }
                cx.notify();
            });
        });
        self.preferences
            .emoji_style
            .stage
            .replace(SetStage::Switching { task });
        cx.notify();
    }

    /// tdesktop `LoadAndSwitchTo`: one download at a time; a new one
    /// replaces the one in flight.
    fn download_emoji_set(&mut self, id: EmojiSetId, cx: &mut Context<Self>) {
        let Some(root) = app_root() else {
            return;
        };
        let state = &mut self.preferences.emoji_style;
        state.file_error = None;
        let progress = Arc::new(Progress::default());
        let worker_progress = progress.clone();
        let worker = cx.background_spawn(async move {
            let result = quill::emoji_sets_style::download::download(&root, id, &worker_progress);
            worker_progress.finish();
            result
        });
        let watched = progress.clone();
        let task = cx.spawn(async move |this, cx| {
            while !watched.is_finished() {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                // Progress is drawn only by the open box: a download that
                // goes on behind a closed one redraws nothing.
                let _ = this.update(cx, |this, cx| {
                    if this.preferences.emoji_style.open {
                        cx.notify();
                    }
                });
            }
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.preferences.emoji_style;
                // Only while this download is still the stage in place: a
                // set chosen since replaced it (and dropped this task).
                if !matches!(&state.stage, SetStage::Downloading { id: d, .. } if *d == id) {
                    return;
                }
                match result {
                    Ok(()) => {
                        state.ready.insert(id);
                        state.stage.finish(SetStage::Idle);
                        this.switch_emoji_set(id, true, cx);
                    }
                    Err(message) if message == "Cancelled." => {
                        state.stage.finish(SetStage::Idle);
                    }
                    Err(message) => state.stage.finish(SetStage::Failed { id, why: message }),
                }
                cx.notify();
            });
        });
        state
            .stage
            .replace(SetStage::Downloading { id, progress, task });
        cx.notify();
    }

    /// "Install from file…": a zip the user downloaded from the landing
    /// page (for machines that can't reach the release). Quill-only; the
    /// file's hash picks the set, then it is switched to like a download.
    fn install_emoji_set_file(&mut self, cx: &mut Context<Self>) {
        let Some(root) = app_root() else {
            self.preferences.emoji_style.file_error =
                Some(tr(keys::Q_MEDIA_COULDNT_OPEN_QUILLS_DATA_FOLDER).into());
            cx.notify();
            return;
        };
        self.preferences.emoji_style.file_error = None;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_MEDIA_INSTALL).into()),
        });
        self.preferences.emoji_style.file_task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { quill::emoji_sets_style::download::install_file(&root, &path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(id) => {
                        let state = &mut this.preferences.emoji_style;
                        // The file wins over a download in flight, which
                        // would switch sets again when it lands.
                        state.stage.replace(SetStage::Idle);
                        state.ready.insert(id);
                        this.switch_emoji_set(id, true, cx);
                    }
                    Err(message) => this.preferences.emoji_style.file_error = Some(message),
                }
                cx.notify();
            });
        }));
    }

    /// The Settings row that opens the box (tdesktop: Chat settings,
    /// "Choose emoji set" with the emoji menu icon).
    pub(super) fn emoji_set_settings_row(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().child(tr(keys::Q_MEDIA_EMOJI_SET)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.preferences.emoji_style.current.name()),
                    ),
            )
            .child(
                Button::new("appearance-emoji-sets")
                    .label(tr(keys::EMOJI_MANAGE_SETS))
                    .outline()
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.open_emoji_set_style(cx))),
            )
            .into_any_element()
    }

    fn emoji_set_row(&self, id: EmojiSetId, cx: &mut Context<Self>) -> AnyElement {
        let state = self.emoji_set_state(id);
        let status_color = match &state {
            SetState::Active => cx.theme().primary,
            SetState::Failed(_) => cx.theme().danger,
            _ => cx.theme().muted_foreground,
        };
        let status = state.description();
        let clickable = state.clickable();
        let label = format!("{}, {status}", id.name());
        let trailing = match state.progress() {
            Some(fraction) => ProgressCircle::new(("emoji-set-progress", id as usize))
                .value(fraction * 100.)
                .accessibility_label(tr_with(keys::Q_MEDIA_DOWNLOADING, &[("name", (id.name()))]))
                .small()
                .into_any_element(),
            None => Radio::new(("emoji-set-radio", id as usize))
                .checked(state == SetState::Active)
                .tab_stop(false)
                .accessibility_label(label.clone())
                .into_any_element(),
        };
        div()
            .id(("emoji-set-row", id as usize))
            .flex()
            .items_center()
            .gap_4()
            .w_full()
            .px_2()
            .py_2()
            .rounded_md()
            .role(Role::RadioButton)
            .aria_label(label)
            .tab_index(0)
            .when(clickable, |row| {
                row.cursor_pointer()
                    .pressable(cx.theme())
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_emoji_set(id, cx)))
            })
            .child(preview(id))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().text_sm().font_medium().child(id.name()))
                    .child(div().text_xs().text_color(status_color).child(status)),
            )
            .child(div().flex_none().child(trailing))
            .into_any_element()
    }

    pub(super) fn build_emoji_set_style_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::EmojiSetStyle, |this, _, cx| {
                this.close_emoji_set_style(cx);
            });
        app.update(cx, |this, cx| {
            let mut list = div().id("emoji-sets").flex().flex_col().gap_1();
            for id in EmojiSetId::ALL {
                list = list.child(this.emoji_set_row(id, cx));
            }
            let credits: Vec<&str> = EmojiSetId::ALL
                .into_iter()
                .filter_map(EmojiSetId::credit)
                .collect();
            let body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(list)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{}.", credits.join(". "))),
                )
                .when_some(
                    this.preferences.emoji_style.file_error.clone(),
                    |body, error| {
                        body.child(div().text_xs().text_color(cx.theme().danger).child(error))
                    },
                );
            let install = Button::new("emoji-sets-install-file")
                .label(tr(keys::Q_MEDIA_INSTALL_FROM_FILE))
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| this.install_emoji_set_file(cx)));
            let footer = div().flex().justify_between().gap_2().child(install).child(
                Button::new("emoji-sets-close")
                    .label(tr(keys::CLOSE))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_emoji_set_style(cx);
                        this.close_kit_dialog_if_done(DialogKind::EmojiSetStyle, window, cx);
                    })),
            );
            finish_dialog(
                dialog,
                tr(keys::EMOJI_MANAGE_SETS).to_string(),
                body.into_any_element(),
                Some(footer.into_any_element()),
                on_close,
            )
        })
    }
}

/// The row's 2×2 preview: the system font's faces, or the pack's own
/// (bundled, as tdesktop bundles its previews, so a set shows its art
/// before it is downloaded).
fn preview(id: EmojiSetId) -> AnyElement {
    let cells: Vec<AnyElement> = match preview_images(id) {
        Some(images) => images
            .into_iter()
            .map(|image| {
                img(ImageSource::Render(image))
                    .size(px(PREVIEW_EMOJI))
                    .object_fit(ObjectFit::Contain)
                    .into_any_element()
            })
            .collect(),
        None => PREVIEW
            .iter()
            .map(|emoji| {
                div()
                    .size(px(PREVIEW_EMOJI))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(PREVIEW_EMOJI * 0.8))
                    .line_height(px(PREVIEW_EMOJI))
                    .child(*emoji)
                    .into_any_element()
            })
            .collect(),
    };
    div()
        .size(px(PREVIEW_BOX))
        .flex_none()
        .flex()
        .flex_wrap()
        .content_between()
        .justify_between()
        .children(cells)
        .into_any_element()
}

thread_local! {
    static PREVIEWS: std::cell::RefCell<Vec<(EmojiSetId, Vec<Arc<RenderImage>>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The bundled preview strip of `id` cut into its four faces (BGRA).
fn preview_images(id: EmojiSetId) -> Option<Vec<Arc<RenderImage>>> {
    let bytes: &[u8] = match id {
        EmojiSetId::System => return None,
        EmojiSetId::Noto => include_bytes!("../../assets/emoji/sets/emoji-set-noto-preview.png"),
        EmojiSetId::Twemoji => {
            include_bytes!("../../assets/emoji/sets/emoji-set-twemoji-preview.png")
        }
    };
    if let Some(found) = PREVIEWS.with(|p| {
        p.borrow()
            .iter()
            .find(|(set, _)| *set == id)
            .map(|(_, images)| images.clone())
    }) {
        return Some(found);
    }
    let strip = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .to_rgba8();
    let edge = strip.height();
    let mut images = Vec::with_capacity(PREVIEW.len());
    for i in 0..PREVIEW.len() as u32 {
        if (i + 1) * edge > strip.width() {
            return None;
        }
        let mut cell = image::imageops::crop_imm(&strip, i * edge, 0, edge, edge).to_image();
        for pixel in cell.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        images.push(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
            image::Frame::new(cell),
        ]))));
    }
    PREVIEWS.with(|p| p.borrow_mut().push((id, images.clone())));
    Some(images)
}

crate::ui::shell::register_dialogs! {
    /// Settings → Chat → "Choose emoji set".
    EmojiSetStyle => DialogSpec::new(
        7250,
        |app| app.preferences.emoji_style.open,
        QuillApp::build_emoji_set_style_dialog,
    )
    .close(|app, _, cx| app.close_emoji_set_style(cx)),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "emoji_style_tests.rs"]
mod tests;

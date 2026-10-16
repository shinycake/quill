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

struct ActiveDownload {
    id: EmojiSetId,
    progress: Arc<Progress>,
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
    download: Option<ActiveDownload>,
    download_task: Option<Task<()>>,
    switch_task: Option<Task<()>>,
    failed: Option<(EmojiSetId, String)>,
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
        let state = &mut self.settings.emoji_style;
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
        self.settings.emoji_style.switch_task = Some(cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| this.apply_emoji_state(loaded, cx));
        }));
    }

    fn apply_emoji_state(&mut self, loaded: Loaded, cx: &mut Context<Self>) {
        let state = &mut self.settings.emoji_style;
        state.ready = loaded.ready;
        let download_fallback = loaded.fallback;
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
        self.settings.emoji_style.open = true;
        self.settings.emoji_style.file_error = None;
        cx.notify();
    }

    fn emoji_set_state(&self, id: EmojiSetId) -> SetState {
        let state = &self.settings.emoji_style;
        let loading = state
            .download
            .as_ref()
            .map(|d| (d.id, d.progress.received()));
        let failed = state.failed.as_ref().map(|(id, why)| (*id, why.as_str()));
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
            SetState::Ready => self.switch_emoji_set(id, cx),
            SetState::Available(_) | SetState::Failed(_) => self.download_emoji_set(id, cx),
            SetState::Active | SetState::Loading { .. } => {}
        }
    }

    /// tdesktop `SwitchToSet`: load the pack off the UI thread, then draw
    /// with it. A pack that no longer loads is downloaded again.
    fn switch_emoji_set(&mut self, id: EmojiSetId, cx: &mut Context<Self>) {
        let Some(root) = app_root() else {
            return;
        };
        // The choice is saved off the UI thread too, once the pack loads.
        let load = cx.background_spawn(async move {
            let loaded = load_state(&root, Some(id));
            if loaded.current == id {
                let _ = save_current(&root, id);
            }
            loaded
        });
        self.settings.emoji_style.switch_task = Some(cx.spawn(async move |this, cx| {
            let loaded = load.await;
            let _ = this.update(cx, |this, cx| {
                if loaded.current == id {
                    this.apply_emoji_state(loaded, cx);
                } else {
                    this.settings.emoji_style.ready.remove(&id);
                    this.download_emoji_set(id, cx);
                }
            });
        }));
    }

    /// tdesktop `LoadAndSwitchTo`: one download at a time; a new one
    /// replaces the one in flight.
    fn download_emoji_set(&mut self, id: EmojiSetId, cx: &mut Context<Self>) {
        let Some(root) = app_root() else {
            return;
        };
        let state = &mut self.settings.emoji_style;
        if let Some(previous) = state.download.take() {
            previous.progress.cancel();
        }
        state.failed = None;
        state.file_error = None;
        let progress = Arc::new(Progress::default());
        state.download = Some(ActiveDownload {
            id,
            progress: progress.clone(),
        });
        let worker_progress = progress.clone();
        let worker = cx.background_spawn(async move {
            let result = quill::emoji_sets_style::download::download(&root, id, &worker_progress);
            worker_progress.finish();
            result
        });
        state.download_task = Some(cx.spawn(async move |this, cx| {
            while !progress.is_finished() {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                let state = &mut this.settings.emoji_style;
                if state.download.as_ref().is_some_and(|d| d.id == id) {
                    state.download = None;
                }
                match result {
                    Ok(()) => {
                        state.ready.insert(id);
                        this.switch_emoji_set(id, cx);
                    }
                    Err(message) if message == "Cancelled." => {}
                    Err(message) => state.failed = Some((id, message)),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// "Install from file…": a zip the user downloaded from the landing
    /// page (for machines that can't reach the release). Quill-only; the
    /// file's hash picks the set, then it is switched to like a download.
    fn install_emoji_set_file(&mut self, cx: &mut Context<Self>) {
        let Some(root) = app_root() else {
            self.settings.emoji_style.file_error =
                Some(tr(keys::Q_MEDIA_COULDNT_OPEN_QUILLS_DATA_FOLDER).into());
            cx.notify();
            return;
        };
        self.settings.emoji_style.file_error = None;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_MEDIA_INSTALL).into()),
        });
        self.settings.emoji_style.file_task = Some(cx.spawn(async move |this, cx| {
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
                        let state = &mut this.settings.emoji_style;
                        // The file wins over a download in flight, which
                        // would switch sets again when it lands.
                        if let Some(previous) = state.download.take() {
                            previous.progress.cancel();
                        }
                        state.ready.insert(id);
                        if state.failed.as_ref().is_some_and(|(f, _)| *f == id) {
                            state.failed = None;
                        }
                        this.switch_emoji_set(id, cx);
                    }
                    Err(message) => this.settings.emoji_style.file_error = Some(message),
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
                            .child(self.settings.emoji_style.current.name()),
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
                this.settings.emoji_style.open = false;
                cx.notify();
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
                    this.settings.emoji_style.file_error.clone(),
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
                        this.settings.emoji_style.open = false;
                        cx.notify();
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
        |app| app.settings.emoji_style.open,
        QuillApp::build_emoji_set_style_dialog,
    ),
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "emoji_style_tests.rs"]
mod tests;

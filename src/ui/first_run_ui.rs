//! The first-run onboarding, drawn from `QuillApp::first_run`
//! (`ui/first_run_state.rs`): the intro clip edge to edge, crossfading
//! into a notice that Quill is experimental, then three highlights, then
//! the sign-in screen. It shows once per install, never to someone who was
//! already signed in (`quill::settings::first_run_pending`), unless it is
//! replayed from Settings › Advanced, where it plays over the signed-in
//! app. A build made with `QUILL_ONBOARDING_EVERY_LAUNCH` set shows it on
//! every launch (for trying it out).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::first_run_state::{CARD_IN, FirstRun};
use super::motion::{ease_out_circ, elapsed_since, progress};
use super::video_player::{Purpose, VideoPlayer, video_frame};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{Key, keys, tr};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

static INTRO_MP4: &[u8] = include_bytes!("../../assets/onboarding/intro.mp4");
static LOGO_PNG: &[u8] = include_bytes!("../../assets/icons/hicolor/128x128/apps/quill.png");
/// The intro's length, shown until the file reports its own.
const INTRO_SECONDS: f64 = 6.04;
const CARD_WIDTH: f32 = 400.;
/// How far a card rises while it fades in.
const CARD_RISE: f32 = 12.;

const HIGHLIGHTS: [(IconName, Key, Key); 3] = [
    (
        IconName::CodeXml,
        keys::Q_FIRST_RUN_OPEN_TITLE,
        keys::Q_FIRST_RUN_OPEN_BODY,
    ),
    (
        IconName::Monitor,
        keys::Q_FIRST_RUN_NATIVE_TITLE,
        keys::Q_FIRST_RUN_NATIVE_BODY,
    ),
    (
        IconName::MessagesSquare,
        keys::Q_FIRST_RUN_TELEGRAM_TITLE,
        keys::Q_FIRST_RUN_TELEGRAM_BODY,
    ),
];

/// macOS keeps its traffic lights above the window's content, so the
/// intro covers the whole window. Elsewhere the in-window title bar holds
/// the window controls, and the intro fills the page below it.
const COVERS_TITLE_BAR: bool = cfg!(target_os = "macos");

/// Where the app starts: the onboarding on a first run, otherwise done.
/// Screenshot demos start done (they set their own step), and so does a
/// build without API credentials (its synthetic screen has no sign-in).
pub(super) fn initial_first_run(
    demo: bool,
    credentials: &Option<quill::credentials::TelegramCredentials>,
    cx: &mut Context<QuillApp>,
) -> FirstRun {
    let every_launch = option_env!("QUILL_ONBOARDING_EVERY_LAUNCH").is_some();
    let pending = !demo
        && credentials.is_some()
        && (every_launch
            || quill::settings::safe_app_root()
                .is_some_and(|root| quill::settings::first_run_pending(&root)));
    if !pending {
        return FirstRun::Done;
    }
    start_intro(cx)
}

/// The onboarding's welcome: the intro on black while its clip is hashed
/// and written out off the UI thread, then the clip playing with its
/// sound. With interface animations off, the notice at once.
pub(super) fn start_intro(cx: &mut Context<QuillApp>) -> FirstRun {
    let now = Instant::now();
    if super::motion::interface_animations_off() {
        return FirstRun::welcome(None, now);
    }
    let written = cx.background_spawn(async { intro_file() });
    cx.spawn(async move |this, cx| {
        let path = written.await;
        let _ = this.update(cx, |this, cx| {
            if !this.first_run.awaits_intro() {
                return;
            }
            let player = path.and_then(|path| open_intro(&path, cx));
            this.first_run.intro_written(player);
            cx.notify();
        });
    })
    .detach();
    FirstRun::writing_intro(now)
}

/// The intro clip at `path`, playing with its sound; `None` when it can't
/// be opened.
fn open_intro(path: &std::path::Path, cx: &mut Context<QuillApp>) -> Option<VideoPlayer> {
    let player = VideoPlayer::open(path, Purpose::Viewer, Some(INTRO_SECONDS), cx).ok()?;
    player.play();
    player.watch(cx.entity_id());
    Some(player)
}

/// The bundled clip as a file the player can open (the players read
/// files, not memory), written once into the app's own data folder under
/// a name that changes with its content. One write at a time: Replay
/// while the first write runs waits for it and finds the file written,
/// where a second rename onto the clip the player holds open would fail on
/// Windows.
fn intro_file() -> Option<PathBuf> {
    use std::io::Write;
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = quill::locks::lock_or_recover(&WRITING);
    let hash = INTRO_MP4
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        });
    let dir = quill::settings::safe_app_root()?.join("intro");
    let path = dir.join(format!("intro-{hash:016x}.mp4"));
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() == INTRO_MP4.len() as u64) {
        return Some(path);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let partial = dir.join(format!(
        "intro-{hash:016x}.{}-{}.part",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos())
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .ok()?;
    let written = file.write_all(INTRO_MP4).and_then(|()| file.sync_all());
    drop(file);
    match written
        .and_then(|()| quill::fs_retry::rename_retrying(&partial, &path, quill::fs_retry::SHORT))
    {
        Ok(()) => Some(path),
        Err(_) => {
            let _ = std::fs::remove_file(&partial);
            None
        }
    }
}

fn logo() -> Arc<Image> {
    static LOGO: OnceLock<Arc<Image>> = OnceLock::new();
    LOGO.get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())))
        .clone()
}

impl QuillApp {
    /// The onboarding in place of the sign-in screen, while it runs.
    pub(super) fn first_run_view(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = Instant::now();
        self.settle_first_run(now, cx);
        let (card, shown) = match &self.first_run {
            FirstRun::Welcome { .. } => (
                self.first_run_notice(cx),
                self.first_run.intro_look(now).card,
            ),
            FirstRun::Highlights { since } => (
                self.first_run_highlights(cx),
                progress(elapsed_since(*since, now), CARD_IN),
            ),
            FirstRun::Done => return None,
        };
        let eased = ease_out_circ(shown);
        let curtain = if COVERS_TITLE_BAR {
            None
        } else {
            self.first_run_curtain(cx)
        };
        Some(
            div()
                .size_full()
                .relative()
                .child(
                    div()
                        .id("first-run")
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .overflow_y_scroll()
                        .p_6()
                        .bg(cx.theme().background)
                        .child(
                            div()
                                .relative()
                                .top(px(CARD_RISE * (1. - eased)))
                                .opacity(eased)
                                .child(card),
                        ),
                )
                .children(curtain)
                .into_any_element(),
        )
    }

    /// The intro's cover where it spans the whole window (macOS).
    pub(super) fn first_run_window_cover(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if COVERS_TITLE_BAR {
            self.first_run_curtain(cx)
        } else {
            None
        }
    }

    /// The intro on black: it rises out of black, plays with its sound,
    /// sinks back to black, and the black lifts off the notice.
    fn first_run_curtain(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = Instant::now();
        self.settle_first_run(now, cx);
        if !matches!(self.first_run, FirstRun::Welcome { .. }) {
            return None;
        }
        let look = self.first_run.intro_look(now);
        if look.curtain <= 0. {
            return None;
        }
        let player = self.first_run.intro_player();
        if let Some(player) = player {
            player.set_volume(look.volume);
        }
        // A GPU surface ignores element opacity: the clip is darkened by
        // the veil over it, and drawn only while the cover is opaque.
        let clip = player.filter(|_| look.curtain >= 1.).map(|player| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .h_full()
                        .max_w_full()
                        .aspect_square()
                        .child(video_frame(player).object_fit(ObjectFit::Contain)),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(gpui_kit::black())
                        .opacity(look.veil),
                )
        });
        let mut cover = div()
            .id("first-run-curtain")
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui_kit::black())
            .opacity(look.curtain)
            // Escape reaches the app's key bindings through the cover
            // (the chat list that usually holds this focus is not drawn
            // before sign-in).
            .track_focus(&self.focus_sidebar)
            .children(clip);
        if look.skippable {
            cover = cover
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| this.skip_first_run_intro(cx)))
                // The whole cover skips; the label says so. Light on the
                // black cover whatever the theme.
                .child(
                    div()
                        .id("first-run-skip")
                        .role(Role::Button)
                        .aria_label(tr(keys::Q_OPS_SKIP))
                        .absolute()
                        .bottom(px(24.))
                        .right(px(24.))
                        .text_sm()
                        .text_color(gpui_kit::white().opacity(0.7))
                        .child(tr(keys::Q_OPS_SKIP)),
                );
        }
        Some(cover.into_any_element())
    }

    /// Bring the onboarding up to `now` before drawing it, and keep frames
    /// coming while a transition runs. Both draw paths call it, since the
    /// macOS cover is drawn in every pane mode; doing it twice is harmless.
    fn settle_first_run(&mut self, now: Instant, cx: &mut Context<Self>) {
        self.first_run.settle(now);
        if self.first_run.animating(now) {
            self.request_animation_tick(60, cx);
        }
    }

    /// Play the onboarding again from the start (Settings › Advanced): the
    /// settings dialog closes so nothing sits over it, and Esc reaches the
    /// intro through the focus its cover holds.
    pub(super) fn replay_first_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_settings();
        self.first_run = start_intro(cx);
        window.focus(&self.focus_sidebar, cx);
        cx.notify();
    }

    pub(super) fn skip_first_run_intro(&mut self, cx: &mut Context<Self>) {
        self.first_run.skip_intro(Instant::now());
        cx.notify();
    }

    fn finish_first_run(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = quill::settings::safe_app_root()
            && let Err(err) = quill::settings::mark_first_run_done(&root)
        {
            eprintln!("quill: could not save first_run.json: {err}");
        }
        self.first_run.finish();
        cx.notify();
    }

    fn first_run_notice(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        card()
            .child(img(ImageSource::Image(logo())).size(px(72.)))
            .child(heading(tr(keys::Q_FIRST_RUN_NOTICE_TITLE)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .text_center()
                    .child(paragraph(tr(keys::Q_FIRST_RUN_NOTICE_EARLY), muted))
                    .child(paragraph(tr(keys::Q_FIRST_RUN_NOTICE_RISK), muted)),
            )
            .child(
                div()
                    .w_full()
                    .debug_selector(|| "first-run-understand".into())
                    .child(
                        Button::new("first-run-understand")
                            .label(tr(keys::FROM_REQUEST_UNDERSTAND))
                            .primary()
                            .w_full()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.first_run.acknowledge(Instant::now());
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_center()
                    .text_color(muted)
                    .child(quill::about::INTRO_NOTICE),
            )
    }

    fn first_run_highlights(&self, cx: &mut Context<Self>) -> Div {
        let mut rows = div().w_full().flex().flex_col().gap_5();
        for (icon, title, body) in HIGHLIGHTS {
            rows = rows.child(highlight_row(icon, tr(title), tr(body), cx));
        }
        card()
            .child(img(ImageSource::Image(logo())).size(px(72.)))
            .child(heading(tr(keys::Q_FIRST_RUN_WELCOME)))
            .child(rows)
            .child(
                div()
                    .w_full()
                    .debug_selector(|| "first-run-start".into())
                    .child(
                        Button::new("first-run-start")
                            .label(tr(keys::START_MSGS))
                            .primary()
                            .w_full()
                            .on_click(cx.listener(|this, _, _, cx| this.finish_first_run(cx))),
                    ),
            )
    }
}

fn card() -> Div {
    div()
        .w(px(CARD_WIDTH))
        .max_w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap_6()
}

fn heading(text: &'static str) -> impl IntoElement {
    div()
        .id("first-run-title")
        .role(Role::Heading)
        .aria_label(text)
        .text_2xl()
        .font_weight(FontWeight::SEMIBOLD)
        .text_center()
        .child(text)
}

fn paragraph(text: &'static str, color: Hsla) -> Div {
    div().text_sm().text_color(color).child(text)
}

fn highlight_row(icon: IconName, title: &'static str, body: &'static str, cx: &App) -> Div {
    div()
        .flex()
        .items_start()
        .gap_4()
        .child(
            div()
                .flex_none()
                .size(px(40.))
                .rounded_lg()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().primary.opacity(0.12))
                .text_color(cx.theme().primary)
                .child(Icon::new(icon).size(px(20.))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(paragraph(body, cx.theme().muted_foreground)),
        )
}

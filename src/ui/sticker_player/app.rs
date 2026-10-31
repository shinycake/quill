//! Where the app asks for sticker views: history rows, dice, custom emoji
//! in text and the chat list, panels and effects.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::{CLOSE_SLACK, ClipSpec, DISPLAY, HeadId, StickerView, edge_for};
use futures_channel::oneshot;
use gpui_kit::*;
use quill::ids::{ChatId, FileId};
use quill::local_path::sandboxed_display_path;
use quill::locks::lock_or_recover;
use quill::power_saving;
use quill::sticker_playback::player::{ClipKey, PlayMode, Rest, grid_hz};
use quill::sticker_playback::{FitzType, Tint};
use quill::telegram::envelope::{DicePhase, StickerFormat};
use std::cell::Cell;
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Frames a second at most for small custom emoji (in text, the chat
/// list, statuses): the file's own rate, no in-between frames.
const EMOJI_MAX_FPS: u32 = 60;
/// For stickers and effects: the display's rate, up to 120.
const STICKER_MAX_FPS: u32 = 120;

/// The box of a custom emoji in text, in points, before paint measures it
/// (`message_text` draws them at 1.25 × the font, the composer at 1.1).
#[derive(Clone, Copy)]
pub(in crate::ui) struct EmojiSide(pub(in crate::ui) f32);

impl Default for EmojiSide {
    fn default() -> Self {
        Self(22.)
    }
}

/// A chat-list preview's custom emoji: stills (and emoji still opening)
/// as images, animated ones for the sidebar's animation layer.
#[derive(Default)]
pub(in crate::ui) struct PreviewEmoji {
    pub(in crate::ui) still: HashMap<i64, ImageSource>,
    pub(in crate::ui) layered: HashMap<i64, StickerView>,
}

/// What a view asks for.
#[derive(Clone, Copy)]
pub(in crate::ui) struct ViewAsk {
    pub(in crate::ui) file: FileId,
    pub(in crate::ui) format: StickerFormat,
    pub(in crate::ui) head: HeadId,
    pub(in crate::ui) mode: PlayMode,
    /// The box it is drawn in, in points.
    pub(in crate::ui) side: f32,
    pub(in crate::ui) max_fps: u32,
    pub(in crate::ui) fitz: FitzType,
    pub(in crate::ui) tint: Option<Tint>,
    pub(in crate::ui) mirror: bool,
}

impl ViewAsk {
    /// A sticker or effect: up to 120 frames a second.
    pub(in crate::ui) fn sticker(
        file: FileId,
        format: StickerFormat,
        head: HeadId,
        mode: PlayMode,
        side: f32,
    ) -> Self {
        Self {
            file,
            format,
            head,
            mode,
            side,
            max_fps: STICKER_MAX_FPS,
            fitz: FitzType::None,
            tint: None,
            mirror: false,
        }
    }

    /// A small custom emoji: at most the file's own rate.
    pub(in crate::ui) fn emoji(
        file: FileId,
        format: StickerFormat,
        head: HeadId,
        mode: PlayMode,
        side: f32,
    ) -> Self {
        Self {
            max_fps: EMOJI_MAX_FPS,
            ..Self::sticker(file, format, head, mode, side)
        }
    }

    pub(in crate::ui) fn mirrored(self, mirror: bool) -> Self {
        Self { mirror, ..self }
    }

    pub(in crate::ui) fn tinted(self, tint: Option<Tint>) -> Self {
        Self { tint, ..self }
    }

    /// Its clip at the size its box has on the last painted window, on
    /// the frame grid for that window's display. A WebM clip keeps its own
    /// frames, and a view capped at 60 frames a second never steps finer.
    pub(super) fn key(&self) -> ClipKey {
        let (scale, display_hz) = DISPLAY.with(Cell::get);
        let hz = if self.format == StickerFormat::Webm || self.max_fps <= 60 {
            grid_hz(0.0)
        } else {
            grid_hz(display_hz).min(self.max_fps)
        };
        ClipKey {
            file: self.file.0,
            edge: edge_for(self.side * scale),
            fitz: self.fitz,
            tint: self.tint,
            mirror: self.mirror,
            hz,
        }
    }
}

/// `tdesktop Sticker::mirrorHorizontal`: a Premium sticker (one with a
/// Premium effect) that is not right-aligned faces its mirrored effect.
pub(in crate::ui) fn mirrors(has_premium_effect: bool, right_aligned: bool) -> bool {
    has_premium_effect && !right_aligned
}

impl QuillApp {
    /// Close every clip and forget every head and every lone emoji played:
    /// another chat opened, the chat closed, another account.
    pub(in crate::ui) fn stop_sticker_playback(&mut self) {
        self.playback.stickers.stop();
        self.playback.emoji_plays.get_mut().clear();
    }

    /// Whether looping is on (Settings → Chat → "Loop animated stickers").
    fn loop_stickers(&self) -> bool {
        self.session()
            .is_none_or(|s| s.settings.media_prefs.loop_animated_stickers)
    }

    /// How a sticker in a chat plays: looping, or once resting on its last
    /// frame.
    pub(in crate::ui) fn sticker_mode(&self) -> PlayMode {
        if self.loop_stickers() {
            PlayMode::Loop
        } else {
            PlayMode::Once(Rest::Last)
        }
    }

    /// The local file of `file`, downloading it when missing (unless data
    /// saving holds it back).
    fn sticker_path(&self, file: FileId, cx: &mut Context<QuillApp>) -> Option<std::path::PathBuf> {
        let session = self.session()?;
        let path = session
            .media
            .files
            .get(&file.0)
            .and_then(|f| f.usable_path())
            .and_then(|p| sandboxed_display_path(p, &self.media_display_roots()));
        if path.is_none()
            && !session.settings.media_prefs.data_saver
            && session.should_download(file)
        {
            // Asked for after this frame: rendering borrows the app.
            let weak = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, _| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.download_file(file, 1);
                    }
                });
            });
        }
        path
    }

    /// A view of `ask`, once a frame of it can show. `None` while the file
    /// downloads, the clip opens, or when it failed: the caller shows the
    /// still meanwhile. A view drawn outside any animation layer asks the
    /// frame clock for its ticks here.
    pub(in crate::ui) fn sticker_view(
        &self,
        ask: ViewAsk,
        cx: &mut Context<QuillApp>,
    ) -> Option<StickerView> {
        if !matches!(ask.format, StickerFormat::Tgs | StickerFormat::Webm) || ask.file.0 == 0 {
            return None;
        }
        #[cfg(test)]
        let path = if self.playback.stickers.has_fixture(ask.file.0) {
            Some(std::path::PathBuf::new())
        } else {
            self.sticker_path(ask.file, cx)
        };
        #[cfg(not(test))]
        let path = self.sticker_path(ask.file, cx);
        let path = path?;
        self.ensure_sticker_pump(cx);
        let key = ask.key();
        let spec = ClipSpec {
            file: ask.file,
            format: ask.format,
            path,
            fitz: ask.fitz,
            tint: ask.tint,
            mirror: ask.mirror,
            hz: key.hz,
        };
        let edge = key.edge;
        let player = &self.playback.stickers;
        let ready = player.prepare(&spec, edge);
        self.ensure_sticker_closer(cx);
        if !ready {
            return None;
        }
        let view = StickerView {
            player: player.clone(),
            spec: Rc::new(spec),
            head: ask.head,
            mode: ask.mode,
            edge,
            max_fps: ask.max_fps,
            active: self.frame.window_active.get() || super::super::frame_clock::assume_active(),
            window: self.windows.scope,
        };
        if super::super::anim_layer::current().is_none() {
            let fps = view.fps();
            if fps > 0 {
                self.request_animation_tick(fps, cx);
            }
        }
        Some(view)
    }

    /// [`Self::sticker_view`] when its clip is already open (at any size);
    /// `None` without opening it otherwise.
    pub(in crate::ui) fn opened_sticker_view(
        &self,
        ask: ViewAsk,
        cx: &mut Context<QuillApp>,
    ) -> Option<StickerView> {
        if !self.playback.stickers.has_clip_like(&ask.key()) {
            return None;
        }
        self.sticker_view(ask, cx)
    }

    /// Whether the clip `ask` names failed to open.
    #[cfg(feature = "demo-capture")]
    pub(in crate::ui) fn sticker_failed(&self, ask: ViewAsk) -> bool {
        self.playback.stickers.failed_key(&ask.key())
    }

    /// A window starts a frame, before anything in it renders (`None`: the
    /// main window).
    pub(in crate::ui) fn sticker_frame_starts(
        &self,
        window: Option<ChatId>,
        cx: &mut Context<QuillApp>,
    ) {
        self.playback.stickers.frame_starts(window);
        self.ensure_sticker_closer(cx);
    }

    /// Starts the task closing clips once no window shows them, while any
    /// is open. Before it, clips closed only while some view painted, so
    /// with nothing animating (the emoji panel closed over a chat of text)
    /// every clip stayed open with its frames and canvas. It looks between
    /// frames, after the next frame of any window or when the next clip
    /// off screen is due (each plus `CLOSE_SLACK`), and ends once no clip
    /// is open.
    fn ensure_sticker_closer(&self, cx: &mut Context<QuillApp>) {
        let player = &self.playback.stickers;
        if player.closer.get() || !player.has_clips() {
            return;
        }
        player.closer.set(true);
        cx.spawn(async move |this, cx| {
            loop {
                let wait = this.update(cx, |this, cx| {
                    let player = this.playback.stickers.clone();
                    let closed = player.close_stale(player.now());
                    drop_frames(closed.frames, cx);
                    this.schedule_heap_release(cx);
                    player.trace_closed(closed.clips);
                    if !closed.open {
                        player.closer.set(false);
                        return None;
                    }
                    Some((closed.next, player.next_frame()))
                });
                let Ok(Some((due, next_frame))) = wait else {
                    break;
                };
                let executor = cx.background_executor().clone();
                let after_frame = async {
                    let _ = next_frame.await;
                    executor.timer(CLOSE_SLACK).await;
                };
                match due {
                    Some(due) => first_of(after_frame, executor.timer(due + CLOSE_SLACK)).await,
                    None => after_frame.await,
                }
            }
        })
        .detach();
    }

    /// Starts the task that redraws the app when a clip opened or its first
    /// frame is ready (views then switch from the still to the animation).
    fn ensure_sticker_pump(&self, cx: &mut Context<QuillApp>) {
        let player = &self.playback.stickers;
        // Tests run GPUI's deterministic scheduler, which a wake from a
        // worker thread would trip; they take frames with `Player::settle`.
        if cfg!(test) || player.pump.replace(true) {
            return;
        }
        // A wake while the task is not waiting is kept in `pending`; one
        // while it waits fires the channel it waits on.
        let pending = Arc::new(AtomicBool::new(false));
        let slot: Arc<Mutex<Option<oneshot::Sender<()>>>> = Arc::new(Mutex::new(None));
        player.engine().set_waker(Arc::new({
            let pending = pending.clone();
            let slot = slot.clone();
            move || {
                pending.store(true, Ordering::Release);
                if let Some(tx) = lock_or_recover(&slot).take() {
                    let _ = tx.send(());
                }
            }
        }));
        // window-scope: the player is shared by every window; a wake
        // redraws the app, which redraws them all.
        cx.spawn(async move |this, cx| {
            loop {
                let (tx, rx) = oneshot::channel::<()>();
                *lock_or_recover(&slot) = Some(tx);
                if !pending.swap(false, Ordering::AcqRel) && rx.await.is_err() {
                    break;
                }
                pending.store(false, Ordering::Release);
                let alive = this.update(cx, |this, cx| {
                    this.playback.stickers.drain();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// A history row's animated sticker, `side` points across, with its
    /// own head (it plays once per row when looping is off).
    pub(in crate::ui) fn history_sticker(
        &self,
        message: &quill::state::HistoryMessage,
        sticker: &quill::telegram::envelope::StickerContent,
        mirror: bool,
        side: f32,
        cx: &mut Context<QuillApp>,
    ) -> Option<StickerView> {
        // Battery and animations: stickers in messages stay still (the
        // row shows the preview image).
        if power_saving::on(power_saving::Flag::StickersChat) {
            return None;
        }
        let head = HeadId::of(("history", message.chat_id.0, message.id.0));
        let ask = ViewAsk::sticker(
            sticker.file_id,
            sticker.format,
            head,
            self.sticker_mode(),
            side,
        )
        .mirrored(mirror);
        self.sticker_view(ask, cx)
    }

    /// Layer `index` of a dice: the roll loops and the landing plays once
    /// and rests on its last frame (the rolled value), whatever the loop
    /// setting says. A dice always plays (tdesktop: not a sticker autoplay).
    pub(in crate::ui) fn history_dice_sticker(
        &self,
        message: &quill::state::HistoryMessage,
        index: usize,
        sticker: &quill::telegram::envelope::StickerContent,
        phase: DicePhase,
        cx: &mut Context<QuillApp>,
    ) -> Option<StickerView> {
        let (mode, phase_id) = match phase {
            DicePhase::Rolling => (PlayMode::Loop, 0u8),
            DicePhase::Landing => (PlayMode::Once(Rest::Last), 1),
        };
        let head = HeadId::of(("dice", message.chat_id.0, message.id.0, index, phase_id));
        let side = quill::bubble_layout::DICE_SIZE as f32;
        self.sticker_view(
            ViewAsk::sticker(sticker.file_id, sticker.format, head, mode, side),
            cx,
        )
    }

    /// Views of the custom emoji in `message`'s text, by custom emoji id.
    /// Emoji still opening (or static ones) are absent; the text shows
    /// their still meanwhile.
    pub(in crate::ui) fn message_custom_emoji_frames(
        &self,
        message: &quill::state::HistoryMessage,
        cx: &mut Context<QuillApp>,
    ) -> HashMap<i64, StickerView> {
        use quill::telegram::envelope::MessageContent;
        use quill::text::TextEntityKind;
        let mut out = HashMap::new();
        let MessageContent::Text(text) = &message.content else {
            return out;
        };
        // Custom emoji drawn big are stickers of their own size.
        if let Some(big) = self.big_custom_emoji_views(message, cx) {
            return big;
        }
        if power_saving::on(power_saving::Flag::EmojiChat) {
            return out;
        }
        let Some(session) = self.session() else {
            return out;
        };
        let wanted: Vec<_> = text
            .entities
            .iter()
            .filter_map(|entity| match entity.kind {
                TextEntityKind::CustomEmoji { custom_emoji_id } => Some(custom_emoji_id),
                _ => None,
            })
            .filter_map(|id| {
                session
                    .stickers
                    .emoji
                    .custom_emoji_stickers
                    .iter()
                    .find(|item| item.custom_emoji_id == Some(id))
                    .map(|item| (id, item.file_id, item.format))
            })
            .collect();
        let mode = self.sticker_mode();
        for (id, file, format) in wanted {
            if out.contains_key(&id) {
                continue;
            }
            let head = HeadId::of(("message-emoji", message.chat_id.0, message.id.0, id));
            let side = EmojiSide::default().0;
            if let Some(view) =
                self.sticker_view(ViewAsk::emoji(file, format, head, mode, side), cx)
            {
                out.insert(id, view);
            }
        }
        out
    }

    /// Images for the custom emoji in a chat-list preview: animated once
    /// open, the still meanwhile. Built inside the chat list slice, the
    /// animated ones are painted by its animation layer (`anim_layer`), so
    /// their ticks don't rebuild the list.
    pub(in crate::ui) fn preview_emoji_images(
        &self,
        chat: &quill::state::ChatSummary,
        cx: &mut Context<QuillApp>,
    ) -> PreviewEmoji {
        self.sidebar_emoji_images(&chat.last_preview_style.entities, cx)
    }

    /// [`Self::preview_emoji_images`] for the custom emoji a forum row shows
    /// as topic icons.
    pub(in crate::ui) fn topic_icon_images(
        &self,
        icons: &[i64],
        cx: &mut Context<QuillApp>,
    ) -> PreviewEmoji {
        let entities: Vec<quill::text::TextEntity> = icons
            .iter()
            .map(|id| quill::text::TextEntity {
                utf8_start: 0,
                utf8_end: 0,
                kind: quill::text::TextEntityKind::CustomEmoji {
                    custom_emoji_id: *id,
                },
            })
            .collect();
        self.sidebar_emoji_images(&entities, cx)
    }

    fn sidebar_emoji_images(
        &self,
        entities: &[quill::text::TextEntity],
        cx: &mut Context<QuillApp>,
    ) -> PreviewEmoji {
        if !self.slices.in_sidebar()
            || super::super::anim_layer::current().is_none()
            || power_saving::on(power_saving::Flag::EmojiChat)
        {
            return PreviewEmoji {
                still: self.custom_emoji_images(entities, EmojiSide(14.), cx),
                layered: HashMap::new(),
            };
        }
        use quill::text::TextEntityKind;
        let mut out = PreviewEmoji::default();
        let Some(session) = self.session() else {
            return out;
        };
        let roots = self.media_display_roots();
        let mode = self.sticker_mode();
        for entity in entities {
            let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                continue;
            };
            let Some(item) = session
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(custom_emoji_id))
            else {
                continue;
            };
            let head = HeadId::of(("sidebar-emoji", custom_emoji_id));
            let ask = ViewAsk::emoji(item.file_id, item.format, head, mode, 14.);
            if let Some(view) = self.sticker_view(ask, cx) {
                out.layered.insert(custom_emoji_id, view);
                continue;
            }
            let still = item
                .display_file_id()
                .and_then(|file| session.media.files.get(&file.0))
                .and_then(|file| file.usable_path())
                .and_then(|path| sandboxed_display_path(path, &roots))
                .map(ImageSource::from);
            if let Some(still) = still {
                out.still.insert(custom_emoji_id, still);
            }
        }
        out
    }

    /// Images for the custom emoji among `entities`, `side` points across:
    /// the frame due now when animated, the still meanwhile; unresolved
    /// ones are absent.
    pub(in crate::ui) fn custom_emoji_images(
        &self,
        entities: &[quill::text::TextEntity],
        side: EmojiSide,
        cx: &mut Context<QuillApp>,
    ) -> HashMap<i64, ImageSource> {
        use quill::text::TextEntityKind;
        let mut out = HashMap::new();
        let Some(session) = self.session() else {
            return out;
        };
        let roots = self.media_display_roots();
        for entity in entities {
            let TextEntityKind::CustomEmoji { custom_emoji_id } = entity.kind else {
                continue;
            };
            let Some(item) = session
                .stickers
                .emoji
                .custom_emoji_stickers
                .iter()
                .find(|item| item.custom_emoji_id == Some(custom_emoji_id))
            else {
                continue;
            };
            let source = (!power_saving::on(power_saving::Flag::EmojiChat))
                .then(|| self.custom_emoji_image(item.file_id, item.format, side.0, cx))
                .flatten()
                .map(ImageSource::from)
                .or_else(|| {
                    item.display_file_id()
                        .and_then(|file| session.media.files.get(&file.0))
                        .and_then(|file| file.usable_path())
                        .and_then(|path| sandboxed_display_path(path, &roots))
                        .map(ImageSource::from)
                });
            if let Some(source) = source {
                out.insert(custom_emoji_id, source);
            }
        }
        out
    }

    /// The frame due now of a custom emoji `side` points across (small,
    /// many on screen), for an `img`.
    pub(in crate::ui) fn custom_emoji_image(
        &self,
        id: FileId,
        format: StickerFormat,
        side: f32,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        let head = HeadId::of(("emoji-image", id.0));
        let mode = self.sticker_mode();
        self.sticker_view(ViewAsk::emoji(id, format, head, mode, side), cx)?
            .frame_now()
    }

    /// [`Self::custom_emoji_image`] for a cell that is built but not on
    /// screen (list overdraw): the first frame, no ticks.
    pub(in crate::ui) fn custom_emoji_image_parked(
        &self,
        id: FileId,
        format: StickerFormat,
        side: f32,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        let head = HeadId::of(("emoji-image", id.0));
        let at = PlayMode::At(Duration::ZERO);
        self.sticker_view(ViewAsk::emoji(id, format, head, at, side), cx)?
            .frame_now()
    }

    /// The frame due now of a sticker in a panel cell `side` points across.
    pub(in crate::ui) fn sticker_image(
        &self,
        id: FileId,
        format: StickerFormat,
        side: f32,
        cx: &mut Context<QuillApp>,
    ) -> Option<Arc<RenderImage>> {
        // Battery and animations: the sticker panel shows stills.
        if power_saving::on(power_saving::Flag::StickersPanel) {
            return None;
        }
        let head = HeadId::of(("panel-sticker", id.0));
        let mode = self.sticker_mode();
        self.sticker_view(ViewAsk::sticker(id, format, head, mode, side), cx)?
            .frame_now()
    }
}

/// Free the frames of clips the closer closed. No window shows them, so
/// each one nothing else holds leaves every window's atlas and memory now:
/// with no frame drawn, the sweep of retired images (`image_budget::sweep`)
/// never runs. One still held elsewhere waits for that sweep.
fn drop_frames(frames: Vec<Arc<RenderImage>>, cx: &mut App) {
    let mut held = Vec::new();
    for image in frames {
        if Arc::strong_count(&image) == 1 {
            super::super::image_budget::note_dropped(&image);
            cx.drop_image(image, None);
        } else {
            held.push(image);
        }
    }
    super::super::image_budget::retire_all(held);
}

/// Wait until `a` or `b` is done.
async fn first_of(a: impl Future<Output = ()>, b: impl Future<Output = ()>) {
    let (mut a, mut b) = (std::pin::pin!(a), std::pin::pin!(b));
    std::future::poll_fn(|cx| {
        if a.as_mut().poll(cx).is_ready() || b.as_mut().poll(cx).is_ready() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
}

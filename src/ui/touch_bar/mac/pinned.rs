//! The pinned chats panel (tdesktop `PinnedDialogsPanel`,
//! `items/mac_pinned_chats_item.mm`): a row of round pictures, a touch
//! opens the chat (or the archive), and a sideways drag of a pinned chat
//! reorders the pinned chats, the others sliding aside.
//!
//! Like tdesktop's, it is an `NSImageView` whose empty image sets the
//! width, drawing the pictures itself.

use super::super::model::{
    self, CIRCLE_DIAMETER, Command, Pin, PinKind, drop_index, index_at, pin_x,
};
use super::super::{Palette, initials, initials_colors};
use super::draw;
use objc2::rc::{Retained, Weak};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSApplication, NSFont, NSGestureRecognizerState, NSImage, NSImageView,
    NSPressGestureRecognizer, NSStringDrawing, NSTouchTypeMask, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSTimer};
use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::time::{Duration, Instant};

/// tdesktop `st::slideWrapDuration` and `st::dialogsOnlineBadgeDuration`.
const ANIMATION: Duration = Duration::from_millis(150);
/// Qt's start drag distance: a touch that moves less is a tap.
const DRAG_START: f64 = 10.;
/// tdesktop `kOnlineCircleSize`, `kOnlineCircleStrokeWidth`.
const ONLINE_SIZE: f64 = 8.;
const ONLINE_STROKE: f64 = 1.5;
/// tdesktop `kUnreadBadgeSize`.
const BADGE_HEIGHT: f64 = 15.;
/// tdesktop: the badge text's side padding.
const BADGE_PADDING: f64 = 4.;

/// A value sliding from one target to the next.
#[derive(Clone, Copy, Debug)]
struct Slide {
    from: f64,
    to: f64,
    started: Option<Instant>,
}

impl Slide {
    fn at(value: f64) -> Self {
        Self {
            from: value,
            to: value,
            started: None,
        }
    }

    fn value(&self, now: Instant) -> f64 {
        let Some(started) = self.started else {
            return self.to;
        };
        let t = (now.saturating_duration_since(started).as_secs_f64() / ANIMATION.as_secs_f64())
            .clamp(0., 1.);
        // Ease out, like tdesktop's default transition.
        let eased = 1. - (1. - t) * (1. - t);
        self.from + (self.to - self.from) * eased
    }

    fn animating(&self, now: Instant) -> bool {
        self.started
            .is_some_and(|started| now.saturating_duration_since(started) < ANIMATION)
    }

    fn go(&mut self, to: f64, now: Instant, animate: bool) {
        if (self.to - to).abs() < f64::EPSILON {
            return;
        }
        self.from = if animate { self.value(now) } else { to };
        self.to = to;
        self.started = animate.then_some(now);
    }
}

struct Entry {
    pin: Pin,
    shift: Slide,
    online: Slide,
}

/// A drag in progress.
struct Drag {
    index: usize,
    start_x: f64,
    x: f64,
    started: bool,
}

#[derive(Default)]
pub(super) struct Ivars {
    entries: RefCell<Vec<Entry>>,
    palette: Cell<Option<Palette>>,
    touch: RefCell<Option<Drag>>,
    timer: RefCell<Option<Retained<NSTimer>>>,
}

define_class!(
    #[unsafe(super(NSImageView))]
    #[thread_kind = MainThreadOnly]
    #[name = "QuillTouchBarPinnedPanel"]
    #[ivars = Ivars]
    pub(super) struct PinnedPanel;

    impl PinnedPanel {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw_pins();
        }

        #[unsafe(method(touchGesture:))]
        fn touch_gesture(&self, gesture: &NSPressGestureRecognizer) {
            let x = gesture.locationInView(Some(self)).x;
            let state = gesture.state();
            if state == NSGestureRecognizerState::Began {
                self.touch_began(x);
            } else if state == NSGestureRecognizerState::Changed {
                self.touch_moved(x);
            } else if state == NSGestureRecognizerState::Ended {
                self.touch_ended(x, true);
            } else if state == NSGestureRecognizerState::Cancelled
                || state == NSGestureRecognizerState::Failed
            {
                self.touch_ended(x, false);
            }
        }
    }
);

impl PinnedPanel {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: `init` of NSImageView (NSObject's designated chain).
        let this: Retained<Self> = unsafe { msg_send![super(this), init] };
        // SAFETY: the target outlives its recognizer (the recognizer
        // belongs to this view), and `touchGesture:` takes the recognizer.
        let gesture = unsafe {
            NSPressGestureRecognizer::initWithTarget_action(
                NSPressGestureRecognizer::alloc(mtm),
                Some(&this),
                Some(sel!(touchGesture:)),
            )
        };
        gesture.setAllowedTouchTypes(NSTouchTypeMask::Direct);
        gesture.setMinimumPressDuration(0.);
        gesture.setAllowableMovement(0.);
        this.addGestureRecognizer(&gesture);
        this
    }

    /// Show `pins`. A change of the pinned order cancels a drag (tdesktop
    /// `pinnedDialogsOrderUpdated`); an online change animates the dot
    /// while the app is active.
    pub(super) fn set_pins(&self, pins: &[Pin], palette: Palette) {
        let now = Instant::now();
        let mtm = MainThreadMarker::from(self);
        let active = NSApplication::sharedApplication(mtm).isActive();
        let same_order = {
            let Ok(entries) = self.ivars().entries.try_borrow() else {
                return;
            };
            entries.len() == pins.len()
                && entries.iter().zip(pins).all(|(entry, pin)| {
                    entry.pin.chat_id == pin.chat_id && entry.pin.kind == pin.kind
                })
        };
        let resize = {
            let Ok(mut entries) = self.ivars().entries.try_borrow_mut() else {
                return;
            };
            let resize = entries.len() != pins.len();
            if same_order {
                for (entry, pin) in entries.iter_mut().zip(pins) {
                    if entry.pin.online != pin.online {
                        let to = if pin.online { 1. } else { 0. };
                        entry.online.go(to, now, active);
                    }
                    entry.pin = pin.clone();
                }
            } else {
                *entries = pins
                    .iter()
                    .map(|pin| Entry {
                        pin: pin.clone(),
                        shift: Slide::at(0.),
                        online: Slide::at(if pin.online { 1. } else { 0. }),
                    })
                    .collect();
            }
            resize
        };
        if !same_order && let Ok(mut touch) = self.ivars().touch.try_borrow_mut() {
            *touch = None;
        }
        self.ivars().palette.set(Some(palette));
        if resize {
            let width = model::panel_width(pins.len()).max(1.);
            let image =
                NSImage::initWithSize(NSImage::alloc(), NSSize::new(width, CIRCLE_DIAMETER));
            self.setImage(Some(&image));
        }
        NSView::setNeedsDisplay(self, true);
        self.animate_if_needed();
    }

    fn count(&self) -> usize {
        self.ivars()
            .entries
            .try_borrow()
            .map(|entries| entries.len())
            .unwrap_or(0)
    }

    /// The first slot a pinned chat may move to: the archive and an
    /// unpinned Saved Messages stay in front.
    fn first_reorderable(&self) -> usize {
        self.ivars()
            .entries
            .try_borrow()
            .ok()
            .and_then(|entries| entries.iter().position(|entry| entry.pin.reorderable()))
            .unwrap_or(usize::MAX)
    }

    fn touch_began(&self, x: f64) {
        let index = index_at(x, self.count());
        if let Ok(mut touch) = self.ivars().touch.try_borrow_mut() {
            *touch = index.map(|index| Drag {
                index,
                start_x: x,
                x,
                started: false,
            });
        }
    }

    fn touch_moved(&self, x: f64) {
        let first = self.first_reorderable();
        let count = self.count();
        let Ok(mut touch) = self.ivars().touch.try_borrow_mut() else {
            return;
        };
        let Some(drag) = touch.as_mut() else {
            return;
        };
        drag.x = x;
        // Only pinned chats move, and only with another one to swap with.
        let movable = drag.index >= first && count.saturating_sub(first) >= 2;
        if !drag.started {
            if !movable || (x - drag.start_x).abs() < DRAG_START {
                return;
            }
            drag.started = true;
        }
        let (index, shift) = (drag.index, x - drag.start_x);
        drop(touch);
        self.layout_drag(index, shift, first, count);
    }

    /// The dragged picture follows the touch; the others step aside.
    fn layout_drag(&self, index: usize, shift: f64, first: usize, count: usize) {
        let now = Instant::now();
        let target = drop_index(index, shift, first, count);
        let pitch = CIRCLE_DIAMETER + model::PINNED_SPACE;
        if let Ok(mut entries) = self.ivars().entries.try_borrow_mut() {
            for (i, entry) in entries.iter_mut().enumerate() {
                if i == index {
                    entry.shift = Slide::at(shift);
                } else if index < target && i > index && i <= target {
                    entry.shift.go(-pitch, now, true);
                } else if target < index && i >= target && i < index {
                    entry.shift.go(pitch, now, true);
                } else {
                    entry.shift.go(0., now, true);
                }
            }
        }
        NSView::setNeedsDisplay(self, true);
        self.animate_if_needed();
    }

    fn touch_ended(&self, x: f64, ended: bool) {
        let drag = self
            .ivars()
            .touch
            .try_borrow_mut()
            .ok()
            .and_then(|mut touch| touch.take());
        let Some(drag) = drag else {
            return;
        };
        if drag.started {
            self.finish_drag(&drag, x);
        } else if ended && (x - drag.start_x).abs() < DRAG_START {
            self.open(drag.index);
        }
    }

    /// Drop the dragged chat into its slot and save the order.
    fn finish_drag(&self, drag: &Drag, x: f64) {
        let now = Instant::now();
        let first = self.first_reorderable();
        let count = self.count();
        let target = drop_index(drag.index, x - drag.start_x, first, count);
        let pitch = CIRCLE_DIAMETER + model::PINNED_SPACE;
        let mut ids = None;
        if let Ok(mut entries) = self.ivars().entries.try_borrow_mut() {
            let pins: Vec<Pin> = entries.iter().map(|entry| entry.pin.clone()).collect();
            ids = model::reordered_ids(&pins, drag.index, target);
            if target != drag.index && drag.index < entries.len() && target < entries.len() {
                // The dragged picture lands from where the touch left it.
                let landed = (x - drag.start_x) - (target as f64 - drag.index as f64) * pitch;
                let moved = entries.remove(drag.index);
                entries.insert(target, moved);
                for (i, entry) in entries.iter_mut().enumerate() {
                    if i == target {
                        entry.shift = Slide::at(landed);
                    } else {
                        entry.shift = Slide::at(0.);
                    }
                }
            }
            for entry in entries.iter_mut() {
                entry.shift.go(0., now, true);
            }
        }
        NSView::setNeedsDisplay(self, true);
        self.animate_if_needed();
        if let Some(ids) = ids {
            super::send(Command::ReorderPinned(ids));
        }
    }

    /// tdesktop `performAction`: the archive opens its folder, Saved
    /// Messages your own chat, the rest their chat.
    fn open(&self, index: usize) {
        let pin = self
            .ivars()
            .entries
            .try_borrow()
            .ok()
            .and_then(|entries| entries.get(index).map(|entry| entry.pin.clone()));
        let Some(pin) = pin else {
            return;
        };
        let command = match (pin.kind, pin.chat_id) {
            (PinKind::Archive, _) => Command::OpenArchive,
            (PinKind::SavedMessages, _) => Command::OpenSavedMessages,
            (PinKind::Chat, Some(chat_id)) => Command::OpenChat(chat_id),
            (PinKind::Chat, None) => return,
        };
        super::send(command);
    }

    /// Redraw on every frame while something slides.
    fn animate_if_needed(&self) {
        let now = Instant::now();
        let animating = self.ivars().entries.try_borrow().is_ok_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.shift.animating(now) || entry.online.animating(now))
        });
        let Ok(mut timer) = self.ivars().timer.try_borrow_mut() else {
            return;
        };
        if !animating || timer.is_some() {
            return;
        }
        let weak = Weak::from_retained(&self.retain());
        let block = block2::RcBlock::new(move |timer: NonNull<NSTimer>| {
            let Some(view) = weak.load() else {
                // SAFETY: the timer passed to its own block is alive.
                unsafe { timer.as_ref() }.invalidate();
                return;
            };
            NSView::setNeedsDisplay(&view, true);
            let now = Instant::now();
            let done = view.ivars().entries.try_borrow().is_ok_and(|entries| {
                !entries
                    .iter()
                    .any(|entry| entry.shift.animating(now) || entry.online.animating(now))
            });
            if done {
                // SAFETY: as above.
                unsafe { timer.as_ref() }.invalidate();
                if let Ok(mut slot) = view.ivars().timer.try_borrow_mut() {
                    *slot = None;
                }
            }
        });
        // SAFETY: a main-thread timer on the main run loop; the block only
        // touches main-thread objects and runs there.
        let scheduled = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(1. / 60., true, &block)
        };
        *timer = Some(scheduled);
    }

    fn draw_pins(&self) {
        let Some(palette) = self.ivars().palette.get() else {
            return;
        };
        let Ok(entries) = self.ivars().entries.try_borrow() else {
            return;
        };
        let now = Instant::now();
        let dragged = self.ivars().touch.try_borrow().ok().and_then(|touch| {
            touch
                .as_ref()
                .filter(|drag| drag.started)
                .map(|drag| drag.index)
        });
        // The dragged picture is drawn last, over the others.
        for (i, entry) in entries.iter().enumerate() {
            if Some(i) != dragged {
                draw_pin(entry, i, now, palette);
            }
        }
        if let Some(i) = dragged
            && let Some(entry) = entries.get(i)
        {
            draw_pin(entry, i, now, palette);
        }
    }
}

fn draw_pin(entry: &Entry, index: usize, now: Instant, palette: Palette) {
    let x = pin_x(index) + entry.shift.value(now);
    let rect = NSRect::new(
        NSPoint::new(x, 0.),
        NSSize::new(CIRCLE_DIAMETER, CIRCLE_DIAMETER),
    );
    let pin = &entry.pin;
    match pin.kind {
        PinKind::Archive => {
            draw::fill_oval(rect, &draw::color(palette.archive));
            draw::draw_symbol(
                "archivebox.fill",
                rect,
                CIRCLE_DIAMETER * 0.45,
                &draw::color(palette.on_fill),
            );
        }
        PinKind::SavedMessages => {
            draw::fill_oval(rect, &draw::color(palette.accent));
            draw::draw_symbol(
                "bookmark.fill",
                rect,
                CIRCLE_DIAMETER * 0.45,
                &draw::color(palette.on_fill),
            );
        }
        PinKind::Chat => match pin.photo.as_deref().and_then(draw::file_image) {
            Some(photo) => draw::draw_round(&photo, rect),
            None => {
                let (bg, fg) = initials_colors(&pin.title);
                draw::fill_oval(rect, &draw::color(bg));
                draw::draw_centered(
                    &initials(&pin.title),
                    rect,
                    &NSFont::boldSystemFontOfSize(CIRCLE_DIAMETER * 0.4),
                    &draw::color(fg),
                );
            }
        },
    }
    let right = x + CIRCLE_DIAMETER;
    if let Some(badge) = &pin.badge {
        draw_badge(badge, right, palette);
        return;
    }
    let online = entry.online.value(now);
    if online < 0.05 {
        return;
    }
    // tdesktop: a green disc ringed in black at the bottom-right that
    // grows and shrinks.
    let side = online * ONLINE_SIZE;
    let diff = (ONLINE_SIZE - side) / 2.;
    let dot = NSRect::new(
        NSPoint::new(right - ONLINE_SIZE + diff - ONLINE_STROKE / 2., diff),
        NSSize::new(side, side),
    );
    let path = objc2_app_kit::NSBezierPath::bezierPathWithOvalInRect(dot);
    draw::color(palette.online).setFill();
    path.fill();
    objc2_app_kit::NSColor::blackColor().setStroke();
    path.setLineWidth(ONLINE_STROKE);
    path.stroke();
}

/// tdesktop `UnreadBadge` at Touch Bar size: a 15 pt pill at the
/// picture's bottom-right with the count (an empty circle for a chat
/// marked as unread), grey when the chat is muted.
fn draw_badge(badge: &model::Badge, right: f64, palette: Palette) {
    let text = badge.count.map(model::badge_text).unwrap_or_default();
    let font = NSFont::boldSystemFontOfSize(9.5);
    let attributes = draw::attributes(&font, None);
    // SAFETY: the attributes hold a font, as AppKit expects.
    let size = unsafe {
        objc2_foundation::NSString::from_str(&text).sizeWithAttributes(Some(&attributes))
    };
    let width = (size.width + 2. * BADGE_PADDING).max(BADGE_HEIGHT);
    let rect = NSRect::new(
        NSPoint::new(right - width, 0.),
        NSSize::new(width, BADGE_HEIGHT),
    );
    let fill = if badge.muted {
        palette.badge_muted
    } else {
        palette.badge
    };
    draw::color(fill).setFill();
    objc2_app_kit::NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
        rect,
        BADGE_HEIGHT / 2.,
        BADGE_HEIGHT / 2.,
    )
    .fill();
    if !text.is_empty() {
        draw::draw_centered(&text, rect, &font, &draw::color(palette.on_fill));
    }
}

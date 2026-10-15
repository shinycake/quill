//! The three Touch Bars and their items (tdesktop `RootTouchBar`,
//! `TouchBarMain`, `TouchBarAudioPlayer`, `MediaViewTouchBar`,
//! `TextFormatPopover` and `mac_touchbar_controls.mm`).

use super::super::Palette;
use super::super::PickerContents;
use super::super::model::{
    Accessory, Bar, CIRCLE_DIAMETER, Command, FormatCommand, PickerItem, Track, ViewerBar,
    position_text,
};
use super::draw;
use super::picker::PickerSource;
use super::pinned::PinnedPanel;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSButton, NSColor, NSControl, NSCustomTouchBarItem, NSFont, NSImage,
    NSPopoverTouchBarItem, NSScrollView, NSSegmentStyle, NSSegmentSwitchTracking,
    NSSegmentedControl, NSSlider, NSTextField, NSTouchBar, NSTouchBarItem,
    NSTouchBarItemPriorityHigh, NSTouchPhase, NSView, NSWindow,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSet, NSSize, NSString};
use quill::lang::{keys, tr};
use std::cell::RefCell;
use std::rc::Rc;

/// An AppKit image name constant.
macro_rules! named {
    ($name:ident) => {
        // SAFETY: AppKit's image name constants are immutable NSStrings.
        unsafe { objc2_app_kit::$name }
    };
}

const PINNED: &str = "pinnedPanel";
const FORMAT: &str = "popoverInput";
const PICKER: &str = "popoverPicker";
const FORMATTER: &str = "popoverInputFormatter";
const STICKERS: &str = "scrubberStickers";
const EMOJI: &str = "scrubberEmoji";

const PLAYER: &str = "quill.touchbar";
const PLAY: &str = "quill.touchbar.play";
const PREVIOUS: &str = "quill.touchbar.previousItem";
const NEXT: &str = "quill.touchbar.nextItem";
const SEEK: &str = "quill.touchbar.seekbar";
const CLOSE: &str = "quill.touchbar.closePlayer";
const POSITION: &str = "quill.touchbar.currentPosition";

const VIEWER_PLAY: &str = "button.playPause";
const VIEWER_ROTATE: &str = "button.rotate";
const VIEWER_FULLSCREEN: &str = "button.fullscreen";
const VIEWER_PIP: &str = "button.pip";
const VIEWER_POSITION: &str = "trackPosition";
const VIEWER_SEEK: &str = "seekBar";

/// tdesktop `kSegmentSize`, `kSegmentIconSize`.
const SEGMENT_WIDTH: f64 = 92.;
const SEGMENT_ICON: f64 = 25.;

/// Button tags, mapped to commands by `Target`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(isize)]
enum Tag {
    PlayerPlay = 1,
    PlayerPrevious,
    PlayerNext,
    PlayerClose,
    PlayerSeek,
    ViewerPlay,
    ViewerRotate,
    ViewerFullscreen,
    ViewerPip,
    ViewerSeek,
}

impl Tag {
    fn from_raw(raw: isize) -> Option<Self> {
        [
            Self::PlayerPlay,
            Self::PlayerPrevious,
            Self::PlayerNext,
            Self::PlayerClose,
            Self::PlayerSeek,
            Self::ViewerPlay,
            Self::ViewerRotate,
            Self::ViewerFullscreen,
            Self::ViewerPip,
            Self::ViewerSeek,
        ]
        .into_iter()
        .find(|tag| *tag as isize == raw)
    }
}

define_class!(
    /// Receives every button, slider and segment of the bars.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "QuillTouchBarTarget"]
    struct Target;

    impl Target {
        #[unsafe(method(buttonPressed:))]
        fn button_pressed(&self, sender: &NSControl) {
            let command = match Tag::from_raw(sender.tag()) {
                Some(Tag::PlayerPlay) => Command::PlayerPlayPause,
                Some(Tag::PlayerPrevious) => Command::PlayerPrevious,
                Some(Tag::PlayerNext) => Command::PlayerNext,
                Some(Tag::PlayerClose) => Command::PlayerClose,
                Some(Tag::ViewerPlay) => Command::ViewerPlayPause,
                Some(Tag::ViewerRotate) => Command::ViewerRotate,
                Some(Tag::ViewerFullscreen) => Command::ViewerFullscreen,
                Some(Tag::ViewerPip) => Command::ViewerPictureInPicture,
                Some(Tag::PlayerSeek | Tag::ViewerSeek) | None => return,
            };
            super::send(command);
        }

        /// tdesktop `CreateTouchBarSlider`: a touch lifting off ends the seek.
        #[unsafe(method(sliderMoved:))]
        fn slider_moved(&self, sender: &NSControl) {
            let mtm = MainThreadMarker::from(self);
            let done = NSApplication::sharedApplication(mtm)
                .currentEvent()
                .is_some_and(|event| event.touchesMatchingPhase_inView(NSTouchPhase::Ended, None).count() > 0);
            let fraction = sender.doubleValue();
            let command = match Tag::from_raw(sender.tag()) {
                Some(Tag::PlayerSeek) => Command::PlayerSeek { fraction, done },
                Some(Tag::ViewerSeek) => Command::ViewerSeek { fraction },
                _ => return,
            };
            super::send(command);
        }

        #[unsafe(method(formatClicked:))]
        fn format_clicked(&self, sender: &NSSegmentedControl) {
            let index = sender.selectedSegment();
            sender.setSelectedSegment(-1);
            if let Some(format) = usize::try_from(index)
                .ok()
                .and_then(|index| FormatCommand::ALL.get(index))
            {
                super::send(Command::Format(*format));
            }
            let item = super::HOST.with(|host| {
                host.try_borrow()
                    .ok()
                    .and_then(|host| host.as_ref().map(|host| host.format.clone()))
            });
            if let Some(item) = item {
                // SAFETY: a main-thread call on a live popover item.
                unsafe { item.dismissPopover(None) };
            }
        }

        /// tdesktop `segmentClicked:`: open the stickers or the emoji.
        #[unsafe(method(pickerClicked:))]
        fn picker_clicked(&self, sender: &NSSegmentedControl) {
            let emoji = sender.selectedSegment() == 1;
            let host = super::HOST.with(|host| host.try_borrow().ok().and_then(|host| host.clone()));
            let Some(host) = host else {
                return;
            };
            host.picker_bar.setDefaultItemIdentifiers(&NSArray::new());
            host.picker_bar
                .setDefaultItemIdentifiers(&ids(&[if emoji { EMOJI } else { STICKERS }]));
            host.fill_pending_picker();
            super::send(Command::PickerShown);
            // SAFETY: main-thread call on a live popover item.
            unsafe { host.picker.showPopover(None) };
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }
}

fn any(target: &Target) -> &AnyObject {
    target
}

fn ids(list: &[&str]) -> Retained<NSArray<NSString>> {
    let strings: Vec<Retained<NSString>> = list.iter().map(|id| NSString::from_str(id)).collect();
    NSArray::from_retained_slice(&strings)
}

fn custom_item(
    id: &str,
    view: &NSView,
    label: Option<&str>,
    mtm: MainThreadMarker,
) -> Retained<NSTouchBarItem> {
    let item = NSCustomTouchBarItem::initWithIdentifier(
        NSCustomTouchBarItem::alloc(mtm),
        &NSString::from_str(id),
    );
    item.setView(view);
    if let Some(label) = label {
        item.setCustomizationLabel(Some(&NSString::from_str(label)));
    }
    Retained::into_super(item)
}

fn button(image: &NSImage, tag: Tag, target: &Target, mtm: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: AppKit holds a control's target weakly, so a dropped host
    // leaves a nil target, never a dangling one; `buttonPressed:` takes the
    // sender.
    let button = unsafe {
        NSButton::buttonWithImage_target_action(
            image,
            Some(any(target)),
            Some(sel!(buttonPressed:)),
            mtm,
        )
    };
    button.setTag(tag as isize);
    button
}

fn slider(tag: Tag, target: &Target, mtm: MainThreadMarker) -> Retained<NSSlider> {
    // SAFETY: as for `button`, with `sliderMoved:`.
    let slider = unsafe {
        NSSlider::sliderWithTarget_action(Some(any(target)), Some(sel!(sliderMoved:)), mtm)
    };
    slider.setMinValue(0.);
    slider.setMaxValue(1.);
    slider.setContinuous(true);
    slider.setTag(tag as isize);
    slider
}

/// tdesktop `TrackPosition`: "0:42 / 3:10" in white 14 pt.
fn position_label(mtm: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(&position_text(0., 0.)), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(14.)));
    label.setTextColor(Some(&NSColor::whiteColor()));
    label
}

fn image(name: &objc2_app_kit::NSImageName) -> Retained<NSImage> {
    draw::named(name).unwrap_or_else(|| {
        NSImage::initWithSize(
            NSImage::alloc(),
            NSSize::new(CIRCLE_DIAMETER / 2., CIRCLE_DIAMETER / 2.),
        )
    })
}

fn symbol(name: &str, side: f64) -> Retained<NSImage> {
    draw::symbol(name, side)
        .unwrap_or_else(|| NSImage::initWithSize(NSImage::alloc(), NSSize::new(side, side)))
}

/// The picker's segment icons, the app's own (kit) sticker and emoji
/// icons, as tdesktop uses its own.
pub(super) struct Icons {
    pub(super) stickers: Option<Retained<NSImage>>,
    pub(super) emoji: Option<Retained<NSImage>>,
}

/// Which bar is on the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shown {
    None,
    Main,
    Audio,
    Viewer,
}

struct State {
    shown: Shown,
    /// The main bar's identifiers now on it.
    main_items: Vec<&'static str>,
    viewer_items: Vec<&'static str>,
    /// The emoji list the scrubber shows.
    emoji: Option<Rc<Vec<PickerItem>>>,
    /// The latest picker lists, shown when a segment is next tapped, as
    /// tdesktop's `_waitingForUpdate` does, so nothing moves under a
    /// finger.
    pending: Option<PickerContents>,
    /// Whether the scrubbers were ever filled.
    filled: bool,
}

/// Everything built once per window. Cloning shares the same objects.
#[derive(Clone)]
pub(super) struct Host {
    window: Retained<NSWindow>,
    _target: Retained<Target>,
    main: Retained<NSTouchBar>,
    pinned: Retained<PinnedPanel>,
    format: Retained<NSPopoverTouchBarItem>,
    pub(super) picker: Retained<NSPopoverTouchBarItem>,
    picker_bar: Retained<NSTouchBar>,
    stickers: Retained<PickerSource>,
    stickers_scrubber: Retained<objc2_app_kit::NSScrubber>,
    emoji: Retained<PickerSource>,
    emoji_scrubber: Retained<objc2_app_kit::NSScrubber>,
    audio: Retained<NSTouchBar>,
    audio_play: Retained<NSButton>,
    audio_previous: Retained<NSButton>,
    audio_next: Retained<NSButton>,
    audio_seek: Retained<NSSlider>,
    audio_position: Retained<NSTextField>,
    viewer: Retained<NSTouchBar>,
    viewer_play: Retained<NSButton>,
    viewer_fullscreen: Retained<NSButton>,
    viewer_seek: Retained<NSSlider>,
    viewer_position: Retained<NSTextField>,
    state: Rc<RefCell<State>>,
}

impl Host {
    pub(super) fn new(window: Retained<NSWindow>, icons: Icons, mtm: MainThreadMarker) -> Self {
        // tdesktop turns on the View menu's "Customize Touch Bar…".
        NSApplication::sharedApplication(mtm).setAutomaticCustomizeTouchBarMenuItemEnabled(true);
        let target = Target::new(mtm);

        // Main bar: the pinned chats, then formatting or the picker.
        let pinned = PinnedPanel::new(mtm);
        let pinned_item = custom_item(PINNED, &pinned, None, mtm);
        let format = format_popover(&target, mtm);
        let (picker, picker_bar, stickers, stickers_scrubber, emoji, emoji_scrubber) =
            picker_popover(&target, icons, mtm);
        let main = NSTouchBar::new(mtm);
        main.setTemplateItems(&NSSet::from_retained_slice(&[
            pinned_item,
            Retained::into_super(format.clone()),
            Retained::into_super(picker.clone()),
        ]));

        // The player (customizable, as tdesktop's).
        let audio_play = button(
            &image(named!(NSImageNameTouchBarPlayTemplate)),
            Tag::PlayerPlay,
            &target,
            mtm,
        );
        let audio_previous = button(
            &image(named!(NSImageNameTouchBarSkipToStartTemplate)),
            Tag::PlayerPrevious,
            &target,
            mtm,
        );
        let audio_next = button(
            &image(named!(NSImageNameTouchBarSkipToEndTemplate)),
            Tag::PlayerNext,
            &target,
            mtm,
        );
        let audio_close = button(
            &image(named!(NSImageNameStopProgressTemplate)),
            Tag::PlayerClose,
            &target,
            mtm,
        );
        let audio_seek = slider(Tag::PlayerSeek, &target, mtm);
        let audio_position = position_label(mtm);
        let audio = NSTouchBar::new(mtm);
        audio.setCustomizationIdentifier(Some(&NSString::from_str(PLAYER)));
        audio.setTemplateItems(&NSSet::from_retained_slice(&[
            custom_item(PLAY, &audio_play, Some(tr(keys::Q_MISC_PLAY_PAUSE)), mtm),
            custom_item(
                PREVIOUS,
                &audio_previous,
                Some(tr(keys::Q_MISC_PREVIOUS_PLAYLIST_ITEM)),
                mtm,
            ),
            custom_item(
                NEXT,
                &audio_next,
                Some(tr(keys::Q_MISC_NEXT_PLAYLIST_ITEM)),
                mtm,
            ),
            custom_item(SEEK, &audio_seek, Some(tr(keys::Q_MISC_SEEK_BAR)), mtm),
            custom_item(
                CLOSE,
                &audio_close,
                Some(tr(keys::Q_MISC_CLOSE_PLAYER)),
                mtm,
            ),
            custom_item(
                POSITION,
                &audio_position,
                Some(tr(keys::Q_MISC_TRACK_POSITION)),
                mtm,
            ),
        ]));
        audio.setDefaultItemIdentifiers(&ids(&[PLAY, PREVIOUS, NEXT, SEEK, CLOSE]));
        audio.setCustomizationAllowedItemIdentifiers(&ids(&[
            PLAY, PREVIOUS, NEXT, POSITION, SEEK, CLOSE,
        ]));

        // The media viewer.
        let viewer_play = button(
            &image(named!(NSImageNameTouchBarPlayTemplate)),
            Tag::ViewerPlay,
            &target,
            mtm,
        );
        let viewer_rotate = button(
            &image(named!(NSImageNameTouchBarRotateLeftTemplate)),
            Tag::ViewerRotate,
            &target,
            mtm,
        );
        let viewer_fullscreen = button(
            &image(named!(NSImageNameTouchBarEnterFullScreenTemplate)),
            Tag::ViewerFullscreen,
            &target,
            mtm,
        );
        let viewer_pip = button(
            &symbol("pip.enter", CIRCLE_DIAMETER / 2.),
            Tag::ViewerPip,
            &target,
            mtm,
        );
        let viewer_seek = slider(Tag::ViewerSeek, &target, mtm);
        let viewer_position = position_label(mtm);
        let viewer = NSTouchBar::new(mtm);
        viewer.setTemplateItems(&NSSet::from_retained_slice(&[
            custom_item(
                VIEWER_PLAY,
                &viewer_play,
                Some(tr(keys::Q_MISC_PLAY_PAUSE)),
                mtm,
            ),
            custom_item(
                VIEWER_ROTATE,
                &viewer_rotate,
                Some(tr(keys::Q_MISC_ROTATE)),
                mtm,
            ),
            custom_item(
                VIEWER_FULLSCREEN,
                &viewer_fullscreen,
                Some(tr(keys::Q_MISC_FULLSCREEN)),
                mtm,
            ),
            custom_item(
                VIEWER_PIP,
                &viewer_pip,
                Some(tr(keys::Q_MISC_PICTURE_IN_PICTURE)),
                mtm,
            ),
            custom_item(
                VIEWER_SEEK,
                &viewer_seek,
                Some(tr(keys::Q_MISC_SEEK_BAR)),
                mtm,
            ),
            custom_item(
                VIEWER_POSITION,
                &viewer_position,
                Some(tr(keys::Q_MISC_TRACK_POSITION)),
                mtm,
            ),
        ]));

        Self {
            window,
            _target: target,
            main,
            pinned,
            format,
            picker,
            picker_bar,
            stickers,
            stickers_scrubber,
            emoji,
            emoji_scrubber,
            audio,
            audio_play,
            audio_previous,
            audio_next,
            audio_seek,
            audio_position,
            viewer,
            viewer_play,
            viewer_fullscreen,
            viewer_seek,
            viewer_position,
            state: Rc::new(RefCell::new(State {
                shown: Shown::None,
                main_items: Vec::new(),
                viewer_items: Vec::new(),
                emoji: None,
                pending: None,
                filled: false,
            })),
        }
    }

    pub(super) fn picker_item(&self) -> Retained<NSPopoverTouchBarItem> {
        self.picker.clone()
    }

    /// Demo capture: the views of every bar, a row each, after showing
    /// `bar` and sample player and viewer states.
    #[cfg(feature = "demo-capture")]
    pub(super) fn preview_rows(
        &self,
        bar: &Bar,
        picker: Option<&PickerContents>,
        palette: Palette,
    ) -> Vec<Vec<Retained<NSView>>> {
        let mut rows = Vec::new();
        self.apply(bar, picker, palette);
        self.fill_pending_picker();
        let mut main = vec![Retained::into_super(Retained::into_super(
            Retained::into_super(self.pinned.clone()),
        ))];
        main.push(self.picker.collapsedRepresentation());
        rows.push(main);
        let view = self
            .format
            .popoverTouchBar()
            .itemForIdentifier(&NSString::from_str(FORMATTER))
            .and_then(|item| item.view());
        let formatter = view
            .and_then(|view| view.downcast::<NSScrollView>().ok())
            .and_then(|scroll| scroll.documentView());
        rows.extend(formatter.map(|view| vec![view]));
        for scrubber in [&self.stickers_scrubber, &self.emoji_scrubber] {
            scrubber.reloadData();
            rows.push(vec![Retained::into_super(scrubber.clone())]);
        }
        let track = Track {
            playing: true,
            can_previous: true,
            can_next: false,
            position: 42.,
            duration: 190.,
        };
        self.apply(&Bar::Audio(track), None, palette);
        rows.push(item_views(
            &self.audio,
            &[PLAY, PREVIOUS, NEXT, SEEK, CLOSE, POSITION],
        ));
        let video = ViewerBar::Video {
            playing: false,
            fullscreen: false,
            position: 5.,
            duration: 61.,
        };
        self.apply(&Bar::Viewer(video), None, palette);
        rows.push(item_views(
            &self.viewer,
            &[
                VIEWER_ROTATE,
                VIEWER_FULLSCREEN,
                VIEWER_PIP,
                VIEWER_PLAY,
                VIEWER_SEEK,
                VIEWER_POSITION,
            ],
        ));
        self.apply(bar, picker, palette);
        rows
    }

    /// Take the bar off the window.
    pub(super) fn remove(&self) {
        self.window.setTouchBar(None);
    }

    pub(super) fn apply(&self, bar: &Bar, picker: Option<&PickerContents>, palette: Palette) {
        let shown = match bar {
            Bar::Empty => Shown::None,
            Bar::Main { pinned, accessory } => {
                self.pinned.set_pins(&pinned.pins, palette);
                let mut items = vec![PINNED];
                match accessory {
                    Accessory::Format => items.push(FORMAT),
                    Accessory::Picker => items.push(PICKER),
                    Accessory::None => {}
                }
                if let Some(picker) = picker {
                    self.offer_picker(picker);
                }
                self.set_items(&self.main, items, |state| &mut state.main_items);
                Shown::Main
            }
            Bar::Audio(track) => {
                self.apply_track(track);
                Shown::Audio
            }
            Bar::Viewer(viewer) => {
                self.apply_viewer(viewer);
                Shown::Viewer
            }
        };
        let previous = self
            .state
            .try_borrow_mut()
            .map(|mut state| std::mem::replace(&mut state.shown, shown))
            .unwrap_or(shown);
        if previous != shown {
            let bar = match shown {
                Shown::None => None,
                Shown::Main => Some(&self.main),
                Shown::Audio => Some(&self.audio),
                Shown::Viewer => Some(&self.viewer),
            };
            self.window.setTouchBar(bar.map(|bar| &**bar));
        }
    }

    /// Set a bar's identifiers when they changed (setting them rebuilds
    /// the bar on the hardware).
    fn set_items(
        &self,
        bar: &NSTouchBar,
        items: Vec<&'static str>,
        slot: impl FnOnce(&mut State) -> &mut Vec<&'static str>,
    ) {
        let changed = self.state.try_borrow_mut().is_ok_and(|mut state| {
            let current = slot(&mut state);
            if *current == items {
                false
            } else {
                *current = items.clone();
                true
            }
        });
        if changed {
            bar.setDefaultItemIdentifiers(&ids(&items));
        }
    }

    /// Keep new picker lists for the next tap. Sticker pictures that
    /// finished downloading for the lists on screen show at once
    /// (tdesktop's download handler).
    fn offer_picker(&self, picker: &PickerContents) {
        if self.stickers.shows(&picker.stickers)
            && self.stickers.set_items(&picker.stickers, &picker.files)
        {
            self.stickers_scrubber.reloadData();
        }
        let first = self.state.try_borrow_mut().is_ok_and(|mut state| {
            if state.pending.as_ref() != Some(picker) {
                state.pending = Some(picker.clone());
            }
            !state.filled
        });
        if first {
            self.fill_pending_picker();
        }
    }

    /// Show the kept picker lists.
    fn fill_pending_picker(&self) {
        let pending = self.state.try_borrow_mut().ok().and_then(|mut state| {
            state.filled = true;
            state.pending.take()
        });
        if let Some(picker) = pending {
            self.fill_picker(&picker);
        }
    }

    fn fill_picker(&self, picker: &PickerContents) {
        if self.stickers.set_items(&picker.stickers, &picker.files) {
            self.stickers_scrubber.reloadData();
        }
        let emoji_changed = self.state.try_borrow_mut().is_ok_and(|mut state| {
            let same = state
                .emoji
                .as_ref()
                .is_some_and(|shown| Rc::ptr_eq(shown, &picker.emoji));
            if !same {
                state.emoji = Some(picker.emoji.clone());
            }
            !same
        });
        if emoji_changed && self.emoji.set_items(&picker.emoji, &[]) {
            self.emoji_scrubber.reloadData();
        }
    }

    fn apply_track(&self, track: &Track) {
        set_play_image(&self.audio_play, track.playing);
        if self.audio_previous.isEnabled() != track.can_previous {
            self.audio_previous.setEnabled(track.can_previous);
        }
        if self.audio_next.isEnabled() != track.can_next {
            self.audio_next.setEnabled(track.can_next);
        }
        set_seek(&self.audio_seek, track.position, track.duration);
        set_position(&self.audio_position, track.position, track.duration);
    }

    /// tdesktop: a photo gets Rotate; a video Rotate, Fullscreen,
    /// Picture-in-Picture, Play/Pause, the seek bar and the position.
    fn apply_viewer(&self, viewer: &ViewerBar) {
        let items = match viewer {
            ViewerBar::Photo => vec![VIEWER_ROTATE],
            ViewerBar::Video {
                playing,
                fullscreen,
                position,
                duration,
            } => {
                set_play_image(&self.viewer_play, *playing);
                let name = if *fullscreen {
                    named!(NSImageNameTouchBarExitFullScreenTemplate)
                } else {
                    named!(NSImageNameTouchBarEnterFullScreenTemplate)
                };
                self.viewer_fullscreen.setImage(Some(&image(name)));
                set_seek(&self.viewer_seek, *position, *duration);
                set_position(&self.viewer_position, *position, *duration);
                vec![
                    VIEWER_ROTATE,
                    VIEWER_FULLSCREEN,
                    VIEWER_PIP,
                    VIEWER_PLAY,
                    VIEWER_SEEK,
                    VIEWER_POSITION,
                ]
            }
            ViewerBar::Other => Vec::new(),
        };
        self.set_items(&self.viewer, items, |state| &mut state.viewer_items);
    }
}

#[cfg(feature = "demo-capture")]
fn item_views(bar: &NSTouchBar, list: &[&str]) -> Vec<Retained<NSView>> {
    list.iter()
        .filter_map(|id| bar.itemForIdentifier(&NSString::from_str(id))?.view())
        .collect()
}

/// Pause while playing, play while paused.
fn set_play_image(button: &NSButton, playing: bool) {
    let name = if playing {
        named!(NSImageNameTouchBarPauseTemplate)
    } else {
        named!(NSImageNameTouchBarPlayTemplate)
    };
    button.setImage(Some(&image(name)));
}

/// tdesktop: disabled without a length; left alone while touched.
fn set_seek(slider: &NSSlider, position: f64, duration: f64) {
    if duration <= 0. {
        slider.setEnabled(false);
        slider.setDoubleValue(0.);
        return;
    }
    slider.setEnabled(true);
    if !slider.isHighlighted() {
        slider.setDoubleValue((position.max(0.) / duration).clamp(0., 1.));
    }
}

fn set_position(label: &NSTextField, position: f64, duration: f64) {
    let text = position_text(position, duration);
    if label.stringValue().to_string() != text {
        label.setStringValue(&NSString::from_str(&text));
    }
}

/// tdesktop `TextFormatPopover`: an italic icon that opens a scrolling row
/// of the formatting commands.
fn format_popover(target: &Target, mtm: MainThreadMarker) -> Retained<NSPopoverTouchBarItem> {
    let item = NSPopoverTouchBarItem::initWithIdentifier(
        NSPopoverTouchBarItem::alloc(mtm),
        &NSString::from_str(FORMAT),
    );
    item.setCollapsedRepresentationImage(Some(&image(named!(
        NSImageNameTouchBarTextItalicTemplate
    ))));
    let segment = NSSegmentedControl::new(mtm);
    segment.setSegmentStyle(NSSegmentStyle::Rounded);
    // SAFETY: AppKit holds a control's target weakly, so a dropped host leaves a nil target, never a dangling one;
    // `formatClicked:` takes the sender.
    unsafe {
        segment.setTarget(Some(any(target)));
        segment.setAction(Some(sel!(formatClicked:)));
    }
    segment.setSegmentCount(FormatCommand::ALL.len() as isize);
    let mut width = 0.;
    for (index, command) in FormatCommand::ALL.iter().enumerate() {
        let label = NSString::from_str(command.label());
        width += NSTextField::labelWithString(&label, mtm)
            .frame()
            .size
            .width
            .ceil()
            * 1.2
            * 1.4;
        segment.setLabel_forSegment(&label, index as isize);
    }
    segment.setFrame(NSRect::new(
        NSPoint::new(0., 0.),
        NSSize::new(width, CIRCLE_DIAMETER),
    ));
    let scroll = NSScrollView::new(mtm);
    scroll.setDocumentView(Some(&segment));
    let popover_bar = NSTouchBar::new(mtm);
    popover_bar.setTemplateItems(&NSSet::from_retained_slice(&[custom_item(
        FORMATTER, &scroll, None, mtm,
    )]));
    popover_bar.setDefaultItemIdentifiers(&ids(&[FORMATTER]));
    item.setPopoverTouchBar(&popover_bar);
    item
}

type Picker = (
    Retained<NSPopoverTouchBarItem>,
    Retained<NSTouchBar>,
    Retained<PickerSource>,
    Retained<objc2_app_kit::NSScrubber>,
    Retained<PickerSource>,
    Retained<objc2_app_kit::NSScrubber>,
);

/// tdesktop `StickerEmojiPopover`: two icon segments, stickers and emoji,
/// each opening its scrubber.
fn picker_popover(target: &Target, icons: Icons, mtm: MainThreadMarker) -> Picker {
    let item = NSPopoverTouchBarItem::initWithIdentifier(
        NSPopoverTouchBarItem::alloc(mtm),
        &NSString::from_str(PICKER),
    );
    let segment = NSSegmentedControl::new(mtm);
    segment.setSegmentStyle(NSSegmentStyle::Separated);
    segment.setSegmentCount(2);
    let stickers_icon = icons
        .stickers
        .unwrap_or_else(|| symbol("note.text", SEGMENT_ICON * 0.7));
    let emoji_icon = icons
        .emoji
        .unwrap_or_else(|| symbol("face.smiling", SEGMENT_ICON * 0.7));
    segment.setImage_forSegment(Some(&stickers_icon), 0);
    segment.setImage_forSegment(Some(&emoji_icon), 1);
    segment.setWidth_forSegment(SEGMENT_WIDTH, 0);
    segment.setWidth_forSegment(SEGMENT_WIDTH, 1);
    segment.setTrackingMode(NSSegmentSwitchTracking::Momentary);
    // SAFETY: AppKit holds a control's target weakly, so a dropped host leaves a nil target, never a dangling one;
    // `pickerClicked:` takes the sender.
    unsafe {
        segment.setTarget(Some(any(target)));
        segment.setAction(Some(sel!(pickerClicked:)));
    }
    item.setVisibilityPriority(NSTouchBarItemPriorityHigh);
    item.setCollapsedRepresentation(&segment);
    let stickers = PickerSource::new(mtm);
    let stickers_scrubber = stickers.scrubber(mtm);
    let emoji = PickerSource::new(mtm);
    let emoji_scrubber = emoji.scrubber(mtm);
    let popover_bar = NSTouchBar::new(mtm);
    popover_bar.setTemplateItems(&NSSet::from_retained_slice(&[
        custom_item(STICKERS, &stickers_scrubber, None, mtm),
        custom_item(EMOJI, &emoji_scrubber, None, mtm),
    ]));
    item.setPopoverTouchBar(&popover_bar);
    (
        item,
        popover_bar,
        stickers,
        stickers_scrubber,
        emoji,
        emoji_scrubber,
    )
}

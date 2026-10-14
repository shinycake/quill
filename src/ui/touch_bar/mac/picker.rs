//! The sticker and emoji picker (tdesktop `StickerEmojiPopover` and
//! `PickerCustomTouchBarItem`, `items/mac_scrubber_item.mm`): a two-icon
//! segment on the bar; each icon opens a popover with a free-scrolling
//! scrubber of section titles and stickers or emoji. A sticker is sent to
//! the open chat and closes the popover; an emoji goes into the composer
//! and the popover stays.

use super::super::model::{CIRCLE_DIAMETER, Command, PickerItem};
use super::draw;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{
    AnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
};
use objc2_app_kit::{
    NSImage, NSScrubber, NSScrubberAlignment, NSScrubberDataSource, NSScrubberDelegate,
    NSScrubberFlowLayout, NSScrubberFlowLayoutDelegate, NSScrubberImageItemView,
    NSScrubberItemView, NSScrubberMode, NSScrubberTextItemView, NSTextField,
};
use objc2_foundation::{NSInteger, NSRect, NSSize, NSString};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

const TITLE_ID: &str = "pickerTitleItem";
const STICKER_ID: &str = "stickerItem";
const EMOJI_ID: &str = "emojiItem";

#[derive(Default)]
pub(super) struct Ivars {
    items: RefCell<Vec<PickerItem>>,
    files: RefCell<HashMap<i32, PathBuf>>,
    /// Emoji pictures already drawn, by index.
    emoji: RefCell<HashMap<usize, Retained<NSImage>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "QuillTouchBarPickerSource"]
    #[ivars = Ivars]
    pub(super) struct PickerSource;

    unsafe impl NSObjectProtocol for PickerSource {}

    unsafe impl NSScrubberDataSource for PickerSource {
        #[unsafe(method(numberOfItemsForScrubber:))]
        fn number_of_items(&self, _scrubber: &NSScrubber) -> NSInteger {
            self.ivars()
                .items
                .try_borrow()
                .map_or(0, |items| items.len() as NSInteger)
        }

        #[unsafe(method_id(scrubber:viewForItemAtIndex:))]
        fn view_for_item(
            &self,
            scrubber: &NSScrubber,
            index: NSInteger,
        ) -> Retained<NSScrubberItemView> {
            self.item_view(scrubber, index)
        }
    }

    unsafe impl NSScrubberDelegate for PickerSource {
        #[unsafe(method(scrubber:didSelectItemAtIndex:))]
        fn did_select(&self, scrubber: &NSScrubber, index: NSInteger) {
            scrubber.setSelectedIndex(-1);
            let item = usize::try_from(index).ok().and_then(|index| {
                self.ivars()
                    .items
                    .try_borrow()
                    .ok()
                    .and_then(|items| items.get(index).cloned())
            });
            match item {
                Some(PickerItem::Sticker(sticker)) => {
                    super::send(Command::SendSticker(sticker));
                    super::dismiss_picker();
                }
                Some(PickerItem::Emoji(emoji)) => super::send(Command::InsertEmoji(emoji)),
                Some(PickerItem::Title(_)) | None => {}
            }
        }
    }

    unsafe impl NSScrubberFlowLayoutDelegate for PickerSource {
        #[unsafe(method(scrubber:layout:sizeForItemAtIndex:))]
        fn size_for_item(
            &self,
            _scrubber: &NSScrubber,
            _layout: &NSScrubberFlowLayout,
            index: NSInteger,
        ) -> NSSize {
            let title = usize::try_from(index).ok().and_then(|index| {
                match self.ivars().items.try_borrow().ok()?.get(index)? {
                    PickerItem::Title(title) => Some(title.clone()),
                    _ => None,
                }
            });
            let width = title.map_or(0., |title| {
                title_width(&title, MainThreadMarker::from(self))
            });
            NSSize::new(CIRCLE_DIAMETER + width, CIRCLE_DIAMETER)
        }
    }
);

/// tdesktop `WidthFromString`: a label's width, plus a fifth.
fn title_width(title: &str, mtm: MainThreadMarker) -> f64 {
    let label = NSTextField::labelWithString(&NSString::from_str(title), mtm);
    label.frame().size.width.ceil() * 1.2
}

impl PickerSource {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }

    /// A scrubber listing this source (tdesktop's settings: free mode,
    /// 10 pt spacing, centered, more-content indicators).
    pub(super) fn scrubber(&self, mtm: MainThreadMarker) -> Retained<NSScrubber> {
        let scrubber = NSScrubber::initWithFrame(NSScrubber::alloc(mtm), NSRect::ZERO);
        let layout = NSScrubberFlowLayout::new(mtm);
        layout.setItemSpacing(10.);
        scrubber.setScrubberLayout(&layout);
        scrubber.setMode(NSScrubberMode::Free);
        scrubber.setFloatsSelectionViews(true);
        scrubber.setShowsAdditionalContentIndicators(true);
        scrubber.setItemAlignment(NSScrubberAlignment::Center);
        scrubber.setDelegate(Some(ProtocolObject::from_ref(self)));
        scrubber.setDataSource(Some(ProtocolObject::from_ref(self)));
        // SAFETY: both classes are NSScrubberItemView subclasses.
        unsafe {
            scrubber.registerClass_forItemIdentifier(
                Some(NSScrubberTextItemView::class()),
                &NSString::from_str(TITLE_ID),
            );
            scrubber.registerClass_forItemIdentifier(
                Some(NSScrubberImageItemView::class()),
                &NSString::from_str(STICKER_ID),
            );
            scrubber.registerClass_forItemIdentifier(
                Some(NSScrubberImageItemView::class()),
                &NSString::from_str(EMOJI_ID),
            );
        }
        scrubber
    }

    /// Whether these are the items shown now.
    pub(super) fn shows(&self, items: &[PickerItem]) -> bool {
        self.ivars()
            .items
            .try_borrow()
            .is_ok_and(|current| current.as_slice() == items)
    }

    /// New contents; true when they changed (the scrubber then reloads).
    pub(super) fn set_items(&self, items: &[PickerItem], files: &[(i32, PathBuf)]) -> bool {
        let files: HashMap<i32, PathBuf> = files.iter().cloned().collect();
        let same = self
            .ivars()
            .items
            .try_borrow()
            .is_ok_and(|current| current.as_slice() == items)
            && self
                .ivars()
                .files
                .try_borrow()
                .is_ok_and(|current| *current == files);
        if same {
            return false;
        }
        if let Ok(mut current) = self.ivars().items.try_borrow_mut() {
            *current = items.to_vec();
        }
        if let Ok(mut current) = self.ivars().files.try_borrow_mut() {
            *current = files;
        }
        if let Ok(mut emoji) = self.ivars().emoji.try_borrow_mut() {
            emoji.clear();
        }
        true
    }

    fn item_view(&self, scrubber: &NSScrubber, index: NSInteger) -> Retained<NSScrubberItemView> {
        let mtm = MainThreadMarker::from(self);
        let item = usize::try_from(index).ok().and_then(|index| {
            self.ivars()
                .items
                .try_borrow()
                .ok()
                .and_then(|items| items.get(index).cloned())
        });
        let owner: &AnyObject = self;
        let make = |id: &str| {
            // SAFETY: the owner is this data source, as AppKit expects.
            unsafe { scrubber.makeItemWithIdentifier_owner(&NSString::from_str(id), Some(owner)) }
        };
        match item {
            Some(PickerItem::Title(title)) => {
                if let Some(view) = make(TITLE_ID)
                    && let Ok(text) = view.clone().downcast::<NSScrubberTextItemView>()
                {
                    text.textField().setStringValue(&NSString::from_str(&title));
                    return view;
                }
            }
            Some(PickerItem::Sticker(sticker)) => {
                if let Some(view) = make(STICKER_ID)
                    && let Ok(image_view) = view.clone().downcast::<NSScrubberImageItemView>()
                {
                    let picture = sticker
                        .display
                        .and_then(|id| self.ivars().files.try_borrow().ok()?.get(&id.0).cloned())
                        .and_then(|path| draw::file_image(&path));
                    let picture = picture.unwrap_or_else(|| {
                        NSImage::initWithSize(
                            NSImage::alloc(),
                            NSSize::new(CIRCLE_DIAMETER, CIRCLE_DIAMETER),
                        )
                    });
                    image_view.setImage(&picture);
                    return view;
                }
            }
            Some(PickerItem::Emoji(emoji)) => {
                if let Some(view) = make(EMOJI_ID)
                    && let Ok(image_view) = view.clone().downcast::<NSScrubberImageItemView>()
                {
                    let key = usize::try_from(index).unwrap_or(0);
                    let cached = self
                        .ivars()
                        .emoji
                        .try_borrow()
                        .ok()
                        .and_then(|cache| cache.get(&key).cloned());
                    let picture = cached.unwrap_or_else(|| {
                        let picture = draw::emoji_image(&emoji);
                        if let Ok(mut cache) = self.ivars().emoji.try_borrow_mut() {
                            cache.insert(key, picture.clone());
                        }
                        picture
                    });
                    image_view.setImage(&picture);
                    return view;
                }
            }
            None => {}
        }
        NSScrubberItemView::new(mtm)
    }
}

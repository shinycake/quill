//! What the stickers manager box keeps while it is open.
use gpui_kit::ScrollHandle;
use quill::stickers_manager::ManagerEdits;

/// Removals and the dragged order, one set per list, and the scroll
/// position shared by both boxes (only one is open at a time).
#[derive(Default)]
pub(crate) struct StickersManagerUi {
    pub(in crate::ui) stickers: ManagerEdits,
    pub(in crate::ui) masks: ManagerEdits,
    pub(in crate::ui) scroll: ScrollHandle,
    /// The tab the composer's sticker panel was on when the box opened: the
    /// box browses the same session lists, and puts it back on close.
    pub(in crate::ui) composer_tab: quill::state::StickerTab,
}

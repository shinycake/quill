//! Methods moved out of `message_text.rs` to keep files under 1000 lines.

use super::*;

impl FooterMeta {
    pub(in crate::ui) fn of(
        message: &quill::state::HistoryMessage,
        receipt: OutboxReceipt,
        views: Option<i32>,
        signature: Option<String>,
    ) -> Self {
        Self {
            date: message.date,
            pending: message.pending,
            receipt,
            views,
            signature,
            edited: message.extras.edit_date > 0,
            pinned: message.is_pinned,
            imported: message.extras.import_info.is_some(),
            tooltip: quill::state::footer_tooltip(message, format_unix_date_time),
        }
    }

    /// Width the whole footer needs: `base` (time and receipt) plus the
    /// extras that precede it.
    pub(in crate::ui) fn reserve(&self, base: Pixels) -> Pixels {
        let mut width = base;
        if self.edited || self.imported {
            width += px(44.);
        }
        if self.pinned {
            width += px(20.);
        }
        if self.views.is_some() {
            width += px(48.);
        }
        width
    }

    pub(super) fn plain(&self) -> bool {
        self.views.is_none()
            && self.signature.is_none()
            && !self.edited
            && !self.pinned
            && !self.imported
            && self.tooltip.is_none()
    }
}

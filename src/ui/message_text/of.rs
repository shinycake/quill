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
            effect: None,
            forward_drag: None,
        }
    }

    /// The footer of a message sent with an effect gets its badge.
    pub(in crate::ui) fn with_effect(
        mut self,
        effect: Option<crate::ui::message_effect_badge::FooterEffect>,
    ) -> Self {
        self.effect = effect;
        self
    }

    /// The footer's time starts a forward drag of the message.
    pub(in crate::ui) fn with_forward_drag(
        mut self,
        drag: Option<quill::forward_drag::ForwardDrag>,
    ) -> Self {
        self.forward_drag = drag;
        self
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
        if self.effect.is_some() {
            // The cell and the footer's gap after it (tdesktop
            // `countEffectMaxWidth`).
            width += px(crate::ui::message_effect_badge::CELL + 8.0);
        }
        width
    }

    pub(super) fn plain(&self) -> bool {
        self.views.is_none()
            && self.signature.is_none()
            && !self.edited
            && !self.pinned
            && !self.imported
            && self.effect.is_none()
            && self.tooltip.is_none()
    }
}

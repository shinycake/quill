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

    /// The footer as an animation layer draws it again over a picture
    /// (`anim_layer::mirror`): it takes no hover and no press, which the
    /// row's own under it has, and a layer holding a hitbox draws in whole
    /// frames.
    pub(in crate::ui) fn mirrored(&self) -> Self {
        Self {
            tooltip: None,
            forward_drag: None,
            ..self.clone()
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

#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod tests {
    use super::{FooterMeta, OutboxReceipt, message_footer_meta};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext,
        VisualTestContext, Window, div, px, size,
    };
    use quill::forward_drag::ForwardDrag;
    use quill::ids::{ChatId, MessageId};

    struct Footer(FooterMeta);

    impl Render for Footer {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().children(message_footer_meta(&self.0))
        }
    }

    struct Host(Entity<Footer>);

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(self.0.clone().live())
        }
    }

    fn interactive_views(meta: FooterMeta, cx: &mut TestAppContext) -> usize {
        let window = cx.open_window(size(px(200.), px(100.)), |_, cx| {
            Host(cx.new(|_| Footer(meta)))
        });
        let mut vcx = VisualTestContext::from_window(window.into(), cx);
        vcx.update(|window, cx| {
            window.render_frame(cx);
            window.interactive_live_views()
        })
    }

    /// The tooltip and the forward drag stay with the row's footer.
    #[gpui_kit::test]
    fn a_mirrored_footer_builds_no_hitboxes(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let meta = FooterMeta {
            date: 1_790_000_000,
            pending: false,
            receipt: OutboxReceipt::None,
            views: None,
            signature: None,
            edited: true,
            pinned: false,
            imported: false,
            tooltip: Some("Sent".to_string()),
            effect: None,
            forward_drag: ForwardDrag::begin(ChatId(1), MessageId(2), &[], false, |_| true).ok(),
        };
        assert_eq!(interactive_views(meta.clone(), cx), 1, "the row's own");
        assert_eq!(interactive_views(meta.mirrored(), cx), 0, "the layer's");
    }
}

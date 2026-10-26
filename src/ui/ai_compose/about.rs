//! "About AI Editor": tdesktop's `AboutCocoonBox` (ui/boxes/about_cocoon_box.cpp)
//! as a view of the box. The cover's gradient, logo and particles are
//! left out: the heading and subtitle sit on the dialog surface.

use super::{MarkedLink, marked_link};
use crate::ui::ai_compose_state::AiComposeView;
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with};
use std::rc::Rc;

/// Where a Cocoon link goes (tdesktop `CocoonMention`, `CocoonDomain`,
/// `CocoonDirect`).
#[derive(Clone, Copy)]
enum CocoonLink {
    Mention,
    Domain,
    Direct,
}

impl QuillApp {
    fn open_cocoon_link(&mut self, link: CocoonLink, cx: &mut Context<Self>) {
        match link {
            CocoonLink::Mention => {
                self.queue_deep_link("https://t.me/cocoon".into());
                self.close_ai_compose(cx);
            }
            CocoonLink::Direct => {
                self.queue_deep_link(format!("https://{}", tr(keys::TRANSLATE_COCOON_TEXT_LINK)));
                self.close_ai_compose(cx);
            }
            CocoonLink::Domain => {
                let url = format!("https://{}", tr(keys::TRANSLATE_COCOON_EVERYONE_DOMAIN));
                self.open_message_url(&url, cx);
            }
        }
    }

    fn cocoon_text(
        &self,
        id: &'static str,
        marked: MarkedLink,
        link: CocoonLink,
        cx: &mut Context<Self>,
    ) -> InteractiveText {
        let bold = HighlightStyle {
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let linked = HighlightStyle {
            color: Some(cx.theme().primary),
            ..Default::default()
        };
        let mut highlights: Vec<_> = marked
            .bold
            .iter()
            .map(|range| (range.clone(), bold))
            .chain(marked.link.iter().map(|range| (range.clone(), linked)))
            .collect();
        highlights.sort_by_key(|(range, _)| range.start);
        let app = cx.entity().downgrade();
        let open = Rc::new(move |cx: &mut App| {
            let _ = app.update(cx, |this, cx| this.open_cocoon_link(link, cx));
        });
        InteractiveText::new(id, StyledText::new(marked.text).with_highlights(highlights))
            .on_click(marked.link.into_iter().collect(), move |_, _, cx| open(cx))
    }

    fn cocoon_row(
        &self,
        icon: IconName,
        title: &'static str,
        text: impl IntoElement,
        cx: &App,
    ) -> Div {
        div()
            .flex()
            .gap_3()
            .child(
                div()
                    .pt_0p5()
                    .text_color(cx.theme().primary)
                    .child(Icon::new(icon).size(px(18.))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().text_sm().font_semibold().child(title))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(text),
                    ),
            )
    }

    pub(super) fn ai_compose_about_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let (subtitle, bold_ranges) = crate::ui::screen_chooser::split_bold(&tr_with(
            keys::TRANSLATE_COCOON_SUBTITLE,
            &[("text", tr(keys::TRANSLATE_COCOON_EXPLAIN))],
        ));
        let bold = HighlightStyle {
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let subtitle = StyledText::new(subtitle).with_highlights(
            bold_ranges
                .into_iter()
                .map(|range| (range, bold))
                .collect::<Vec<_>>(),
        );
        let private = self.cocoon_text(
            "ai-cocoon-private",
            marked_link(
                tr(keys::TRANSLATE_COCOON_PRIVATE_TEXT),
                "mention",
                tr(keys::TRANSLATE_COCOON_PRIVATE_MENTION),
            ),
            CocoonLink::Mention,
            cx,
        );
        let everyone = self.cocoon_text(
            "ai-cocoon-everyone",
            marked_link(
                tr(keys::TRANSLATE_COCOON_EVERYONE_TEXT),
                "domain",
                tr(keys::TRANSLATE_COCOON_EVERYONE_DOMAIN),
            ),
            CocoonLink::Domain,
            cx,
        );
        let direct = self.cocoon_text(
            "ai-cocoon-direct",
            marked_link(
                tr(keys::TRANSLATE_COCOON_TEXT),
                "link",
                tr(keys::TRANSLATE_COCOON_TEXT_LINK),
            ),
            CocoonLink::Direct,
            cx,
        );
        let theme = cx.theme();
        let (muted, border, primary) = (theme.muted_foreground, theme.border, theme.primary);
        v_flex()
            .gap_4()
            .pt_2()
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_color(primary)
                            .child(Icon::new(IconName::Sparkles).size(px(36.))),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_semibold()
                            .child(tr(keys::TRANSLATE_COCOON_TITLE)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(muted)
                            .child(subtitle),
                    ),
            )
            .child(self.cocoon_row(
                IconName::Lock,
                tr(keys::TRANSLATE_COCOON_PRIVATE_TITLE),
                private,
                cx,
            ))
            .child(self.cocoon_row(
                IconName::ChartBar,
                tr(keys::TRANSLATE_COCOON_EFFICIENT_TITLE),
                tr(keys::TRANSLATE_COCOON_EFFICIENT_TEXT),
                cx,
            ))
            .child(self.cocoon_row(
                IconName::Gift,
                tr(keys::TRANSLATE_COCOON_EVERYONE_TITLE),
                everyone,
                cx,
            ))
            .child(div().h(px(1.)).w_full().bg(border))
            .child(
                div()
                    .text_sm()
                    .text_center()
                    .text_color(muted)
                    .child(direct),
            )
            .into_any_element()
    }

    pub(super) fn ai_compose_about_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        Button::new("ai-compose-about-done")
            .primary()
            .w_full()
            .icon(IconName::Star)
            .label(tr(keys::TRANSLATE_COCOON_DONE))
            .on_click(cx.listener(|this, _, _, cx| {
                this.set_ai_compose_view(AiComposeView::Card, cx);
            }))
            .into_any_element()
    }
}

//! Text with a Fragment link inside it, as tdesktop writes them ("…was
//! acquired on {link}.", "You can acquire additional usernames on {link}.").
//! The link opens the Fragment address from the app config
//! (`premium_hub::fragment`).
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use crate::ui::os_open::open_link_in;
use gpui_kit::component::*;
use gpui_kit::*;

/// `before` + the link word + `after` as one wrapped text; a click on the
/// link word opens `url`. Colors come from the surrounding text and the
/// theme's primary.
pub(super) fn text_with_link(
    id: impl Into<ElementId>,
    before: &str,
    link: &str,
    after: &str,
    url: String,
    app: WeakEntity<QuillApp>,
    cx: &App,
) -> InteractiveText {
    let mut text = String::from(before);
    let start = text.len();
    text.push_str(link);
    let range = start..text.len();
    text.push_str(after);
    let style = HighlightStyle {
        color: Some(cx.theme().primary),
        ..Default::default()
    };
    InteractiveText::new(
        id,
        StyledText::new(text).with_highlights(vec![(range.clone(), style)]),
    )
    .on_click(vec![range], move |_, _, cx| open_link_in(&app, &url, cx))
}

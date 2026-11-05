//! The one look for a list or panel with nothing to show yet: a kit icon in
//! a soft round tile (or a spinner while it loads), a title, a hint and an
//! optional action, composed from the kit's `Empty`. Telegram Desktop pairs a centered picture with one line
//! and a button (`Dialogs::InnerWidget::refreshEmpty`: "You have no
//! conversations yet." over New Message, "Edit folder" under an empty
//! folder); Quill draws the picture with Lucide icons instead of emoji so it
//! matches the rest of the chrome on every platform.
#![deny(clippy::indexing_slicing)]

use gpui_kit::assets::IconName;
use gpui_kit::component::empty::{
    Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::*;

/// Size of the round tile behind the icon.
const TILE: f32 = 56.;
/// Size of the icon inside the tile.
const ICON: f32 = 26.;

#[derive(IntoElement)]
pub(super) struct EmptyState {
    id: ElementId,
    picture: Picture,
    title: Option<SharedString>,
    hints: Vec<SharedString>,
    action: Option<AnyElement>,
}

enum Picture {
    Icon(IconName),
    Loading,
}

impl EmptyState {
    /// Nothing to show: `icon` names what would be here.
    pub(super) fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            picture: Picture::Icon(icon),
            title: None,
            hints: Vec::new(),
            action: None,
        }
    }

    /// Still loading: a spinner where the icon would be.
    pub(super) fn loading(id: impl Into<ElementId>) -> Self {
        Self {
            picture: Picture::Loading,
            ..Self::new(id, IconName::Loader)
        }
    }

    pub(super) fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// A muted line under the title; a `\n` starts another line.
    pub(super) fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        let hint: SharedString = hint.into();
        if hint.contains('\n') {
            self.hints.extend(
                hint.split('\n')
                    .map(|line| SharedString::from(line.to_string())),
            );
        } else if !hint.is_empty() {
            self.hints.push(hint);
        }
        self
    }

    /// What the person can do about it, usually a kit `Button`.
    pub(super) fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        // The kit's icon frame, grown into a round tile; a spinner sits
        // unframed in the same space while loading.
        let media = match self.picture {
            Picture::Icon(icon) => EmptyMedia::new()
                .with_variant(EmptyMediaVariant::Icon)
                .size(px(TILE))
                .rounded_full()
                .bg(theme.secondary)
                .child(Icon::new(icon).size(px(ICON)).text_color(muted)),
            Picture::Loading => EmptyMedia::new()
                .size(px(TILE))
                .child(Spinner::new().large().color(muted)),
        };
        let label = self
            .title
            .clone()
            .or_else(|| self.hints.first().cloned())
            .unwrap_or_default();
        let mut header = EmptyHeader::new().max_w(px(300.)).media(media);
        if let Some(title) = self.title {
            header = header.title(EmptyTitle::new().text_base().font_semibold().child(title));
        }
        if !self.hints.is_empty() {
            header = header.description(
                EmptyDescription::new()
                    .flex()
                    .flex_col()
                    .items_center()
                    .children(self.hints.into_iter().map(|line| div().child(line))),
            );
        }
        let mut empty = Empty::new()
            .flex_1()
            .w_full()
            .border_0()
            .px_6()
            .py_10()
            .header(header);
        if let Some(action) = self.action {
            empty = empty.content(EmptyContent::new().child(action));
        }
        div()
            .id(self.id)
            .flex_1()
            .flex()
            .role(Role::Status)
            .aria_label(label)
            .child(empty)
    }
}

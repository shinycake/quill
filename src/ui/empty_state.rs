//! The one look for a list or panel with nothing to show yet: a kit icon in
//! a soft round tile (or a spinner while it loads), a title, a hint and an
//! optional action. Telegram Desktop pairs a centered picture with one line
//! and a button (`Dialogs::InnerWidget::refreshEmpty`: "You have no
//! conversations yet." over New Message, "Edit folder" under an empty
//! folder); Quill draws the picture with Lucide icons instead of emoji so it
//! matches the rest of the chrome on every platform.
#![deny(clippy::indexing_slicing)]

use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
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
        let picture = match self.picture {
            Picture::Icon(icon) => div()
                .size(px(TILE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme.secondary)
                .child(Icon::new(icon).size(px(ICON)).text_color(muted))
                .into_any_element(),
            Picture::Loading => div()
                .size(px(TILE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().large().color(muted))
                .into_any_element(),
        };
        let label = self
            .title
            .clone()
            .or_else(|| self.hints.first().cloned())
            .unwrap_or_default();
        div()
            .id(self.id)
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_6()
            .py_10()
            .role(Role::Status)
            .aria_label(label)
            .child(div().pb_1().child(picture))
            .when_some(self.title, |this, title| {
                this.child(
                    div()
                        .max_w(px(300.))
                        .text_center()
                        .font_semibold()
                        .text_color(theme.foreground)
                        .child(title),
                )
            })
            .when(!self.hints.is_empty(), |this| {
                this.child(
                    div()
                        .max_w(px(300.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .text_sm()
                        .text_center()
                        .text_color(muted)
                        .children(self.hints.into_iter().map(|line| div().child(line))),
                )
            })
            .when_some(self.action, |this, action| {
                this.child(div().pt_2().child(action))
            })
    }
}

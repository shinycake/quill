//! `QuillApp` side of the main window's columns: the inputs read from
//! state, the shell row built from [`ShellLayout`], and the chat header's
//! back button for one-column windows.

use super::{
    DOWNLOADS_PANEL_WIDTH, ForumPane, INFO_PANEL_WIDTH, PanelPane, SHARED_MEDIA_PANEL_WIDTH,
    ShellInput, ShellLayout, chat_pane_width, max_list_width, shell_layout,
};
use crate::ui::app::{PaneMode, QuillApp};
use crate::ui::navigation::SidebarResize;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::auth::AuthView;
use quill::lang::{keys, tr};
use quill::state::ForumColumn;

impl QuillApp {
    /// The main window's columns this frame.
    pub(in crate::ui) fn shell_layout(&self) -> ShellLayout {
        shell_layout(self.shell_input())
    }

    /// Left and right space outside the conversation pane this frame. The
    /// sign-in, passcode and onboarding screens have no columns, so none.
    pub(in crate::ui) fn chat_insets(&self) -> (Pixels, Pixels) {
        if self.pane_mode() != PaneMode::Ready {
            return (px(0.), px(0.));
        }
        let (left, right) = self.shell_layout().chat_insets(self.folder_rail_width());
        (px(left), px(right))
    }

    /// The one input every column decision reads, forum peek included.
    fn shell_input(&self) -> ShellInput {
        let width = f32::from(self.frame.main_width);
        let session = self.session();
        ShellInput {
            width,
            rail: self.folder_rail_width(),
            list_width: f32::from(self.frame.sidebar_width),
            chat_open: session.is_some_and(|s| s.open_chat.is_some()),
            forum: session.map_or(ForumColumn::Hidden, |s| {
                s.forum_column(width, self.chat_list.forum_chats_peek)
            }),
            topic_open: session.is_some_and(|s| s.open_topic.is_some()),
            panels: self.side_panels_width(),
            docked: if self.pane_mode() == PaneMode::Ready {
                self.frame.sidebar.docked(std::time::Instant::now())
            } else {
                1.
            },
        }
    }

    /// Back from the conversation (Escape, the header's back button): the
    /// topic list first in a one-column forum, else the chat list.
    pub(in crate::ui) fn step_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shell_input().back_to_topics() {
            self.deselect_topic_ui(cx);
        } else {
            self.back_to_chat_list(window, cx);
        }
    }

    /// The chat list's width as laid out (the dragged width, narrowed so
    /// the chat keeps its minimum, or the whole window in one column).
    pub(in crate::ui) fn list_width(&self) -> Pixels {
        self.shell_layout()
            .list
            .map_or(self.frame.sidebar_width, px)
    }

    /// The widest the chat list may be dragged in this window.
    pub(in crate::ui) fn max_list_width(&self) -> Pixels {
        let input = self.shell_input();
        let forum_beside = input.forum == ForumColumn::Beside;
        px(max_list_width(input.width, input.rail, forum_beside))
    }

    /// Summed preferred width of the open side panels.
    fn side_panels_width(&self) -> Option<f32> {
        let widths = [
            self.info_panel_open().then_some(INFO_PANEL_WIDTH),
            self.downloads_panel_open().then_some(DOWNLOADS_PANEL_WIDTH),
            self.shared_media_panel_open()
                .then_some(SHARED_MEDIA_PANEL_WIDTH),
        ];
        widths.into_iter().flatten().reduce(|a, b| a + b)
    }

    /// The row under the title bar: chat list, forum topics, conversation
    /// and side panels, as [`ShellLayout`] places them.
    pub(in crate::ui) fn shell_columns(
        &mut self,
        auth: &AuthView,
        show_phone: bool,
        show_code: bool,
        show_password: bool,
        show_qr: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.frame.main_width = window.viewport_size().width;
        // Also records the peek and whether the topics are on screen.
        self.forum_column_layout(window);
        self.keep_shared_media_search_field(window, cx);
        let layout = self.shell_layout();
        self.settle_sidebar(&layout, window, cx);
        let input = self.shell_input();
        let layout = shell_layout(input);
        self.seed_history_width(chat_pane_width(input, &layout));
        let open = self.session().and_then(|s| s.open_chat);
        let list = layout.list.and_then(|list| {
            let column = if self.pane_mode() == PaneMode::Ready {
                self.docked_sidebar(&layout)?
            } else {
                self.sidebar(
                    auth,
                    show_phone,
                    show_code,
                    show_password,
                    show_qr,
                    window,
                    cx,
                )
            };
            Some((list, column))
        });
        let peek = self.sidebar_peek_layer(&layout, cx);
        let panels: Vec<AnyElement> = [
            self.info_panel(cx),
            self.downloads_panel(cx),
            self.shared_media_panel(cx),
        ]
        .into_iter()
        .flatten()
        .collect();
        let panels = match layout.panels {
            PanelPane::Hidden => None,
            PanelPane::Column(width) => Some(
                div()
                    .id("side-panels")
                    .role(gpui_kit::Role::Group)
                    .a11y_tag(quill::agent::ui_driver::redact::REGION_PANELS)
                    .flex()
                    .flex_none()
                    .w(px(width))
                    .h_full(),
            ),
            PanelPane::Fill => Some(
                div()
                    .id("side-panels")
                    .role(gpui_kit::Role::Group)
                    .a11y_tag(quill::agent::ui_driver::redact::REGION_PANELS)
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h_full(),
            ),
        }
        .map(|row| row.children(panels));
        let forum = match layout.forum {
            ForumPane::Hidden => None,
            pane => Some(self.forum_column_view(open, pane, cx)),
        };
        div()
            .id("quill-shell")
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            // Outside the peeking column (it blocks the mouse).
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                this.sidebar_pointer_moved_away(cx);
            }))
            // The chat list follows its resize edge while dragged.
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SidebarResize>, window, cx| {
                    let left = event.bounds.origin.x;
                    this.set_sidebar_width(
                        event.event.position.x - left - px(this.folder_rail_width()),
                        window,
                        cx,
                    );
                }),
            )
            .when_some(list, |this, (_, column)| {
                this.child(column).when(layout.resize_handle(), |this| {
                    this.child(self.sidebar_resize_handle(cx))
                })
            })
            .children(forum)
            .when(layout.chat, |this| this.child(self.conversation_slot(cx)))
            .children(panels)
            .children(peek)
            .children(self.forward_drag_banner(&layout, cx))
            .into_any_element()
    }

    /// Messages dragged over the chat list: the conversation dims under a
    /// "Forward to…" prompt (`MainWidget::showDragForwardInfo`, tdesktop's
    /// `HistoryHider`). Not in one column, where the list is never beside it.
    fn forward_drag_banner(
        &self,
        layout: &ShellLayout,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !layout.chat || !self.forward_banner_shown(cx) {
            return None;
        }
        let (left, right) = layout.chat_insets(self.folder_rail_width());
        let theme = cx.theme();
        Some(
            div()
                .id("forward-drag-banner")
                .test_support()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(left))
                .right(px(right))
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.background.opacity(0.7))
                .child(
                    div()
                        .px_4()
                        .py_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .text_color(theme.popover_foreground)
                        .text_sm()
                        .font_medium()
                        .child(tr(keys::FORWARD_CHOOSE)),
                )
                .into_any_element(),
        )
    }

    /// One column with the chat on screen: the header's way back to the
    /// topics of the open forum, or to the chat list.
    pub(in crate::ui) fn chat_back_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        if self.windows.scope.is_some() || !self.shell_layout().chat_back() {
            return None;
        }
        Some(
            Button::new("chat-back")
                .icon(gpui_kit::assets::IconName::ChevronLeft)
                .ghost()
                .tooltip(tr(keys::Q_CHAT_BACK))
                .accessibility_label(tr(keys::Q_CHAT_BACK))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.step_back(window, cx);
                })),
        )
    }
}

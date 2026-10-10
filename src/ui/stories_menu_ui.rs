//! Right-click menu of a tile in the stories strip (tdesktop
//! `FillSourceMenu`; the items are decided in `quill::stories_menu`).

use super::app::QuillApp;
use super::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::stories_menu::{StoryMenuAction, StoryMenuFacts, story_menu};
use quill::telegram::envelope::{ChatKind, StoryListView};

impl QuillApp {
    pub(super) fn open_story_menu(
        &mut self,
        chat_id: i64,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.global.story_menu = Some((chat_id, position));
        cx.notify();
    }

    fn run_story_menu_action(
        &mut self,
        chat_id: i64,
        action: StoryMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = ChatId(chat_id);
        match action {
            StoryMenuAction::Open => self.select_listed_chat(id, window, cx),
            StoryMenuAction::Profile => {
                if let Some(target) = self
                    .session()
                    .and_then(|s| s.info_panel_target_for_chat(id))
                {
                    self.open_info_panel_target(target, window, cx);
                }
            }
            StoryMenuAction::ToggleHidden => {
                let hide = !self.story_tile_hidden(chat_id);
                let sent = self
                    .live
                    .as_mut()
                    .map(|live| live.driver.set_chat_active_stories_list(id, hide));
                self.status_note = match sent {
                    Some(Ok(_)) if hide => "Hiding stories…".into(),
                    Some(Ok(_)) => "Showing stories…".into(),
                    Some(Err(_)) => "Could not change the stories list".into(),
                    None => "demo — setChatActiveStoriesList runs with live TDLib".into(),
                };
            }
            StoryMenuAction::ToggleMute => {
                let mute = !self.story_tile_muted(chat_id);
                self.apply_chat_story_mute(id, mute, cx);
            }
        }
        cx.notify();
    }

    fn story_tile_hidden(&self, chat_id: i64) -> bool {
        self.session()
            .and_then(|s| s.story_tray.get(&chat_id))
            .is_some_and(|tray| tray.list == Some(StoryListView::Archive))
    }

    fn story_tile_muted(&self, chat_id: i64) -> bool {
        self.session().is_some_and(|s| {
            s.chats
                .get(&chat_id)
                .is_some_and(|chat| s.effective_story_muted(chat))
        })
    }

    pub(super) fn story_menu_overlay(
        &self,
        chat_id: i64,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = self
            .session()
            .and_then(|s| s.chats.get(&chat_id))
            .map(|chat| chat.kind.clone())
            .unwrap_or(ChatKind::Unknown);
        let is_self = matches!(kind, ChatKind::Private { user_id }
            if self.session().and_then(|s| s.my_user_id) == Some(user_id.0));
        let items = story_menu(&StoryMenuFacts {
            kind: &kind,
            is_self,
            hidden: self.story_tile_hidden(chat_id),
            muted: self.story_tile_muted(chat_id),
        });
        let hover = cx.theme().accent;
        let rows: Vec<AnyElement> = items
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let icon = match entry.action {
                    StoryMenuAction::Open => IconName::MessageCircle,
                    StoryMenuAction::Profile => IconName::CircleUser,
                    StoryMenuAction::ToggleHidden => IconName::EyeOff,
                    StoryMenuAction::ToggleMute => IconName::BellOff,
                };
                div()
                    .id(("story-menu-item", i))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .text_color(text_menu())
                    .hover(|style| style.bg(hover))
                    .role(Role::MenuItem)
                    .aria_label(entry.label)
                    .child(Icon::new(icon).size(px(16.)))
                    .child(entry.label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.global.story_menu = None;
                        this.run_story_menu_action(chat_id, entry.action, window, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        let panel = div()
            .id("story-menu-panel")
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(200.))
            .px_1()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(accent())
            .bg(bg_canvas())
            .children(rows);
        div()
            .id("story-menu-overlay")
            .track_focus(&self.context_menu_focus)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                div()
                    .id("story-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.global.story_menu = None;
                        cx.notify();
                    })),
            )
            .child(
                anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .focus_trap("story-menu-focus", &self.context_menu_focus)
            .into_any_element()
    }
}

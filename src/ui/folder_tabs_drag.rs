//! Drag a folder tab onto another to reorder the folders, in the strip
//! above the chat list and in the folder column (tdesktop
//! `ChatsFiltersTabsReorder` and `VerticalLayoutReorder` in
//! `ui/widgets/chat_filters_tabs_strip.cpp` and
//! `window/window_filters_menu.cpp`). "All chats" moves too with Premium;
//! without it, it stays first and nothing can take its slot
//! (`quill::folder_order`).

use super::app::QuillApp;
use super::folder_tabs::FolderSlotKind;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use quill::folder_order::{FolderOrder, FolderTab};

#[derive(Clone)]
pub(super) struct FolderTabDrag {
    pub tab: FolderTab,
    pub name: SharedString,
}

impl Render for FolderTabDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.15))
            .text_sm()
            .child(self.name.clone())
    }
}

pub(super) fn slot_tab(kind: FolderSlotKind) -> Option<FolderTab> {
    match kind {
        FolderSlotKind::All => Some(FolderTab::Main),
        FolderSlotKind::Folder(id) => Some(FolderTab::Folder(id)),
        FolderSlotKind::Unread | FolderSlotKind::Archived => None,
    }
}

/// Makes a tab element draggable onto, and a drop target for, the other
/// folder tabs. Without Premium "All chats" is neither.
pub(super) fn folder_tab_drag<E>(
    element: E,
    kind: FolderSlotKind,
    name: SharedString,
    premium: bool,
    weak: &WeakEntity<QuillApp>,
) -> E
where
    E: StatefulInteractiveElement,
{
    let Some(tab) = slot_tab(kind) else {
        return element;
    };
    if !FolderOrder::can_drag(tab, premium) {
        return element;
    }
    let weak = weak.clone();
    element
        .on_drag(
            FolderTabDrag { tab, name },
            |drag: &FolderTabDrag, _, _, cx| cx.new(|_| drag.clone()),
        )
        .drag_over::<FolderTabDrag>(|style, _, _, cx| style.bg(cx.theme().accent.opacity(0.1)))
        .on_drop(move |drag: &FolderTabDrag, _, cx| {
            let moved = drag.tab;
            let _ = weak.update(cx, |this, cx| this.drop_folder_tab(moved, tab, cx));
        })
}

impl QuillApp {
    pub(super) fn drop_folder_tab(
        &mut self,
        moved: FolderTab,
        onto: FolderTab,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let premium = session.my_is_premium();
        let Some(order) = session.chat_list.folder_order().moved(moved, onto, premium) else {
            return;
        };
        self.send_folder_order(&order, cx);
    }

    fn demo_folder_tabs(&mut self) {
        let position = std::env::var("QUILL_DEMO_MAIN_LIST_POSITION")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        if std::env::var_os("QUILL_DEMO_FOLDER_RAIL").is_some() {
            self.appearance.folder_tabs_view = quill::folder_icons::FolderTabsView::Left;
        }
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        session.payments.premium_option = Some(true);
        session.chat_list.chat_folders = [
            (1, "Work", "Work"),
            (2, "Family", "Love"),
            (3, "News", "Channels"),
        ]
        .into_iter()
        .map(
            |(id, name, icon)| quill::telegram::envelope::ChatFolderInfo {
                id,
                name: name.into(),
                icon_name: icon.into(),
                color_id: -1,
                is_shareable: false,
                has_my_invite_links: false,
            },
        )
        .collect();
        session.chat_list.main_list_position = position;
    }
}

crate::ui::screenshot_demo::register_demos![
    // `QUILL_DEMO_MAIN_LIST_POSITION` places "All chats" among the folders;
    // `QUILL_DEMO_FOLDER_RAIL=1` shows the folder column.
    crate::ui::screenshot_demo::DemoSpec::chat_list("ready-folder-tabs")
        .setup(|app, _, _| app.demo_folder_tabs()),
];

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "folder_tabs_drag_tests.rs"]
mod tests;

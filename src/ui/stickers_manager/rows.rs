//! One row of the stickers manager box, drawn like tdesktop's
//! `StickersBox::Inner::paintRow`: a reorder handle on installed rows, the
//! first sticker of the set, its title and count, and one button at the end
//! (remove, Undo, Add or Added).
use super::{Kind, ManagerTab};
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::badge::Badge;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr, trn};
use quill::local_path::sandboxed_display_path;
use quill::state::{RequestPurpose, StickersPurpose};
use quill::telegram::envelope::{StickerItem, StickerSetInfo};

/// The size of the set thumbnail (tdesktop `stickersRowItem.photoSize`).
const THUMB_SIZE: f32 = 36.;

/// What a row shows besides the set itself.
#[derive(Clone, Copy)]
pub(super) struct RowState {
    pub kind: Kind,
    pub tab: ManagerTab,
    /// Marked for removal in this box: the row dims and offers Undo.
    pub removed: bool,
    /// A request for this set is on its way.
    pub pending: bool,
}

/// The row being dragged, as the pointer carries it.
#[derive(Clone)]
pub(super) struct ManagerDrag {
    pub kind: Kind,
    pub id: i64,
    pub title: String,
}

impl Render for ManagerDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(cx.theme().accent.opacity(0.15))
            .text_sm()
            .child(self.title.clone())
    }
}

impl QuillApp {
    /// A request that changes this set is in flight.
    pub(super) fn manager_set_pending(&self, kind: Kind, set_id: i64) -> bool {
        self.session().is_some_and(|session| match kind {
            Kind::Masks => session.mask_change_pending(),
            Kind::Stickers => [(true, false), (false, true), (false, false)]
                .into_iter()
                .any(|(installed, archived)| {
                    session.requests.has_purpose(RequestPurpose::Stickers(
                        StickersPurpose::ManageStickerSet {
                            set_id,
                            installed,
                            archived,
                        },
                    ))
                }),
        })
    }

    /// The first sticker of the set: its downloaded picture, or its emoji
    /// until the picture is there.
    fn manager_thumb(&self, cover: Option<&StickerItem>) -> AnyElement {
        let roots = self.media_display_roots();
        let path = cover
            .and_then(StickerItem::display_file_id)
            .and_then(|id| {
                self.session()
                    .and_then(|session| session.media.files.get(&id.0))
                    .and_then(|file| file.usable_path())
            })
            .and_then(|path| sandboxed_display_path(path, &roots));
        let cell = div()
            .flex_none()
            .size(px(THUMB_SIZE))
            .flex()
            .items_center()
            .justify_center();
        match (path, cover) {
            (Some(path), _) => cell
                .child(
                    img(path)
                        .size(px(THUMB_SIZE))
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            (None, Some(cover)) => cell.child(cover.emoji.clone()).into_any_element(),
            (None, None) => cell.into_any_element(),
        }
    }

    fn manager_action(
        &self,
        set: &StickerSetInfo,
        row: RowState,
        title: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = set.id;
        let kind = row.kind;
        match row.tab {
            ManagerTab::Installed if row.removed => {
                Button::new(format!("manager-undo-{id}"))
                    .label(tr(keys::STICKERS_RETURN))
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.manager_set_removed(kind, id, false, cx)
                    }))
                    .into_any_element()
            }
            ManagerTab::Installed => Button::new(format!("manager-remove-{id}"))
                .icon(IconName::Trash)
                .ghost()
                .small()
                .tooltip(tr(keys::BOX_REMOVE))
                .accessibility_label(format!("{} {title}", tr(keys::BOX_REMOVE)))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.manager_set_removed(kind, id, true, cx)),
                )
                .into_any_element(),
            ManagerTab::Trending if set.is_installed && !row.removed => {
                Button::new(format!("manager-added-{id}"))
                    .label(tr(keys::STICKERS_FEATURED_INSTALLED))
                    .outline()
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.manager_set_removed(kind, id, true, cx)
                    }))
                    .into_any_element()
            }
            ManagerTab::Trending | ManagerTab::Archived => Button::new(format!("manager-add-{id}"))
                .label(tr(keys::STICKERS_FEATURED_ADD))
                .primary()
                .small()
                .loading(row.pending)
                .disabled(row.pending)
                .on_click(cx.listener(move |this, _, _, cx| this.manager_add(kind, id, cx)))
                .into_any_element(),
        }
    }

    pub(super) fn manager_row(
        &self,
        set: &StickerSetInfo,
        row: RowState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = set.id;
        let kind = row.kind;
        let title: String = if set.title.is_empty() {
            set.name.clone()
        } else {
            set.title.clone()
        };
        let count = if set.size == 0 {
            tr(keys::CONTACTS_LOADING).to_string()
        } else {
            match kind {
                Kind::Stickers => trn(keys::STICKERS_COUNT, set.size),
                Kind::Masks => trn(keys::MASKS_COUNT, set.size),
            }
        };
        let draggable = row.tab == ManagerTab::Installed && !row.removed;
        let handle = div()
            .id(format!("manager-handle-{id}"))
            .flex_none()
            .w(px(20.))
            .h(px(THUMB_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .when(row.tab == ManagerTab::Installed, |handle| {
                handle.child(Icon::new(IconName::GripVertical).size_4())
            })
            .when(draggable, |handle| {
                let drag_title = title.clone();
                handle.cursor_move().on_drag(
                    ManagerDrag {
                        kind,
                        id,
                        title: drag_title,
                    },
                    |drag: &ManagerDrag, _, _, cx| cx.new(|_| drag.clone()),
                )
            });
        let heading = div().truncate().font_medium().child(title.clone());
        let heading = if row.tab == ManagerTab::Trending && !set.is_viewed {
            Badge::new().dot().child(heading).into_any_element()
        } else {
            heading.into_any_element()
        };
        h_flex()
            .id(format!("manager-row-{id}"))
            .w_full()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .when(row.removed && row.tab == ManagerTab::Installed, |row| {
                row.opacity(0.5)
            })
            .when(row.tab == ManagerTab::Installed, |row| {
                row.drag_over::<ManagerDrag>(|style, _, _, cx| {
                    style.bg(cx.theme().accent.opacity(0.1))
                })
                .on_drop(cx.listener(move |this, drag: &ManagerDrag, _, cx| {
                    if drag.kind == kind {
                        this.manager_dropped(kind, drag.id, id, cx);
                    }
                }))
            })
            .child(handle)
            .child(
                Button::new(format!("manager-open-{id}"))
                    .ghost()
                    .flex_1()
                    .min_w_0()
                    .h_auto()
                    .justify_start()
                    .accessibility_label(title.clone())
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.view_message_sticker_set(id, cx)),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_3()
                            .child(self.manager_thumb(set.covers.first()))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .items_start()
                                    .child(heading)
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(count),
                                    ),
                            ),
                    ),
            )
            .child(self.manager_action(set, row, &title, cx))
            .into_any_element()
    }
}

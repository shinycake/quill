//! The set box's ⋯ menu and the owned set's "+" cell (tdesktop
//! `StickerSetBox::prepare` menu and `Inner::paintAddCell`).
use super::*;
use quill::lang::{keys, tr};
use quill::sticker_set_box::{AddEntry, STICKERS_BOT_LINK};

impl QuillApp {
    /// The ⋯ button: Share, Copy Link, then for the creator "Edit name"
    /// and "Reorder", Archive for an installed sticker set, and the
    /// creator's "Delete Pack" with a pointer to the @stickers bot.
    pub(super) fn sticker_set_more_button(
        &self,
        set_id: i64,
        kind: SetKind,
        link: String,
        archivable: bool,
        is_owned: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let owner = cx.entity().downgrade();
        Button::new("sticker-set-more")
            .icon(IconName::Ellipsis)
            .ghost()
            .tooltip(tr(keys::Q_MEDIA_MORE))
            .accessibility_label(tr(keys::Q_MEDIA_MORE))
            .dropdown_menu(move |menu, window, cx| {
                let share = owner.clone();
                let copy = owner.clone();
                let (share_link, copy_link) = (link.clone(), link.clone());
                let mut menu = menu
                    .item(
                        PopupMenuItem::new(share_label(kind)).on_click(move |_, window, cx| {
                            let link = share_link.clone();
                            let _ = share.update(cx, |this, cx| {
                                this.share_sticker_set_link(link, window, cx);
                            });
                        }),
                    )
                    .item(PopupMenuItem::new(tr(keys::CONTEXT_COPY_LINK)).on_click(
                        move |_, _, cx| {
                            let link = copy_link.clone();
                            let _ = copy.update(cx, |this, cx| {
                                this.copy_sticker_set_link(link, kind, cx);
                            });
                        },
                    ));
                if is_owned {
                    let rename = owner.clone();
                    let reorder = owner.clone();
                    menu = menu
                        .item(PopupMenuItem::new(tr(keys::Q_MEDIA_EDIT_NAME)).on_click(
                            move |_, window, cx| {
                                let _ = rename.update(cx, |this, cx| {
                                    this.open_rename_set(window, cx);
                                });
                            },
                        ))
                        .item(PopupMenuItem::new(tr(keys::Q_MEDIA_REORDER)).on_click(
                            move |_, _, cx| {
                                let _ = reorder.update(cx, |this, cx| {
                                    this.begin_edit_stickers(cx);
                                });
                            },
                        ));
                }
                if archivable {
                    let archive = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tr(keys::Q_MEDIA_ARCHIVE_STICKERS)).on_click(
                            move |_, window, cx| {
                                let _ = archive.update(cx, |this, cx| {
                                    this.archive_viewed_sticker_set(set_id, window, cx);
                                });
                            },
                        ),
                    );
                }
                if is_owned {
                    let everyone = owner.clone();
                    let myself = owner.clone();
                    menu = menu
                        .submenu(
                            tr(keys::Q_MEDIA_DELETE_PACK),
                            window,
                            cx,
                            move |sub, _, _| {
                                let everyone = everyone.clone();
                                let myself = myself.clone();
                                sub.item(
                                    PopupMenuItem::new(tr(keys::Q_MEDIA_DELETE_FOR_EVERYONE))
                                        .on_click(move |_, _, cx| {
                                            let _ = everyone.update(cx, |this, cx| {
                                                this.sticker_box.delete_set_open = true;
                                                cx.notify();
                                            });
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new(tr(keys::Q_MEDIA_DELETE_FOR_MYSELF))
                                        .on_click(move |_, _, cx| {
                                            let _ = myself.update(cx, |this, cx| {
                                                this.sticker_box.remove_set_open = true;
                                                cx.notify();
                                            });
                                        }),
                                )
                            },
                        )
                        .separator()
                        .link(
                            tr(keys::Q_MEDIA_CHECK_THE_STICKERS_BOT_FOR),
                            STICKERS_BOT_LINK,
                        );
                }
                menu
            })
    }

    /// The round "+" cell after an owned set's stickers: a menu that adds
    /// a new image or an existing sticker (and, for emoji, adapts a
    /// sticker).
    pub(super) fn sticker_set_add_cell(
        &self,
        set_id: i64,
        kind: SetKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let owner = cx.entity().downgrade();
        div()
            .id("sticker-set-add-cell")
            .size(px(72.))
            .p_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                // tdesktop's round add cell: a 56 px disc of subtle text color.
                div()
                    .size(px(56.))
                    .rounded_full()
                    .bg(cx.theme().muted_foreground.opacity(0.12))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Button::new("sticker-set-add")
                            .icon(IconName::Plus)
                            .ghost()
                            .rounded(ButtonRounded::Size(px(28.)))
                            .tooltip(AddEntry::CreateNew.label(kind))
                            .accessibility_label(AddEntry::CreateNew.label(kind))
                            .dropdown_menu(move |menu, _, _| {
                                let mut menu = menu;
                                for entry in kind.add_menu() {
                                    let entry = *entry;
                                    let owner = owner.clone();
                                    menu =
                                        menu.item(PopupMenuItem::new(entry.label(kind)).on_click(
                                            move |_, _, cx| {
                                                let _ = owner.update(cx, |this, cx| match entry {
                                                    AddEntry::CreateNew => {
                                                        this.begin_create_sticker(set_id, kind, cx)
                                                    }
                                                    AddEntry::AddExisting => this
                                                        .open_pick_existing(
                                                            set_id, kind, false, cx,
                                                        ),
                                                    AddEntry::AdaptSticker => this
                                                        .open_pick_existing(set_id, kind, true, cx),
                                                });
                                            },
                                        ));
                                }
                                menu
                            }),
                    ),
            )
            .into_any_element()
    }
}

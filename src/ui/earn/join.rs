//! The affiliate programs page of a user, bot or channel (tdesktop
//! `BotStarRef::Join`): what the programs are, "My Programs" and the
//! sorted "Programs" list, with the rows the programs box reuses.

use super::*;
use crate::ui::scroll_load::scrolled_within;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use quill::earn::*;
use quill::lang::{keys, tr, tr_with};

/// How many screens ahead of the end the next page loads (tdesktop
/// `PeerListContent`'s `PreloadHeightsCount`).
const PRELOAD_SCREENS: f32 = 3.0;

/// Whether a scroll handle sits within `PRELOAD_SCREENS` of the end of its
/// content (`ui/scroll_load.rs`).
pub(super) fn near_end(handle: &ScrollHandle) -> bool {
    scrolled_within(handle, handle.bounds().size.height * PRELOAD_SCREENS)
}

/// One program in a list: what the row shows and what it opens.
#[derive(Clone)]
pub(super) struct ProgramRow {
    pub bot_user_id: i64,
    pub params: ProgramParams,
    /// A joined program (the link badge, the link box, "Copy Link").
    pub joined: Option<ConnectedProgram>,
}

impl ProgramRow {
    pub(super) fn found(program: &FoundProgram) -> Self {
        Self {
            bot_user_id: program.bot_user_id,
            params: program.info.params,
            joined: None,
        }
    }

    pub(super) fn mine(program: &ConnectedProgram) -> Self {
        Self {
            bot_user_id: program.bot_user_id,
            params: program.params,
            joined: Some(program.clone()),
        }
    }
}

/// The commission badge before the duration (tdesktop `Row::refreshStatus`).
fn commission_badge(permille: i32, cx: &App) -> Div {
    div()
        .px_1()
        .rounded_sm()
        .bg(cx.theme().primary)
        .text_xs()
        .font_semibold()
        .text_color(cx.theme().primary_foreground)
        .child(format_commission(permille))
}

impl QuillApp {
    /// A program row: the bot's photo (with the link badge when joined),
    /// the name and "20% · 1 year".
    pub(super) fn program_row(
        &self,
        session: &Session,
        id: impl Into<ElementId>,
        row: ProgramRow,
        for_box: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bot = EarnPeer::User(row.bot_user_id);
        let name = peer_name(session, bot);
        let mut avatar = div().relative().child(self.earn_avatar(session, bot, 40.));
        if row.joined.is_some() {
            avatar = avatar.child(
                div()
                    .absolute()
                    .right(px(-2.))
                    .bottom(px(-2.))
                    .size(px(18.))
                    .rounded_full()
                    .bg(cx.theme().primary)
                    .border_2()
                    .border_color(cx.theme().background)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::Link)
                            .size(px(10.))
                            .text_color(cx.theme().primary_foreground),
                    ),
            );
        }
        let joined = row.joined.clone();
        let bot_user_id = row.bot_user_id;
        let click = cx.listener(move |this, _, _, cx| {
            this.program_clicked(bot_user_id, joined.clone(), for_box, cx);
        });
        let owner = cx.entity().downgrade();
        let menu_link = row.joined.clone();
        div()
            .id(id.into())
            .debug_selector(move || format!("earn-program-{bot_user_id}"))
            .flex()
            .items_center()
            .gap_3()
            .py_1p5()
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(name.clone())
            .tab_index(0)
            .hover(|s| s.bg(cx.theme().secondary))
            .on_click(click)
            .child(avatar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .min_w_0()
                    .flex_1()
                    .child(div().text_sm().font_semibold().truncate().child(name))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(commission_badge(row.params.commission_per_mille, cx))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_muted())
                                    .child(format_program_duration(row.params.month_count)),
                            ),
                    ),
            )
            .context_menu(move |menu, _, _| {
                let open = owner.clone();
                let mut menu = menu.item(
                    PopupMenuItem::new(tr(keys::Q_MONEY_OPEN_APP))
                        .icon(IconName::Bot)
                        .on_click(move |_, window, cx| {
                            let _ = open.update(cx, |this, cx| {
                                this.open_user_chat(bot_user_id, window, cx);
                            });
                        }),
                );
                if let Some(program) = menu_link.clone().filter(|p| !p.url.is_empty()) {
                    let copy = owner.clone();
                    let leave = owner.clone();
                    let url = program.url.clone();
                    menu = menu
                        .item(
                            PopupMenuItem::new(tr(keys::CONTEXT_COPY_LINK))
                                .icon(IconName::Copy)
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                                    let _ = copy.update(cx, |this, cx| {
                                        this.connection
                                            .set_note(StatusNote::info(tr(keys::STICKERS_COPIED)));
                                        cx.notify();
                                    });
                                }),
                        )
                        .item(
                            PopupMenuItem::new(tr(keys::BOX_LEAVE))
                                .icon(IconName::LogOut)
                                .on_click(move |_, _, cx| {
                                    let program = program.clone();
                                    let _ = leave.update(cx, |this, cx| {
                                        this.ask_revoke(program, cx);
                                    });
                                }),
                        );
                }
                menu
            })
            .into_any_element()
    }

    /// tdesktop `rowClicked`: a joined program opens its link, any other
    /// asks TDLib whether it is joined first.
    fn program_clicked(
        &mut self,
        bot_user_id: i64,
        joined: Option<ConnectedProgram>,
        for_box: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(program) = joined {
            let recipient = self
                .earn_state()
                .and_then(|e| e.join.as_ref())
                .map(|j| j.peer);
            if let (Some(recipient), Some(earn)) = (recipient, self.earn_mut()) {
                earn.link_box = Some(LinkBox { program, recipient });
            }
            cx.notify();
            return;
        }
        self.earn_call(tr(keys::Q_MONEY_COULDNT_OPEN_THE_PROGRAM), cx, |driver| {
            driver.open_affiliate_program(bot_user_id, for_box)
        });
    }

    /// "Leave": the "Revoke Link" box.
    fn ask_revoke(&mut self, program: ConnectedProgram, cx: &mut Context<Self>) {
        let recipient = self
            .earn_state()
            .and_then(|e| e.join.as_ref())
            .map(|j| j.peer);
        if let (Some(recipient), Some(earn)) = (recipient, self.earn_mut()) {
            earn.revoke = Some(RevokeBox {
                program,
                recipient,
                sending: false,
            });
        }
        cx.notify();
    }

    /// The join page.
    pub(super) fn join_page(&self, peer: EarnPeer, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let primary = cx.theme().primary;
        let mut page = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .pt_2()
                    .child(
                        div()
                            .size(px(64.))
                            .rounded_full()
                            .bg(primary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Handshake)
                                    .size(px(30.))
                                    .text_color(cx.theme().primary_foreground),
                            ),
                    )
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .child(tr(keys::Q_MONEY_AFFILIATE_PROGRAMS)),
                    )
                    .child(about(tr(keys::STAR_REF_LIST_ABOUT_CHANNEL)).text_center()),
            )
            .child(info_point(
                IconName::ShieldCheck,
                tr(keys::Q_MONEY_RELIABLE),
                tr(keys::STAR_REF_RELIABLE_ABOUT),
                cx,
            ))
            .child(info_point(
                IconName::Eye,
                tr(keys::Q_MONEY_TRANSPARENT),
                tr(keys::STAR_REF_TRANSPARENT_ABOUT),
                cx,
            ))
            .child(info_point(
                IconName::ThumbsUp,
                tr(keys::Q_MONEY_SIMPLE),
                tr(keys::STAR_REF_SIMPLE_ABOUT),
                cx,
            ))
            .child(divider(cx));
        let Some(join) = session
            .payments
            .earn
            .join
            .as_ref()
            .filter(|j| j.peer == peer)
        else {
            return page
                .child(div().flex().justify_center().py_4().child(Spinner::new()))
                .into_any_element();
        };
        if !join.mine.programs.is_empty() {
            let mut mine = div()
                .flex()
                .flex_col()
                .child(header(tr(keys::Q_MONEY_MY_PROGRAMS), cx));
            for (index, program) in join.mine.programs.iter().enumerate() {
                mine = mine.child(self.program_row(
                    session,
                    ("earn-mine", index),
                    ProgramRow::mine(program),
                    false,
                    cx,
                ));
            }
            page = page.child(mine).child(divider(cx));
        }
        if !join.found.programs.is_empty() {
            let mut found = div().flex().flex_col().child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(header(tr(keys::Q_MONEY_PROGRAMS), cx))
                    .child(self.sort_menu(join.sort, cx)),
            );
            for (index, program) in join.found.programs.iter().enumerate() {
                found = found.child(self.program_row(
                    session,
                    ("earn-found", index),
                    ProgramRow::found(program),
                    false,
                    cx,
                ));
            }
            page = page.child(found);
        }
        if join.mine.loading || join.found.loading {
            page = page.child(div().flex().justify_center().py_2().child(Spinner::new()));
        }
        page.into_any_element()
    }

    /// "Sort by Profitability" and its menu (tdesktop `setupSort`).
    fn sort_menu(&self, sort: ProgramSort, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        Button::new("earn-sort")
            .label(tr_with(keys::Q_MONEY_SORT_BY, &[("label", (sort.label()))]))
            .ghost()
            .xsmall()
            .dropdown_menu(move |mut menu, _, _| {
                for order in ProgramSort::ALL {
                    let owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(order.label())
                            .checked(order == sort)
                            .on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    this.earn_call(
                                        tr(keys::Q_MONEY_COULDNT_LOAD_THE_PROGRAMS),
                                        cx,
                                        |driver| driver.set_starref_sort(order),
                                    );
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// Per-frame: the bots' photos and the next page at the end of a list.
    pub(super) fn join_upkeep(&mut self) {
        let Some(earn) = self.earn_state() else {
            return;
        };
        let mut bots: Vec<i64> = Vec::new();
        if let Some(join) = earn.join.as_ref() {
            bots.extend(join.mine.programs.iter().map(|p| p.bot_user_id));
            bots.extend(join.found.programs.iter().map(|p| p.bot_user_id));
        }
        if let Some(list) = earn.programs_box.as_ref() {
            bots.extend(list.programs.iter().map(|p| p.bot_user_id));
        }
        let page_more = earn.join.as_ref().map(|join| {
            (
                !join.mine.loading && !join.mine.next_offset.is_empty(),
                !join.found.loading && !join.found.next_offset.is_empty(),
            )
        });
        let box_more = earn
            .programs_box
            .as_ref()
            .is_some_and(|list| !list.loading && !list.next_offset.is_empty());
        let join_open = matches!(
            self.session().and_then(|s| s.users_state.open_info_panel),
            Some(InfoPanelTarget::Earn(EarnPanel::Join { .. }))
        );
        let Some(live) = self.live.as_mut() else {
            return;
        };
        for bot in bots {
            if live.driver.session.user_photo_path(bot).is_none()
                && self.earn.photos_asked.insert(bot)
            {
                let _ = live.driver.download_user_photo(bot);
            }
        }
        if join_open && near_end(&self.earn.scroll) {
            // tdesktop loads "My Programs" to the end before the rest.
            match page_more {
                Some((true, _)) => {
                    let _ = live.driver.more_starref_programs(true, false);
                }
                Some((false, true)) => {
                    let _ = live.driver.more_starref_programs(false, false);
                }
                _ => {}
            }
        }
        if box_more && near_end(&self.earn.box_scroll) {
            let _ = live.driver.more_starref_programs(false, true);
        }
    }
}

//! A community's picture and its collapsed chat-list row ("Show as One
//! Chat"), after tdesktop `PaintCommunityUserpicEffect` and the community
//! entry of `Dialogs::Row`: a rounded-square userpic with two cards
//! peeking out to its left, the joined chats' names as the preview, and
//! the unread-chats badge.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::chat_list_keys::ListRow;
use crate::ui::chat_row::{CHAT_ROW_AVATAR, chat_row_height};
use crate::ui::context_menu_keys::context_menu_item;
use crate::ui::dismiss_layer::DismissLayer;
use crate::ui::menu_surface::MenuSurface;
use crate::ui::pressable::PressableDiv;
use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use quill::lang::trn_with;
use quill::telegram::envelope::{ChatNotificationSettings, MUTE_FOREVER};

/// Strip to the left of the userpic the cards peek into (`kPeek`).
const PEEK: f32 = 0.3;
/// Rotation point, as a share of the size down the left edge (`kPivotY`).
const PIVOT_Y: f32 = 0.75;
/// The gap carved around each shape (`kGap`).
const GAP: f32 = 0.02;
/// The userpic's corner radius (`ForumUserpicRadiusMultiplier`).
const ROUNDING: f32 = 0.3;

/// One card behind the userpic: scale, corner share, angle, opacity
/// (`kCard1*`, `kCard2*`).
struct Card {
    scale: f32,
    round: f32,
    angle: f32,
    opacity: f32,
}

const FRONT_CARD: Card = Card {
    scale: 0.82,
    round: 0.22,
    angle: -9.,
    opacity: 0.5,
};
const BACK_CARD: Card = Card {
    scale: 0.7,
    round: 0.16,
    angle: -16.,
    opacity: 0.3,
};

/// The rounded rect `x, y, w, h` with corner `r`, as points rotated by
/// `angle` degrees around `pivot` (clockwise on screen for a positive
/// angle, as `QPainter::rotate`).
fn rounded_rect_points(
    (x, y, w, h): (f32, f32, f32, f32),
    r: f32,
    angle: f32,
    pivot: (f32, f32),
) -> Vec<(f32, f32)> {
    const STEPS: usize = 8;
    let r = r.min(w / 2.).min(h / 2.).max(0.);
    let corners = [
        (x + w - r, y + r, -90.0_f32),
        (x + w - r, y + h - r, 0.),
        (x + r, y + h - r, 90.),
        (x + r, y + r, 180.),
    ];
    let (sin, cos) = angle.to_radians().sin_cos();
    corners
        .iter()
        .flat_map(|&(cx, cy, start)| {
            (0..=STEPS).map(move |step| {
                let a = (start + 90. * step as f32 / STEPS as f32).to_radians();
                (cx + r * a.cos(), cy + r * a.sin())
            })
        })
        .map(|(px_, py_)| {
            let (dx, dy) = (px_ - pivot.0, py_ - pivot.1);
            (pivot.0 + dx * cos - dy * sin, pivot.1 + dx * sin + dy * cos)
        })
        .collect()
}

fn fill_points(origin: Point<Pixels>, points: &[(f32, f32)], color: Hsla, window: &mut Window) {
    let points: Vec<Point<Pixels>> = points
        .iter()
        .map(|&(x, y)| point(origin.x + px(x), origin.y + px(y)))
        .collect();
    let mut path = PathBuilder::fill();
    path.add_polygon(&points, true);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// The two cards, drawn into a box `size * PEEK` wider than the userpic
/// on its left. tdesktop carves the gaps transparent; here they take
/// `surface`, the color under the picture.
fn paint_cards(origin: Point<Pixels>, size: f32, color: Hsla, surface: Hsla, window: &mut Window) {
    let peek = size * PEEK;
    let gap = size * GAP;
    let pivot = (peek, size * PIVOT_Y);
    let card = |card: &Card, grow: f32| {
        let side = size * card.scale;
        let radius = side * card.round;
        let top = pivot.1 - (side - radius);
        rounded_rect_points(
            (peek - grow, top - grow, side + 2. * grow, side + 2. * grow),
            radius + grow,
            card.angle,
            pivot,
        )
    };
    fill_points(
        origin,
        &card(&BACK_CARD, 0.),
        color.opacity(BACK_CARD.opacity),
        window,
    );
    fill_points(origin, &card(&FRONT_CARD, gap), surface, window);
    fill_points(
        origin,
        &card(&FRONT_CARD, 0.),
        color.opacity(FRONT_CARD.opacity),
        window,
    );
    let hole = rounded_rect_points(
        (peek - gap, -gap, size + 2. * gap, size + 2. * gap),
        size * ROUNDING + gap,
        0.,
        pivot,
    );
    fill_points(origin, &hole, surface, window);
}

/// The initials tile: the identity colors of a chat's initials avatar
/// on a rounded square.
fn initials_tile(name: &str, size: f32) -> AnyElement {
    let initials = crate::ui::message_text::avatar_initials(name);
    let hue = (gpui_kit::hash(&SharedString::from(initials.clone())) % 12) as f32 * 30.;
    let (background, foreground) = if crate::ui::chat_theme::is_dark_palette() {
        (oklch(0.30, 0.05, hue), oklch(0.82, 0.11, hue))
    } else {
        (oklch(0.93, 0.05, hue), oklch(0.48, 0.14, hue))
    };
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(size * ROUNDING))
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .text_color(foreground)
        .text_size(px(size * 0.4))
        .line_height(px(size * 0.4))
        .font_semibold()
        .child(initials)
        .into_any_element()
}

/// A community's picture on `surface`: `size` square plus the peek strip
/// on its left. TDLib's `community.photo` is not read yet, so the
/// userpic is the name's initials.
fn community_avatar_on(name: &str, size: f32, surface: Hsla, cx: &App) -> AnyElement {
    let color = cx.theme().muted_foreground;
    let peek = size * PEEK;
    div()
        .relative()
        .flex_none()
        .w(px(size + peek))
        .h(px(size))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    paint_cards(bounds.origin, size, color, surface, window)
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left(px(peek))
                .child(initials_tile(name, size)),
        )
        .into_any_element()
}

pub(in crate::ui) fn community_avatar(name: &str, size: f32, cx: &App) -> AnyElement {
    community_avatar_on(name, size, cx.theme().background, cx)
}

/// The same, with an empty strip on the right as wide as the peek, so a
/// centered picture centers on the userpic.
pub(in crate::ui) fn community_avatar_centered(name: &str, size: f32, cx: &App) -> AnyElement {
    div()
        .pr(px(size * PEEK))
        .child(community_avatar(name, size, cx))
        .into_any_element()
}

/// `ComposeFolderListEntryText`: the joined chats' names, unread ones in
/// semibold and the full text color, then "and N more chats" for the
/// chats outside the list.
fn names_line(summary: &CommunityRowSummary, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let strong = cx.theme().foreground;
    let list = summary.joined_names();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut at = 0;
    for name in &summary.names {
        let end = at + name.title.len();
        if name.unread {
            highlights.push((
                at..end,
                HighlightStyle {
                    color: Some(strong),
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            ));
        }
        at = end + 2;
    }
    let text = if summary.not_joined > 0 {
        trn_with(keys::ARCHIVED_LAST, summary.not_joined, &[("chats", &list)])
    } else {
        list.clone()
    };
    let highlights = match text.find(list.as_str()) {
        Some(base) => highlights
            .into_iter()
            .map(|(range, style)| (range.start + base..range.end + base, style))
            .collect(),
        None => Vec::new(),
    };
    div()
        .min_w_0()
        .flex_1()
        .truncate()
        .text_xs()
        .text_color(muted)
        .child(crate::ui::bidi_line::one_line(text, highlights, Vec::new()))
        .into_any_element()
}

/// The unread-chats counter, grey when every unread chat is muted.
fn unread_badge(text: String, muted: bool, cx: &App) -> AnyElement {
    div()
        .id("community-unread")
        .flex_none()
        .h(px(20.))
        .min_w(px(20.))
        .px(px(6.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .map(|badge| {
            if muted {
                badge.bg(cx.theme().muted_foreground.opacity(0.55))
            } else {
                badge.bg(crate::ui::chat_theme::accent())
            }
        })
        .text_color(crate::ui::chat_theme::text_on_fill())
        .text_xs()
        .font_semibold()
        .aria_label(tr_with(
            keys::Q_CHATLIST_UNREAD_CHATS,
            &[("text", &text.to_string())],
        ))
        .child(text)
        .into_any_element()
}

fn mention_badge() -> AnyElement {
    div()
        .flex_none()
        .size(px(20.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::ui::chat_theme::accent())
        .text_color(crate::ui::chat_theme::text_on_fill())
        .text_xs()
        .font_semibold()
        .child("@")
        .into_any_element()
}

impl QuillApp {
    fn community_row_summary(&self, community_id: i64) -> Option<CommunityRowSummary> {
        self.listed_community_rows()
            .into_iter()
            .find(|row| row.community_id == community_id)
    }

    /// A collapsed community's row: opens the community's chat-list mode.
    pub(in crate::ui) fn community_row_element(
        &mut self,
        community_id: i64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(summary) = self.community_row_summary(community_id) else {
            return div().into_any_element();
        };
        let height = chat_row_height(&[], self.appearance.preview_lines);
        let badge = summary.badge();
        let mentioned = summary.mentioned;
        div()
            .id(("community-row", community_id as u64))
            .w_full()
            .px_2()
            .h(height)
            .flex()
            .flex_col()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .pressable(cx.theme())
            .role(Role::Button)
            .aria_label(format!("{} — {}", summary.name, summary.joined_names()))
            .bg(cx.theme().sidebar)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.enter_community_chat_list_mode(community_id, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.admin.community.flow.row_menu = Some((community_id, event.position));
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .ml(px(-CHAT_ROW_AVATAR * PEEK))
                            .child(community_avatar_on(
                                &summary.name,
                                CHAT_ROW_AVATAR,
                                cx.theme().sidebar,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .font_semibold()
                                    .min_w_0()
                                    .truncate()
                                    .child(summary.name.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(names_line(&summary, cx))
                                    .when(mentioned, |line| line.child(mention_badge()))
                                    .when_some(badge, |line, text| {
                                        line.child(unread_badge(text, summary.badge_muted, cx))
                                    }),
                            ),
                    ),
            )
            .map(|row| self.chat_list_nav_row(row, ListRow::Community(community_id), false, cx))
            .into_any_element()
    }

    /// Mute forever or unmute the community, which its chats follow
    /// while they keep their defaults.
    fn toggle_community_muted(&mut self, community_id: i64, cx: &mut Context<Self>) {
        let current = self
            .session()
            .and_then(|s| s.community_notification_settings(community_id))
            .unwrap_or_default();
        let settings = ChatNotificationSettings {
            use_default_mute_for: false,
            mute_for: if current.is_muted() { 0 } else { MUTE_FOREVER },
            ..current
        };
        if let Some(live) = self.live.as_mut()
            && let Err(err) = live
                .driver
                .set_community_notification_settings(community_id, settings)
        {
            self.connection
                .set_note(StatusNote::error(user_error(&err)));
        }
        cx.notify();
    }

    /// "Mark as read" for every chat the row stands for.
    fn mark_community_read(&mut self, community_id: i64, cx: &mut Context<Self>) {
        let Some(summary) = self.community_row_summary(community_id) else {
            return;
        };
        if let Some(live) = self.live.as_mut() {
            let failed = summary
                .member_chat_ids
                .iter()
                .filter_map(|chat_id| {
                    live.driver
                        .mark_chat_as_read(quill::ids::ChatId(*chat_id))
                        .err()
                })
                .last();
            if let Some(err) = failed {
                self.connection
                    .set_note(StatusNote::error(user_error(&err)));
            }
        }
        cx.notify();
    }

    /// The row's right-click menu, in tdesktop's order for a community
    /// entry: View community info, Ungroup, Mute / Unmute, Mark as read.
    pub(in crate::ui) fn community_row_menu_overlay(
        &self,
        community_id: i64,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = self
            .session()
            .and_then(|s| s.community_notification_settings(community_id))
            .is_some_and(|settings| settings.is_muted());
        let any_unread = self
            .community_row_summary(community_id)
            .is_some_and(|summary| summary.unread_chats > 0);
        let item = |id: &'static str,
                    icon: IconName,
                    label: &'static str,
                    on_click: Box<dyn Fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)>,
                    cx: &mut Context<QuillApp>| {
            context_menu_item(
                id,
                icon,
                label,
                text_menu().into(),
                true,
                &self.frame.menu.items,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.admin.community.flow.row_menu = None;
                on_click(this, window, cx);
                cx.notify();
            }))
            .into_any_element()
        };
        let mut rows: Vec<AnyElement> = vec![
            item(
                "community-menu-info",
                IconName::Info,
                tr(keys::CONTEXT_VIEW_COMMUNITY),
                Box::new(move |this, window, cx| {
                    this.open_info_panel_target(
                        quill::state::InfoPanelTarget::Community(community_id),
                        window,
                        cx,
                    );
                }),
                cx,
            ),
            item(
                "community-menu-ungroup",
                IconName::ChevronsUpDown,
                tr(keys::COMMUNITY_UNGROUP),
                Box::new(move |this, _, cx| {
                    this.show_community_box(CommunityBox::Ungroup { community_id }, cx);
                }),
                cx,
            ),
            item(
                "community-menu-mute",
                if muted {
                    IconName::Bell
                } else {
                    IconName::BellOff
                },
                if muted {
                    tr(keys::MUTE_MENU_UNMUTE)
                } else {
                    tr(keys::MUTE_MENU_MUTE)
                },
                Box::new(move |this, _, cx| this.toggle_community_muted(community_id, cx)),
                cx,
            ),
        ];
        if any_unread {
            rows.push(item(
                "community-menu-read",
                IconName::CircleCheck,
                tr(keys::CONTEXT_MARK_READ),
                Box::new(move |this, _, cx| this.mark_community_read(community_id, cx)),
                cx,
            ));
        }
        let panel = div()
            .id("community-menu-panel")
            .role(Role::Group)
            .occlude()
            .flex()
            .flex_col()
            .min_w(px(200.))
            .px_1()
            .py_1()
            .menu_surface(cx)
            .children(rows);
        DismissLayer::new(
            "community-menu-overlay",
            cx.listener(|this, _, _, cx| {
                this.admin.community.flow.row_menu = None;
                cx.notify();
            }),
        )
        .layer(|layer| layer.track_focus(&self.frame.menu.focus))
        .anchored(
            position,
            div()
                .id("community-menu-region")
                .role(Role::Group)
                .a11y_tag(quill::agent::ui_driver::redact::REGION_MENU)
                .child(panel),
        )
        .focus_trap("community-menu-focus", &self.frame.menu.focus)
    }
}

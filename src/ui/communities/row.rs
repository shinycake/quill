//! A community's picture and its collapsed chat-list row ("Show as One
//! Chat"), after tdesktop `PaintCommunityUserpicEffect` and the community
//! entry of `Dialogs::Row`: a rounded-square userpic with two cards
//! peeking out to its left, the joined chats' names as the preview, and
//! the unread-chats badge.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::chat_list_keys::ListRow;
use crate::ui::chat_row::{CHAT_ROW_AVATAR, chat_row_height};
use crate::ui::pressable::PressableDiv;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use quill::lang::trn_with;
use quill::local_path::sandboxed_display_path;

/// Strip to the left of the userpic the cards peek into (`kPeek`).
const PEEK: f32 = 0.3;
/// Rotation point, as a share of the size down the left edge (`kPivotY`).
const PIVOT_Y: f32 = 0.75;
/// The gap carved around each shape (`kGap`).
const GAP: f32 = 0.02;
/// The userpic's corner radius (`ForumUserpicRadiusMultiplier`).
pub(super) const ROUNDING: f32 = 0.3;

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

/// The community photo on the same rounded square, decoded at the drawn
/// size as chat avatars are.
pub(super) fn photo_tile(path: &std::path::Path, size: f32) -> AnyElement {
    img(crate::ui::image_budget::sized_image(
        crate::ui::image_budget::SizedSource::Path(std::sync::Arc::from(path)),
        px(size),
    ))
    .size(px(size))
    .flex_none()
    .rounded(px(size * ROUNDING))
    .object_fit(ObjectFit::Cover)
    .into_any_element()
}

/// A community's picture on `surface`: `size` square plus the peek strip
/// on its left. The userpic is `community.photo` once downloaded, the
/// name's initials until then (tdesktop's empty userpic).
fn community_avatar_on(
    name: &str,
    photo: Option<&std::path::Path>,
    size: f32,
    surface: Hsla,
    cx: &App,
) -> AnyElement {
    let tile = match photo {
        Some(path) => photo_tile(path, size),
        None => initials_tile(name, size),
    };
    with_cards(tile, size, surface, cx)
}

/// `tile`, a `size` rounded square, with the two cards peeking out on
/// its left (`PaintCommunityUserpicEffect`).
pub(super) fn with_cards(tile: AnyElement, size: f32, surface: Hsla, cx: &App) -> AnyElement {
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
        .child(div().absolute().top_0().left(px(peek)).child(tile))
        .into_any_element()
}

impl QuillApp {
    /// The downloaded `community.photo`, inside the media roots.
    fn community_photo(&self, community_id: i64) -> Option<std::path::PathBuf> {
        let roots = self.media_display_roots();
        self.session()
            .and_then(|s| s.community_photo_path(community_id))
            .and_then(|path| sandboxed_display_path(path, &roots))
    }

    fn community_userpic_on(
        &self,
        community_id: i64,
        size: f32,
        surface: Hsla,
        cx: &App,
    ) -> AnyElement {
        let photo = self.community_photo(community_id);
        community_avatar_on(
            &self.community_name(community_id),
            photo.as_deref(),
            size,
            surface,
            cx,
        )
    }

    /// The community's picture on the window background.
    pub(in crate::ui) fn community_userpic(
        &self,
        community_id: i64,
        size: f32,
        cx: &App,
    ) -> AnyElement {
        self.community_userpic_on(community_id, size, cx.theme().background, cx)
    }

    /// The same, with an empty strip on the right as wide as the peek, so
    /// a centered picture centers on the userpic.
    pub(in crate::ui) fn community_userpic_centered(
        &self,
        community_id: i64,
        size: f32,
        cx: &App,
    ) -> AnyElement {
        div()
            .pr(px(size * PEEK))
            .child(self.community_userpic(community_id, size, cx))
            .into_any_element()
    }
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

/// The row's date: its newest chat's last message (`_chatsListDate`),
/// in the chat rows' format.
fn row_date(date: i32) -> Option<String> {
    (date > 0).then(|| {
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let date = quill::local_time::civil_local(i64::from(date));
        quill::local_time::chat_list_stamp(&date, &now)
    })
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
    pub(super) fn community_row_summary(&self, community_id: i64) -> Option<CommunityRowSummary> {
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
                    this.admin.community.flow.row_menu =
                        Some(RowMenu::at(community_id, event.position));
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
                            .child(self.community_userpic_on(
                                community_id,
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
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .font_semibold()
                                            .min_w_0()
                                            .truncate()
                                            .child(summary.name.clone()),
                                    )
                                    .when_some(row_date(summary.date), |line, stamp| {
                                        line.child(
                                            div()
                                                .flex_none()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(stamp),
                                        )
                                    }),
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
}

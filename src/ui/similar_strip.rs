//! The similar-channels strip under "You joined this channel" (Telegram
//! Desktop `HistoryView::SimilarChannels`): a bubble pointing at the notice
//! with a row of channel tiles (avatar, subscriber badge, name), a locked
//! "More Channels" tile without Premium, "View all" when the row overflows.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_row::{chat_avatar, compact_count};
use super::chat_theme::{accent, text_muted};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::premium_limits::PremiumLimitKind;
use quill::state::{InfoPanelTarget, Session, SimilarTile};
use quill::telegram::envelope::ChatKind;
use std::path::PathBuf;

/// tdesktop `chatSimilarChannelPhoto`.
const PHOTO: f32 = 50.;
/// tdesktop `chatSimilarChannelPadding` (8px each side of the photo).
const TILE_WIDTH: f32 = PHOTO + 16.;
/// tdesktop `chatSimilarSkip`.
const TILE_GAP: f32 = 12.;
/// tdesktop `chatSimilarWidthMax`.
const STRIP_MAX_WIDTH: f32 = 424.;
/// tdesktop `chatSimilarArrowSize`.
const ARROW: f32 = 6.;

/// The strip under the joined notice of `chat_id`, or nothing while it is
/// collapsed, still loading or empty. A missing list is requested (the
/// driver dedupes).
pub(super) fn similar_channels_strip(
    session: Option<&Session>,
    chat_id: ChatId,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let session = session?;
    let strip = session.similar_strip(chat_id)?;
    if !strip.expanded {
        return None;
    }
    let Some(tiles) = session.similar_strip_tiles(chat_id) else {
        request_similar_chats(chat_id, cx);
        return None;
    };
    if tiles.is_empty() {
        return None;
    }
    {
        let roots = media_roots;
        let premium = session.is_premium();
        let row_id = chat_id.0.unsigned_abs();
        let full_width = tiles.len() as f32 * (TILE_WIDTH + TILE_GAP) - TILE_GAP;
        let overflows = full_width > STRIP_MAX_WIDTH;
        let mut row = div()
            .id(("similar-strip-row", row_id))
            .flex()
            .flex_row()
            .gap(px(TILE_GAP))
            .px_2()
            .pb_1()
            .when(overflows, |this| this.overflow_x_scroll());
        for (ix, tile) in tiles.iter().enumerate() {
            row = row.child(match tile {
                SimilarTile::Channel { chat_id: similar } => {
                    let similar = ChatId(*similar);
                    let chat = session.chats.get(&similar.0);
                    let title = chat.map(|chat| chat.title.clone()).unwrap_or_default();
                    let photo = session
                        .chat_photo_path(similar)
                        .and_then(|path| sandboxed_display_path(path, roots));
                    let members = chat
                        .and_then(|chat| session.group_member_counts(chat))
                        .map(|(members, _)| members)
                        .filter(|members| *members > 1)
                        .map(compact_count);
                    tile_element(
                        ("similar-tile", row_id * 64 + ix as u64),
                        chat_avatar(&title, photo.as_deref(), PHOTO).into_any_element(),
                        members.map(|count| badge(Some(IconName::Users), count, cx)),
                        title.clone(),
                        false,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_similar_channel(chat_id, similar, window, cx);
                    }))
                    .into_any_element()
                }
                SimilarTile::More {
                    count,
                    locked,
                    stack,
                } => {
                    let stack: Vec<(String, Option<std::path::PathBuf>)> = stack
                        .iter()
                        .map(|id| {
                            let title = session
                                .chats
                                .get(id)
                                .map(|chat| chat.title.clone())
                                .unwrap_or_default();
                            let photo = session
                                .chat_photo_path(ChatId(*id))
                                .and_then(|path| sandboxed_display_path(path, roots));
                            (title, photo)
                        })
                        .collect();
                    let locked = *locked;
                    tile_element(
                        ("similar-tile", row_id * 64 + ix as u64),
                        stacked_avatars(&stack, cx),
                        Some(badge(
                            locked.then_some(IconName::Lock),
                            format!("+{}", compact_count(*count)),
                            cx,
                        )),
                        tr(keys::SIMILAR_CHANNELS_MORE).to_string(),
                        true,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_more_similar_channels(chat_id, locked || !premium, window, cx);
                    }))
                    .into_any_element()
                }
            });
        }
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .pt_2()
            .pb_1()
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::SIMILAR_CHANNELS_TITLE)),
            )
            .when(overflows, |this| {
                this.child(
                    Button::new(("similar-view-all", row_id))
                        .ghost()
                        .xsmall()
                        .label(tr(keys::SIMILAR_CHANNELS_VIEW_ALL))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_more_similar_channels(chat_id, false, window, cx);
                        })),
                )
            });
        let fill = cx.theme().secondary.opacity(0.85);
        Some(
            div()
                .id(("similar-strip", row_id))
                .relative()
                .mt(px(ARROW))
                .max_w(px(STRIP_MAX_WIDTH))
                .rounded_xl()
                .bg(fill)
                .text_color(cx.theme().secondary_foreground)
                .child(arrow(fill))
                .child(header)
                .child(row)
                .into_any_element(),
        )
    }
}

/// Ask for the similar channels once (the driver skips a list that is
/// loading or loaded).
fn request_similar_chats(chat_id: ChatId, cx: &mut Context<QuillApp>) {
    let weak = cx.weak_entity();
    cx.defer(move |cx| {
        let _ = weak.update(cx, |this, cx| {
            if let Some(live) = this.live.as_mut()
                && matches!(live.driver.fetch_similar_chats(chat_id), Ok(Some(_)))
            {
                cx.notify();
            }
        });
    });
}

impl QuillApp {
    /// A tile click: tell TDLib which strip it came from
    /// (`openChatSimilarChat`) and open the channel.
    fn open_similar_channel(
        &mut self,
        chat_id: ChatId,
        similar: ChatId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_similar_chat(chat_id, similar);
        }
        self.select_listed_chat(similar, window, cx);
    }

    /// "More Channels" / "View all": the channel's info panel lists every
    /// similar channel; without Premium the locked tile explains the cap
    /// with tdesktop's Premium toast (`lng_similar_channels_premium_all`).
    fn open_more_similar_channels(
        &mut self,
        chat_id: ChatId,
        locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if locked {
            self.show_premium_limit(PremiumLimitKind::SimilarChats, 0, cx);
            return;
        }
        let supergroup_id = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .and_then(|chat| match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
                _ => None,
            });
        if let Some(supergroup_id) = supergroup_id {
            self.open_info_panel_target(InfoPanelTarget::Supergroup(supergroup_id), window, cx);
        }
    }

    /// Clicking the joined notice collapses or expands the strip (tdesktop
    /// toggles `SimilarExpanded` from the service text).
    pub(super) fn toggle_similar_strip(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.session.toggle_similar_strip(chat_id);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.toggle_similar_strip(chat_id);
        }
        cx.notify();
    }
}

/// The small triangle on the bubble's top edge pointing at the notice.
fn arrow(fill: impl Into<Hsla>) -> AnyElement {
    let fill: Hsla = fill.into();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let x = bounds.center().x;
            let y = bounds.top();
            let mut path = PathBuilder::fill();
            path.move_to(point(x, y - px(ARROW)));
            path.line_to(point(x + px(ARROW), y));
            path.line_to(point(x - px(ARROW), y));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, fill);
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h(px(1.))
    .into_any_element()
}

/// One tile: the avatar with its badge, the name below (two lines at
/// most, centered). `more` names the tile in the muted color.
fn tile_element(
    id: (&'static str, u64),
    avatar: AnyElement,
    badge: Option<AnyElement>,
    name: String,
    more: bool,
    cx: &mut Context<QuillApp>,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .w(px(TILE_WIDTH))
        .flex()
        .flex_col()
        .items_center()
        .pt(px(5.))
        .pb_1()
        .rounded_lg()
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(name.clone())
        .tab_index(0)
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .child(
            div()
                .relative()
                .size(px(PHOTO))
                .child(avatar)
                .when_some(badge, |this, badge| {
                    this.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            // tdesktop `chatSimilarBadgeTop`: over the
                            // photo's bottom edge.
                            .top(px(38.))
                            .flex()
                            .justify_center()
                            .child(badge),
                    )
                }),
        )
        .child(
            div()
                .mt(px(9.))
                .w_full()
                .px_0p5()
                .text_xs()
                .text_center()
                .line_clamp(2)
                .when(more, |this| this.text_color(text_muted()))
                .child(name),
        )
}

/// The subscriber count (or "+N") pill: tdesktop draws it in the photo's
/// average color; the accent stands in, with a white border like the photo
/// ring.
fn badge(icon: Option<IconName>, text: String, cx: &mut Context<QuillApp>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_0p5()
        .px_1()
        .h(px(14.))
        .rounded_full()
        .bg(accent())
        .border_1()
        .border_color(cx.theme().background)
        .text_color(gpui_kit::white())
        .text_size(px(10.))
        .font_bold()
        .when_some(icon, |this, icon| {
            this.child(Icon::new(icon).size(px(9.)).text_color(gpui_kit::white()))
        })
        .child(text)
        .into_any_element()
}

/// Up to three overlapping avatars for the "More Channels" tile.
fn stacked_avatars(
    stack: &[(String, Option<std::path::PathBuf>)],
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let ring = cx.theme().background;
    let mut layer = div().relative().size(px(PHOTO));
    // Back to front, the front one at the left (tdesktop paints the extra
    // thumbnails shifted right behind the first).
    for (ix, (name, photo)) in stack.iter().enumerate().rev() {
        let side = PHOTO - 8. * ix as f32;
        layer = layer.child(
            div()
                .absolute()
                .top(px((PHOTO - side) / 2.))
                .left(px(8. * ix as f32))
                .size(px(side))
                .rounded_full()
                .border_2()
                .border_color(ring)
                .child(chat_avatar(name, photo.as_deref(), side - 4.)),
        );
    }
    layer.into_any_element()
}

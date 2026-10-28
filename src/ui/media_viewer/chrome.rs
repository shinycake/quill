//! The viewer's chrome over the picture: top bar, arrows, caption and
//! controls. In video full screen the top bar and caption go, and the
//! arrows and controls sit over the picture on a scrim and fade with the
//! idle timer (tdesktop `_fullScreenVideo`).
#![deny(clippy::indexing_slicing)]

use super::geometry::{CAPTION_GAP, GROUP_STRIP_ROOM, lane_width};
use super::layer::ViewerGeometry;
use super::*;
use crate::ui::video_player::controls::hide_cursor;
use quill::lang::{keys, tr};
use quill::media_viewer::ViewerTtl;

/// The full screen scrim's height: the controller and a band above it.
const FULLSCREEN_SCRIM: f32 = 160.0;

/// What the media box already worked out that the chrome needs too.
pub(super) struct ChromeFacts {
    pub(super) row_id: u64,
    pub(super) kind_label: &'static str,
    pub(super) ttl: Option<ViewerTtl>,
    pub(super) protected: bool,
    pub(super) can_delete: bool,
    pub(super) can_set_main: bool,
    pub(super) can_report: bool,
    /// The item is a message the user may answer here.
    pub(super) can_reply: bool,
    pub(super) profile_view: bool,
    /// The profile photo viewer's own header ("Photo 2 of 5", "Group
    /// Photo", ...).
    pub(super) header: Option<String>,
}

impl QuillApp {
    /// The chrome's elements, positioned in a window-sized panel.
    pub(super) fn viewer_chrome(
        &mut self,
        item: &MediaViewerItem,
        facts: ChromeFacts,
        geometry: ViewerGeometry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let ChromeFacts {
            row_id,
            kind_label,
            ttl,
            protected,
            can_delete,
            can_set_main,
            can_report,
            can_reply,
            profile_view,
            header,
        } = facts;
        self.tick_viewer_ttl(ttl, cx);
        let ttl_badge = ttl.and_then(|ttl| self.viewer_ttl_badge(ttl));
        let has_local_clip = self.viewer_clip_path(item).is_some();
        let hidden = self.viewer.controls_hidden;
        let fade_gen = self.viewer.controls_gen;
        let (position, total) = self.viewer.state.position().unwrap_or((0, 0));
        let sender_line = (!profile_view)
            .then(|| self.viewer_sender_line(item))
            .flatten();
        let header_label = if let Some(header) = header {
            header
        } else if total > 1 {
            format!("{kind_label} {position} of {total}")
        } else {
            kind_label.to_string()
        };
        // MED1: album pin action for the header — only when the item is in
        // an album and the user may pin in this chat (rights-gated, TGX
        // `MessagePinAlbum` semantics: unpins when any member is pinned).
        let album_pin_label: Option<String> =
            self.viewer_album_id(item.message_id).and_then(|album_id| {
                let can_pin = self
                    .session()
                    .and_then(|s| s.chats.get(&item.chat_id.0))
                    .is_some_and(|chat| chat.can_pin_messages());
                if !can_pin {
                    return None;
                }
                let history: Vec<HistoryMessage> = self
                    .session()
                    .and_then(|s| s.histories.get(&item.chat_id.0))
                    .map(|h| h.ordered().into_iter().cloned().collect())
                    .unwrap_or_default();
                let ids = quill::album::album_message_ids(&history, album_id);
                if ids.is_empty() {
                    return None;
                }
                let any_pinned = ids
                    .iter()
                    .any(|id| history.iter().any(|m| m.id == *id && m.is_pinned));
                Some(if any_pinned {
                    tr(keys::Q_AV_UNPIN_ALBUM).to_string()
                } else {
                    tr(keys::Q_AV_PIN_ALBUM).to_string()
                })
            });
        // Parity slice 5: the video player panel (or a download CTA).
        let is_video = item.kind == MediaViewerKind::Video;
        let video_controls: Option<AnyElement> = is_video
            .then(|| self.viewer_video_controls(item, row_id, cx))
            .flatten();
        let key_focus = self.viewer_key_focus_probe(cx);
        // Videos get the player panel; zoom controls are for photos.
        let transport: Option<AnyElement> = if is_video {
            video_controls
        } else {
            Some(Self::viewer_zoom_controls(self.viewer.zoom, row_id, cx).into_any_element())
        };
        // Custom emoji in the caption render like they do in message text:
        // the resolved sticker images, the span text until they are.
        let caption_emoji = self.viewer_caption_emoji(item);
        let caption: Option<AnyElement> = geometry.caption.is_some().then(|| {
            rich_text_line(
                &item.caption,
                &item.caption_entities,
                (item.chat_id.0, row_id),
                true,
                &self.message_ui.spoiler_revealed,
                // Settings → Appearance: captions follow the message font size.
                self.msg_font(),
                &caption_emoji,
                cx,
            )
        });
        let viewport = window.viewport_size();
        self.viewer.extra.strip_lane = Some(lane_width(f32::from(viewport.width)));
        let group_thumbs = self.viewer_group_thumbs(f32::from(viewport.width), cx);
        // An expanded caption stops short of the top bar with the strip
        // under it too.
        let viewport_h = f32::from(viewport.height)
            - if group_thumbs.is_some() {
                GROUP_STRIP_ROOM
            } else {
                0.0
            };
        let caption = caption.zip(geometry.caption).map(|(caption, layout)| {
            self.viewer_caption_plate(item, caption, layout, viewport_h, cx)
        });
        let sender_profile = self.viewer_sender_profile(item);
        let saved_toast: Option<AnyElement> = self.viewer_saved_toast(cx);
        let icon_action =
            |id: (&'static str, u64), icon: gpui_kit::assets::IconName, label: &'static str| {
                Button::new(id)
                    .icon(icon)
                    .ghost()
                    .text_color(gpui_kit::white())
                    .tooltip(label)
                    .accessibility_label(label)
            };
        // Every control surface occludes: GPUI delivers a click to all
        // hitboxes under the cursor down to the first occluding one, so
        // without it the backdrop below also gets the click and closes.
        let over_controls = Self::over_controls_listener;
        let hdr_badge = self.viewer_hdr_badge();
        let traffic_lights = crate::ui::window_frame::traffic_light_inset(window);
        let top_bar = div()
            .id("media-viewer-top-bar")
            .on_mouse_move(over_controls(cx))
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(VIEWER_TOP_BAR))
            .px_4()
            .when(traffic_lights > px(16.), |this| this.pl(traffic_lights))
            .flex()
            .items_center()
            .justify_between()
            .child(match sender_line {
                Some((name, when)) => div()
                    .id("media-viewer-sender")
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .text_color(gpui_kit::white())
                    .when_some(sender_profile, |this, profile| {
                        this.role(Role::Button)
                            .aria_label(tr(keys::Q_AV_OPEN_PROFILE))
                            .cursor_pointer()
                            .hover(|style| style.opacity(0.8))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_viewer_sender_profile(profile, window, cx);
                            }))
                    })
                    .child(
                        div()
                            .font_semibold()
                            .truncate()
                            .child(crate::ui::bidi_line::one_line_plain(name)),
                    )
                    .child(div().text_xs().opacity(0.7).truncate().child({
                        // `lng_mediaview_ttl_in` follows the date
                        // once the timer runs.
                        let when = match ttl.and_then(|ttl| ttl.date_suffix()) {
                            Some(left) if when.is_empty() => left,
                            Some(left) => format!("{when} \u{b7} {left}"),
                            None => when,
                        };
                        if when.is_empty() {
                            header_label
                        } else {
                            format!("{when} \u{b7} {header_label}")
                        }
                    }))
                    .into_any_element(),
                None => div()
                    .font_semibold()
                    .text_color(gpui_kit::white())
                    .child(header_label)
                    .into_any_element(),
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when_some(hdr_badge, |this, badge| {
                        this.child(crate::ui::hdr_image::badge_element(badge))
                    })
                    .when(item.kind.is_picture(), |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-rotate", row_id),
                                gpui_kit::assets::IconName::RotateCw,
                                tr(keys::Q_AV_ROTATE),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rotate_viewer_photo(cx);
                            })),
                        )
                    })
                    // Protected content can't be shared or saved
                    // (Telegram Desktop hides both).
                    .when(can_set_main, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-set-main", row_id),
                                gpui_kit::assets::IconName::Images,
                                tr(keys::MEDIAVIEW_SET_USERPIC),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_viewer_photo_as_main(cx);
                            })),
                        )
                    })
                    .when(can_report, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-report", row_id),
                                gpui_kit::assets::IconName::Flag,
                                tr(keys::MEDIAVIEW_REPORT_PROFILE_PHOTO),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.report_viewer_profile_photo(cx);
                            })),
                        )
                    })
                    .when(can_reply, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-reply", row_id),
                                gpui_kit::assets::IconName::Reply,
                                tr(keys::Q_MEDIA_REPLY),
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.reply_from_viewer(window, cx);
                                },
                            )),
                        )
                    })
                    .when(!protected && !profile_view, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-share", row_id),
                                gpui_kit::assets::IconName::Forward,
                                tr(keys::Q_AV_SHARE),
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.share_viewer_media(window, cx);
                                },
                            )),
                        )
                    })
                    .when(!protected, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-save", row_id),
                                gpui_kit::assets::IconName::Download,
                                tr(keys::SETTINGS_SAVE),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_viewer_media(cx);
                            })),
                        )
                    })
                    .when(!profile_view, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-show-in-chat", row_id),
                                gpui_kit::assets::IconName::MessageSquare,
                                tr(keys::Q_AV_SHOW_IN_CHAT_2),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_viewer_in_chat(cx);
                            })),
                        )
                    })
                    .when_some(album_pin_label, |this, label| {
                        this.child(
                            Button::new(("media-viewer-pin-album", row_id))
                                .icon(gpui_kit::assets::IconName::Pin)
                                .ghost()
                                .text_color(gpui_kit::white())
                                .tooltip(label.clone())
                                .accessibility_label(label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_viewer_album_pin(cx);
                                })),
                        )
                    })
                    .when(can_delete, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-delete", row_id),
                                gpui_kit::assets::IconName::Trash,
                                tr(keys::BOX_DELETE),
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.delete_viewer_media(window, cx);
                                },
                            )),
                        )
                    })
                    .child(
                        icon_action(
                            ("media-viewer-close", row_id),
                            gpui_kit::assets::IconName::X,
                            tr(keys::Q_AV_CLOSE_MEDIA_VIEWER),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_media_viewer(cx);
                        })),
                    ),
            );
        let nav_arrow = |id: &'static str,
                         icon: gpui_kit::assets::IconName,
                         label: &'static str,
                         step: i32,
                         cx: &mut Context<Self>| {
            Button::new(id)
                .icon(icon)
                .large()
                .rounded_full()
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(gpui_kit::black().opacity(0.4))
                        .foreground(gpui_kit::white())
                        .hover(gpui_kit::black().opacity(0.6)),
                )
                .tooltip(label)
                .accessibility_label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.step_media_viewer(step, cx);
                }))
        };
        // A swipe pulling an arrow in from an edge replaces that edge's
        // button, so the two chevrons never overlap.
        let swiping = self.viewer.swipe.indicator().map(|shown| shown.dir);
        let prev =
            (self.viewer.state.has_prev() && swiping != Some(SwipeDir::Previous)).then(|| {
                nav_arrow(
                    "media-viewer-prev",
                    gpui_kit::assets::IconName::ChevronLeft,
                    tr(keys::Q_AV_PREVIOUS),
                    -1,
                    cx,
                )
            });
        let next = (self.viewer.state.has_next() && swiping != Some(SwipeDir::Next)).then(|| {
            nav_arrow(
                "media-viewer-next",
                gpui_kit::assets::IconName::ChevronRight,
                tr(keys::Q_AV_NEXT),
                1,
                cx,
            )
        });
        let fullscreen = self.viewer.extra.video_fullscreen.is_on();
        // Only in full screen: the platform hides the cursor system wide,
        // and that window covers the screen.
        if fullscreen && hidden && window.is_window_active() {
            hide_cursor();
        }
        let fit = geometry.fit;
        let arrow_top = px(fit.origin.1 + fit.size.1 / 2.0 - 24.0);
        let mut parts: Vec<AnyElement> = Vec::new();
        // Faded out, the controls stop taking the pointer: clicks, wheel
        // and double-clicks reach the media under them.
        if self.viewer_controls_gone() {
            parts.extend(ttl_badge);
            parts.extend(saved_toast);
            parts.push(key_focus);
            return parts;
        }
        if !fullscreen {
            parts.push(
                fade_controls(top_bar, "viewer-top-fade", fade_gen, hidden).into_any_element(),
            );
        }
        if let Some(prev) = prev {
            parts.push(
                fade_controls(
                    div()
                        .id("media-viewer-prev-lane")
                        .on_mouse_move(over_controls(cx))
                        .occlude()
                        .absolute()
                        .left(px(16.))
                        .top(arrow_top)
                        .child(prev),
                    "viewer-prev-fade",
                    fade_gen,
                    hidden,
                )
                .into_any_element(),
            );
        }
        if let Some(next) = next {
            parts.push(
                fade_controls(
                    div()
                        .id("media-viewer-next-lane")
                        .on_mouse_move(over_controls(cx))
                        .occlude()
                        .absolute()
                        .right(px(16.))
                        .top(arrow_top)
                        .child(next),
                    "viewer-next-fade",
                    fade_gen,
                    hidden,
                )
                .into_any_element(),
            );
        }
        let error = self
            .playback
            .error
            .clone()
            .filter(|_| !is_video)
            .map(|err| {
                let can_open = item.kind.is_playable() && has_local_clip;
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_sm().text_color(danger_bright()).child(err))
                    .when(can_open, |row| {
                        row.child(
                            Button::new(("media-viewer-open-externally", row_id))
                                .label(tr(keys::Q_AV_OPEN_EXTERNALLY))
                                .ghost()
                                .text_color(gpui_kit::white())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_viewer_clip_externally(cx);
                                })),
                        )
                    })
                    .into_any_element()
            });
        // Only the rows themselves take the pointer, not the band around
        // them, so the picture under the bar stays clickable. The band
        // still keeps the wheel, so a scroll there does not change the item.
        let rows = [
            ("media-viewer-caption-row", caption),
            ("media-viewer-error-row", error),
            ("media-viewer-controls-row", transport),
        ]
        .into_iter()
        .filter_map(|(id, row)| {
            row.map(|row| {
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .on_mouse_move(over_controls(cx))
                    .occlude()
                    .child(row)
            })
        });
        let bottom_bar = div()
            .debug_selector(|| "media-viewer-bottom-band".into())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .absolute()
            .left_0()
            .right_0()
            .bottom_0()
            .px_6()
            .pb_4()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(CAPTION_GAP))
            .children(rows)
            .children(group_thumbs);
        if fullscreen {
            // Over the picture, a scrim keeps the controller readable.
            let scrim = div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(FULLSCREEN_SCRIM))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(gpui_kit::black().opacity(0.), 0.),
                    linear_color_stop(gpui_kit::black().opacity(0.6), 1.),
                ));
            parts.push(
                fade_controls(scrim, "viewer-scrim-fade", fade_gen, hidden).into_any_element(),
            );
        }
        parts.push(
            fade_controls(bottom_bar, "viewer-bottom-fade", fade_gen, hidden).into_any_element(),
        );
        parts.extend(ttl_badge);
        parts.extend(saved_toast);
        parts.push(key_focus);
        parts
    }
}

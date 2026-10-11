//! The fullscreen viewer overlay (render).

use super::*;

impl QuillApp {
    /// Phase 4.5: fullscreen media viewer overlay. The backdrop is a
    /// separate sibling painted behind the panel (not an ancestor), so a
    /// click on the panel never bubbles into the backdrop's close handler —
    /// "click outside" works regardless of click-bubbling semantics. Esc
    /// closes through `cancel_search`; the header close button is the third
    /// path. Videos show their thumbnail (no in-viewer playback — the
    /// history row's Play path is unchanged); secret/spoiler media never
    /// reach the viewer (filtered in `collect_media_items`).
    /// MED1: fullscreen photo/video viewer overlay. Takes `&mut self` +
    /// `window` so the viewer seek/volume sliders can sync to the clocks
    /// during render.
    pub(in crate::ui) fn media_viewer_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // A deleted item leaves the viewer (next item, or closed).
        self.prune_deleted_viewer_items(cx);
        self.sync_shared_media_viewer(cx);
        self.sync_viewer_window_activity(window.is_window_active(), cx);
        // MED1: keep the viewer seek/volume thumbs on the clocks (the
        // tick has no `&mut Window`, which `SliderState::set_value`
        // needs).
        self.sync_viewer_seek_slider(window, cx);
        self.sync_viewer_volume_slider(window, cx);
        // Native playback draws a new frame every display refresh; a GIF
        // loop only needs the shared frame clock.
        let loops = self.viewer_loops();
        if self
            .viewer
            .native
            .as_mut()
            .is_some_and(|video| video.is_playing())
        {
            if loops {
                self.request_animation_tick(30, cx);
            } else {
                // Through the frame clock, not `request_animation_frame`
                // (which redraws at the display refresh, 120 Hz on
                // ProMotion, even behind other apps). The sound keeps
                // playing in the background like tdesktop's, but the picture
                // only needs a trickle there.
                let fps = if self.frame.window_active.get() {
                    60
                } else {
                    10
                };
                self.request_media_tick(fps, cx);
            }
        } else if loops
            && !self.viewer.video_frames.is_empty()
            && self
                .viewer
                .clock
                .as_ref()
                .is_some_and(|clock| clock.is_playing())
        {
            let fps = self.viewer.video_fps.round().clamp(1.0, 30.0) as u32;
            self.request_animation_tick(fps, cx);
        }
        let item = self
            .viewer
            .state
            .current()
            .cloned()
            .unwrap_or_else(|| MediaViewerItem {
                chat_id: ChatId(0),
                message_id: MessageId(0),
                kind: MediaViewerKind::Photo,
                display_file_ids: Vec::new(),
                download_file_id: FileId(0),
                play_file_id: None,
                duration_secs: None,
                mime_type: None,
                start_timestamp: None,
                caption: String::new(),
                caption_entities: Vec::new(),
                duration_label: None,
                natural_size: None,
            });
        let (position, total) = self.viewer.state.position().unwrap_or((0, 0));
        let files: HashMap<i32, ParsedFile> = self
            .session()
            .map(|s| s.media.files.clone())
            .unwrap_or_default();
        let downloading: HashSet<i32> = self
            .session()
            .map(|s| s.media.downloading.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let thumb_path = viewer_display_path(&item, &files, &roots);
        // Parity slice 5: when the clip's frames are extracted and decoded,
        // the viewer shows the pre-loaded frame matching the playback clock
        // — real in-viewer video (`ImageSource::Render` resolves
        // synchronously, so the 125 ms tick animates without a per-frame
        // async load). Otherwise it falls back to the thumbnail (or the
        // loading status).
        let frame: Option<Arc<RenderImage>> =
            if item.kind.is_playable() && !self.viewer.video_frames.is_empty() {
                self.viewer_render_frame()
            } else {
                None
            };
        let protected = self
            .viewer
            .state
            .current()
            .zip(self.session())
            .is_some_and(|(item, session)| session.chat_has_protected_content(item.chat_id));
        let can_delete = self.viewer_delete_confirm().is_some();
        let has_local_clip = self.viewer_clip_path(&item).is_some();
        let hidden = self.viewer.controls_hidden;
        let fade_gen = self.viewer.controls_gen;
        let row_id = item.message_id.0 as u64;
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, &files, &downloading));
        let profile_view = self.viewer.state.source() == ViewerSource::Profile;
        let kind_label = if profile_view {
            if self.viewer.extra.profile_personal == Some(item.message_id.0) {
                "Photo set by you"
            } else {
                "Profile photo"
            }
        } else {
            item.kind.label()
        };
        let sender_line = (!profile_view)
            .then(|| self.viewer_sender_line(&item))
            .flatten();
        let header_label = if total > 1 {
            format!("{kind_label} {position} of {total}")
        } else {
            kind_label.to_string()
        };
        // B10: "Set as main photo" for one of your own earlier photos (the
        // first one already is the main photo).
        let can_set_main = profile_view
            && position > 1
            && self
                .viewer
                .extra
                .profile_user
                .zip(self.session().and_then(|s| s.my_user_id))
                .is_some_and(|(shown, me)| shown == me);
        // Reporting someone else's profile photo (`reportChatPhoto`).
        let can_report = profile_view
            && self
                .viewer
                .extra
                .profile_user
                .zip(self.session().and_then(|s| s.my_user_id))
                .is_some_and(|(shown, me)| shown != me);
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
                    "Unpin album".to_string()
                } else {
                    "Pin album".to_string()
                })
            });
        // The visual fills the window between the top bar and the bottom
        // controls, leaving lanes for the prev/next arrows; the media fits
        // inside it (object-fit contain). Scroll zooms (1×–8×, frame-center
        // kept) and drag pans when zoomed.
        let viewport = window.viewport_size();
        let bottom_space = if item.caption.is_empty() { 72.0 } else { 104.0 };
        let fit = (
            (f32::from(viewport.width) - 2.0 * VIEWER_SIDE_LANE).max(240.0),
            (f32::from(viewport.height) - VIEWER_TOP_BAR - bottom_space).max(200.0),
        );
        let frame_h = fit.1;
        // The media's own box inside the frame: sized from its natural
        // dimensions (axes swapped for a quarter turn) rather than trusting
        // object-fit, so it can never spill out of the frame.
        let natural = item.natural_size.map(|(w, h)| {
            if self.viewer.orientation.swaps_axes() {
                (h as f32, w as f32)
            } else {
                (w as f32, h as f32)
            }
        });
        let (media_w, media_h) = natural
            .map(|natural| quill::media_viewer::fit_within(natural, fit))
            .unwrap_or(fit);
        // Zoom and pan work in the media's own box.
        if (media_w, media_h) != self.viewer.frame {
            // A resized window (or another item) refits the media.
            self.viewer.frame = (media_w, media_h);
            self.viewer.zoom.reset();
        }
        let zoom = self.viewer.zoom;
        let (zoom_w, zoom_h) = (media_w * zoom.zoom, media_h * zoom.zoom);
        let (pan_x, pan_y) = zoom.pan;
        // The player's current frame (an AVPlayer GPU buffer on macOS, a
        // decoded FFmpeg image on Linux and Windows).
        let native_frame = item
            .kind
            .is_playable()
            .then(|| self.viewer.native.as_mut().and_then(|video| video.frame()))
            .flatten();
        let content: AnyElement = if let Some(frame) = native_frame {
            frame.element(
                px(zoom_w),
                px(zoom_h),
                ObjectFit::Contain,
                Corners::default(),
            )
        } else {
            // Pre-decoded video frame and thumbnail both render through
            // `img`; the frame is an `ImageSource::Render` (synchronous),
            // the thumbnail a path (async-loaded once, then cached). MED1:
            // a rotated photo renders from the eagerly-decoded
            // `viewer_rotated` cache (90°/180°/270° clockwise).
            let rotated: Option<ImageSource> = (item.kind == MediaViewerKind::Photo)
                .then_some(self.viewer.rotated.as_ref())
                .flatten()
                .filter(|(path, turns, _)| {
                    *turns == self.viewer.orientation.code()
                        && Some(path.as_path()) == thumb_path.as_deref()
                })
                .map(|(_, _, image)| ImageSource::from(image.clone()));
            let source: Option<ImageSource> = frame
                .map(ImageSource::from)
                .or(rotated)
                .or_else(|| thumb_path.clone().map(ImageSource::from));
            if let Some(source) = source {
                img(source)
                    .id(("media-viewer-img", row_id))
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .aspect_ratio(px(zoom_w) / px(zoom_h))
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(move || {
                        div()
                            .w(px(zoom_w))
                            .h(px(zoom_h))
                            .bg(bg_deep())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(gpui_kit::white())
                            .child(format!("{kind_label} — could not render"))
                            .into_any_element()
                    })
                    .into_any_element()
            } else {
                // MED3: show the real download percent when known
                // (`updateFile` → `downloaded_size`).
                let progress_pct: Option<i32> = item
                    .display_file_ids
                    .iter()
                    .chain(std::iter::once(&item.download_file_id))
                    .filter_map(|id| files.get(&id.0))
                    .filter_map(|f| f.download_progress())
                    .map(|p| (p * 100.0).round() as i32)
                    .next();
                let downloading_label = match progress_pct {
                    Some(pct) => format!("downloading… {pct}%"),
                    None => "downloading…".to_string(),
                };
                let status = match (&item.duration_label, downloading_now) {
                    (Some(duration), true) => {
                        format!("{kind_label} · {duration} — {downloading_label}")
                    }
                    (Some(duration), false) => {
                        format!("{kind_label} · {duration} — not downloaded")
                    }
                    (None, true) => format!("{kind_label} — {downloading_label}"),
                    (None, false) => format!("{kind_label} — not downloaded"),
                };
                div()
                    .id(("media-viewer-loading", row_id))
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .bg(bg_deep())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_sm().text_color(gpui_kit::white()).child(status))
                    .into_any_element()
            }
        };
        let visual = {
            let view = cx.entity().downgrade();
            let scroll_view = view.clone();
            let down_view = view.clone();
            let move_view = view.clone();
            let up_view = view.clone();
            // Window position of the visual's top-left (it is centered in
            // the frame): wheel zoom keeps the point under the pointer.
            let origin = (
                (f32::from(viewport.width) - media_w) / 2.0,
                VIEWER_TOP_BAR + (frame_h - media_h) / 2.0,
            );
            let menu_view = view;
            let is_photo = item.kind == MediaViewerKind::Photo;
            let menu_protected = protected;
            let menu_can_delete = can_delete;
            let menu_profile = profile_view;
            let menu_set_main = can_set_main;
            let menu_can_report = can_report;
            let menu_playable = item.kind.is_playable();
            let menu_chat_source = self.viewer.state.source() == ViewerSource::Chat;
            // `photo.has_stickers` / `video.has_stickers`: stickers were
            // added to the media (tdesktop "Attached Stickers").
            let menu_attached = self
                .session()
                .and_then(|s| s.histories.get(&item.chat_id.0))
                .and_then(|h| h.messages.get(&item.message_id.0))
                .is_some_and(|m| match &m.content {
                    quill::telegram::envelope::MessageContent::Photo(p) => p.has_stickers,
                    quill::telegram::envelope::MessageContent::Video(v) => v.has_stickers,
                    _ => false,
                });
            div()
                .id(("media-viewer-visual", row_id))
                .relative()
                .w(px(media_w))
                .h(px(media_h))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .left(px(pan_x))
                        .top(px(pan_y))
                        .w(px(zoom_w))
                        .h(px(zoom_h))
                        .child(content),
                )
                .on_scroll_wheel(move |event, _window, cx| {
                    let dy = match event.delta {
                        ScrollDelta::Pixels(p) => f32::from(p.y),
                        ScrollDelta::Lines(l) => l.y * VIEWER_WHEEL_NOTCH_PX,
                    };
                    let anchor = (
                        f32::from(event.position.x) - origin.0,
                        f32::from(event.position.y) - origin.1,
                    );
                    if let Some(view) = scroll_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer_note_activity(false, cx);
                            this.viewer_zoom_scroll(dy, anchor, cx)
                        });
                    }
                })
                .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                    let pos = (f32::from(event.position.x), f32::from(event.position.y));
                    if let Some(view) = down_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer.drag = Some(pos);
                            cx.notify();
                        });
                    }
                })
                .on_mouse_move(move |event, _window, cx| {
                    let pos = (f32::from(event.position.x), f32::from(event.position.y));
                    if let Some(view) = move_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer_note_activity(false, cx);
                            if let Some((lx, ly)) = this.viewer.drag {
                                this.viewer_pan_drag(pos.0 - lx, pos.1 - ly, cx);
                                this.viewer.drag = Some(pos);
                            }
                        });
                    }
                })
                .on_mouse_up(MouseButton::Left, move |_event, _window, cx| {
                    if let Some(view) = up_view.upgrade() {
                        view.update(cx, |this, cx| {
                            this.viewer.drag = None;
                            cx.notify();
                        });
                    }
                })
                // Double-click resets zoom/pan to fit.
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    if event.click_count() >= 2 {
                        this.viewer_reset_zoom(cx);
                    }
                }))
                // The media itself never closes the viewer.
                .occlude()
                // Right-click menu (tdesktop's viewer context menu).
                .context_menu(move |menu, _, _| {
                    let item = |label: &'static str,
                                run: fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)| {
                        let view = menu_view.clone();
                        PopupMenuItem::new(label).on_click(move |_, window, cx| {
                            let _ = view.update(cx, |this, cx| run(this, window, cx));
                        })
                    };
                    let mut menu = menu;
                    if menu_set_main {
                        menu = menu.item(item("Set as Main Photo", |this, _, cx| {
                            this.set_viewer_photo_as_main(cx)
                        }));
                    }
                    if menu_can_report {
                        menu = menu.item(item("Report", |this, _, cx| {
                            this.report_viewer_profile_photo(cx)
                        }));
                    }
                    if !menu_protected && !menu_profile {
                        menu = menu.item(item("Forward", |this, window, cx| {
                            this.share_viewer_media(window, cx)
                        }));
                    }
                    if menu_can_delete {
                        menu = menu.item(item("Delete", |this, window, cx| {
                            this.delete_viewer_media(window, cx)
                        }));
                    }
                    if !menu_protected {
                        menu = menu
                            .item(PopupMenuItem::separator())
                            .item(item("Save As…", |this, _, cx| this.save_viewer_media(cx)));
                        if is_photo {
                            menu =
                                menu.item(item("Copy", |this, _, cx| this.copy_viewer_photo(cx)));
                        } else if menu_playable {
                            menu = menu
                                .item(item("Copy Frame", |this, _, cx| this.copy_viewer_frame(cx)));
                        }
                    }
                    if menu_attached {
                        menu = menu.item(item("Attached Stickers", |this, _, cx| {
                            this.show_viewer_attached_stickers(cx)
                        }));
                    }
                    if !menu_profile {
                        menu = menu.item(item("Show in Chat", |this, _, cx| {
                            this.show_viewer_in_chat(cx)
                        }));
                    }
                    if menu_chat_source {
                        menu = menu.item(item(
                            quill::viewer_extras::view_all_label(is_photo),
                            |this, _, cx| this.view_all_viewer_media(cx),
                        ));
                    }
                    if is_photo {
                        menu = menu
                            .item(PopupMenuItem::separator())
                            .item(item("Rotate", |this, _, cx| this.rotate_viewer_photo(cx)))
                            .item(item("Flip Horizontally", |this, _, cx| {
                                this.flip_viewer_horizontal(cx)
                            }))
                            .item(item("Flip Vertically", |this, _, cx| {
                                this.flip_viewer_vertical(cx)
                            }));
                    }
                    menu
                })
        };
        // Parity slice 5: the video player panel (or a download CTA).
        let video_controls: Option<AnyElement> = (item.kind == MediaViewerKind::Video)
            .then(|| self.viewer_video_controls(&item, row_id, &files, &downloading, cx));
        let transport = Self::viewer_zoom_controls(zoom, row_id, cx);
        // Videos get the player panel; zoom controls are for photos.
        let transport = match video_controls {
            Some(controls) => controls,
            None => transport.into_any_element(),
        };
        // Custom emoji in the caption render like they do in message text:
        // the resolved sticker images, the span text until they are.
        let caption_emoji = self
            .session()
            .map(|session| {
                custom_emoji_paths(
                    &item.caption_entities,
                    &session.stickers.emoji.custom_emoji_stickers,
                    &files,
                    &roots,
                )
            })
            .unwrap_or_default();
        let caption: Option<AnyElement> = (!item.caption.is_empty()).then(|| {
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
        let sender_profile = self.viewer_sender_profile(&item);
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
                            .aria_label("Open profile")
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
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.7)
                            .truncate()
                            .child(if when.is_empty() {
                                header_label
                            } else {
                                format!("{when} \u{b7} {header_label}")
                            }),
                    )
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
                    .when(item.kind == MediaViewerKind::Photo, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-rotate", row_id),
                                gpui_kit::assets::IconName::RotateCw,
                                "Rotate",
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
                                "Set as main photo",
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
                                "Report",
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.report_viewer_profile_photo(cx);
                            })),
                        )
                    })
                    .when(!protected && !profile_view, |this| {
                        this.child(
                            icon_action(
                                ("media-viewer-share", row_id),
                                gpui_kit::assets::IconName::Forward,
                                "Share",
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
                                "Save",
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
                                "Show in chat",
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
                                "Delete",
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
                            "Close media viewer",
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
        let prev = self.viewer.state.has_prev().then(|| {
            nav_arrow(
                "media-viewer-prev",
                gpui_kit::assets::IconName::ChevronLeft,
                "Previous",
                -1,
                cx,
            )
        });
        let next = self.viewer.state.has_next().then(|| {
            nav_arrow(
                "media-viewer-next",
                gpui_kit::assets::IconName::ChevronRight,
                "Next",
                1,
                cx,
            )
        });
        let open_gen = self.viewer.open_gen as usize;
        let arrow_top = px(VIEWER_TOP_BAR + frame_h / 2.0 - 24.0);
        div()
            .id("media-viewer-overlay")
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            // Clicking anywhere outside the media and controls closes.
            .child(
                div()
                    .id("media-viewer-backdrop")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black().opacity(0.92))
                    .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                        this.viewer_note_activity(false, cx);
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_media_viewer(cx);
                    })),
            )
            .child(fade_controls(top_bar, "viewer-top-fade", fade_gen, hidden))
            .child(
                div()
                    .absolute()
                    .top(px(VIEWER_TOP_BAR))
                    .left_0()
                    .right_0()
                    .h(px(frame_h))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(visual),
            )
            .when_some(prev, |this, prev| {
                this.child(fade_controls(
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
                ))
            })
            .when_some(next, |this, next| {
                this.child(fade_controls(
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
                ))
            })
            .child(fade_controls(
                div()
                    .id("media-viewer-bottom-bar")
                    .on_mouse_move(over_controls(cx))
                    .occlude()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .px_6()
                    .pb_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .when_some(caption, |this, caption| {
                        this.child(
                            div()
                                .max_w(px(720.))
                                .text_color(gpui_kit::white())
                                .child(caption),
                        )
                    })
                    // MED1: honest playback error (unsupported format /
                    // player failure) instead of a silent stall.
                    .when_some(self.playback.error.clone(), |this, err| {
                        let can_open = item.kind.is_playable() && has_local_clip;
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(div().text_sm().text_color(danger_bright()).child(err))
                                .when(can_open, |row| {
                                    row.child(
                                        Button::new(("media-viewer-open-externally", row_id))
                                            .label("Open externally")
                                            .ghost()
                                            .text_color(gpui_kit::white())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.open_viewer_clip_externally(cx);
                                            })),
                                    )
                                }),
                        )
                    })
                    .child(transport),
                "viewer-bottom-fade",
                fade_gen,
                hidden,
            ))
            .children(saved_toast)
            // Fade the whole overlay in over 200 ms when it opens
            // (tdesktop `mediaviewShowDuration`). Closing is instant: a
            // fade-out would have to keep the closed viewer's state alive.
            .with_animation(
                ("media-viewer-fade-in", open_gen),
                Animation::new(Duration::from_millis(VIEWER_SHOW_MS)),
                |overlay, t| overlay.opacity(if still_frame() { 1.0 } else { t }),
            )
    }
}

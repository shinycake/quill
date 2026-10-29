//! The fullscreen viewer's panel (render): the media box here, the chrome
//! in `chrome`; `layer` stacks them with the backdrop and the playing
//! picture.

use super::chrome::ChromeFacts;
use super::swipe_indicator::swipe_indicator;
use super::*;
use quill::lang::{keys, tr, tr_with};

impl QuillApp {
    /// Phase 4.5 / MED1: the viewer panel: top bar, the media box (its
    /// thumbnail or picture, zoom, pan and context menu), arrows, the
    /// player controls and caption. Click-outside closes through the
    /// backdrop; Esc through `cancel_search`; the header close button is
    /// the third path. Secret/spoiler media never reach the viewer
    /// (filtered in `collect_media_items`).
    ///
    /// `part`: in the main window the media box and the chrome are two
    /// cached slices with the backdrop and the playing picture drawn by
    /// `QuillApp` around them (`layer`); chat windows draw it all inline.
    /// Takes `&mut self` + `window` so the seek/volume sliders sync during
    /// render.
    pub(in crate::ui) fn media_viewer_panel(
        &mut self,
        part: ViewerPart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if part.draws_chrome() {
            // A deleted item leaves the viewer (next item, or closed).
            self.prune_deleted_viewer_items(cx);
            self.sync_shared_media_viewer(cx);
            self.sync_viewer_window_activity(window.is_window_active(), cx);
            // A hover over the scrubber redraws only this view.
            self.viewer.controls.panel_view = Some(window.current_view());
            // MED1: keep the viewer seek/volume thumbs on the transport.
            self.sync_viewer_seek_slider(window, cx);
            self.sync_viewer_volume_slider(window, cx);
            // The controls redraw when the transport changes (the player
            // notifies the views drawing them, at most every 100 ms).
            if let Some(player) = &self.viewer.player {
                player.watch(window.current_view());
            }
        }
        let item = self
            .viewer
            .state
            .current()
            .cloned()
            .unwrap_or_else(|| MediaViewerItem {
                group: None,
                file_name: None,
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
                quality_options: Vec::new(),
            });
        // A copy: the rendering below needs `self` mutably. Reading the
        // item's files first marks them used, so they stay in the
        // session's bounded map while shown.
        let files: quill::state::FileMap = self
            .session()
            .map(|s| {
                for id in item
                    .display_file_ids
                    .iter()
                    .chain(std::iter::once(&item.download_file_id))
                    .chain(item.play_file_id.as_ref())
                {
                    let _ = s.media.files.get(&id.0);
                }
                s.media.files.clone()
            })
            .unwrap_or_default();
        let downloading: quill::state::DownloadIntents = self
            .session()
            .map(|s| s.media.downloads.clone())
            .unwrap_or_default();
        let roots = self.media_display_roots();
        let thumb_path = viewer_display_path(&item, &files, &roots);
        // Timed media can't be saved, copied or forwarded (tdesktop hides
        // Save and Share for `ttlSeconds`); its countdown shows in the
        // header and as a ring.
        let ttl = self.viewer_ttl();
        let protected =
            ttl.is_some()
                || self.viewer.state.current().zip(self.session()).is_some_and(
                    |(item, session)| session.chat_has_protected_content(item.chat_id),
                );
        let profile_facts = self.profile_photo_facts();
        let can_delete = match &profile_facts {
            Some(facts) => facts.actions.delete.is_some(),
            None => self.viewer_delete_confirm().is_some(),
        };
        let row_id = item.message_id.0 as u64;
        let downloading_now = item
            .display_file_ids
            .iter()
            .chain(std::iter::once(&item.download_file_id))
            .any(|id| file_is_downloading(*id, &files, &downloading));
        let profile_view = self.viewer.state.source() == ViewerSource::Profile;
        let kind_label = if ttl.is_some() {
            quill::media_viewer::disappearing_label(item.kind)
        } else {
            item.kind.label()
        };
        // Set as Main and Report (tdesktop's viewer menu) for profile,
        // group and channel photos.
        let can_set_main = profile_facts
            .as_ref()
            .is_some_and(|facts| facts.actions.set_main);
        let can_report = profile_facts
            .as_ref()
            .is_some_and(|facts| facts.actions.report);
        let header = profile_facts.map(|facts| facts.header);
        let can_reply = self.viewer_can_reply(&item, ttl.is_some());
        // The visual fills the fit area (`geometry`); the media fits inside
        // it (object-fit contain). Scroll zooms (1×–8×, frame-center
        // kept) and drag pans when zoomed.
        let geometry = self.viewer_geometry(&item, window, cx);
        let (media_w, media_h) = geometry.media;
        let (zoom_w, zoom_h) = geometry.zoomed;
        let (pan_x, pan_y) = geometry.pan;
        let visual = if part.draws_media() {
            // Chat windows draw the playing picture over the thumbnail here;
            // the main window's video layer draws it above the panel.
            let inline_video = (part == ViewerPart::Inline
                && item.plays_clip()
                && self.viewer.video == Some(item.message_id))
            .then(|| {
                if let Some(player) = &self.viewer.player {
                    player.set_orientation(self.viewer.orientation.turns, false);
                }
                self.viewer_inline_video()
            })
            .flatten();
            // A file that is not a picture (or too large to show as one) is a
            // card; a picture file decodes through `image_decode` (HEIC, AVIF,
            // JPEG XL, colour profiles), falling back to the card.
            let file_card = self.viewer_file_card(&item, &files, &downloading);
            let fallback_card = (item.kind == MediaViewerKind::ImageFile)
                .then(|| files::FileCard::new(&item, &files, &downloading));
            // An upright HDR picture file draws its HDR rendition (macOS).
            let hdr_frame = (file_card.is_none()
                && item.kind == MediaViewerKind::ImageFile
                && self.viewer.orientation.is_identity())
            .then_some(thumb_path.as_deref())
            .flatten()
            .and_then(|path| self.viewer_hdr_element(path, (zoom_w, zoom_h), window, cx));
            let card_view = cx.entity().downgrade();
            let content: AnyElement = if let Some(card) = file_card {
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(card.render(card_view))
                    .into_any_element()
            } else if let Some(hdr_frame) = hdr_frame {
                hdr_frame
            } else {
                // The thumbnail (under the playing picture) renders through
                // `img` from its path (async-loaded once, then cached). MED1:
                // a rotated photo renders from the eagerly-decoded
                // `viewer_rotated` cache (90°/180°/270° clockwise).
                let rotated: Option<ImageSource> = item
                    .kind
                    .is_picture()
                    .then_some(self.viewer.rotated.as_ref())
                    .flatten()
                    .filter(|(path, turns, _)| {
                        *turns == self.viewer.orientation.code()
                            && Some(path.as_path()) == thumb_path.as_deref()
                    })
                    .map(|(_, _, image)| ImageSource::from(Arc::clone(image)));
                let source: Option<ImageSource> = rotated.or_else(|| {
                    thumb_path.as_deref().map(|path| {
                        if item.kind == MediaViewerKind::ImageFile {
                            crate::ui::image_budget::full_picture(path)
                        } else {
                            ImageSource::from(path.to_path_buf())
                        }
                    })
                });
                if let Some(source) = source {
                    img(source)
                        .id(("media-viewer-img", row_id))
                        .w(px(zoom_w))
                        .h(px(zoom_h))
                        .aspect_ratio(px(zoom_w) / px(zoom_h))
                        .object_fit(ObjectFit::Contain)
                        .with_fallback(move || {
                            if let Some(card) = fallback_card.clone() {
                                return div()
                                    .w(px(zoom_w))
                                    .h(px(zoom_h))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(card.render(card_view.clone()))
                                    .into_any_element();
                            }
                            div()
                                .w(px(zoom_w))
                                .h(px(zoom_h))
                                .bg(bg_deep())
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(gpui_kit::white())
                                .child(tr_with(
                                    keys::Q_AV_COULD_NOT_RENDER,
                                    &[("kind_label", kind_label)],
                                ))
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
                        Some(pct) => tr_with(keys::Q_AV_DOWNLOADING, &[("pct", &pct.to_string())]),
                        None => tr(keys::Q_AV_DOWNLOADING_2).to_string(),
                    };
                    let status = match (&item.duration_label, downloading_now) {
                        (Some(duration), true) => {
                            format!("{kind_label} · {duration} — {downloading_label}")
                        }
                        (Some(duration), false) => tr_with(
                            keys::Q_AV_NOT_DOWNLOADED,
                            &[
                                ("kind_label", kind_label),
                                ("duration", &duration.to_string()),
                            ],
                        ),
                        (None, true) => format!("{kind_label} — {downloading_label}"),
                        (None, false) => {
                            tr_with(keys::Q_AV_NOT_DOWNLOADED_2, &[("kind_label", kind_label)])
                        }
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
            let content = match inline_video {
                Some(video) => div()
                    .relative()
                    .w(px(zoom_w))
                    .h(px(zoom_h))
                    .child(content)
                    .child(div().absolute().inset_0().child(video))
                    .into_any_element(),
                None => content,
            };
            // The download ring, the "can't be played" plate, or (chat
            // windows) the waiting spinner, over the picture.
            let picture_overlay = self.viewer_picture_overlay(
                &item,
                row_id,
                &files,
                &downloading,
                part != ViewerPart::Inline,
                cx,
            );
            Some({
                let view = cx.entity().downgrade();
                let scroll_view = view.clone();
                let pinch_view = view.clone();
                let down_view = view.clone();
                let move_view = view.clone();
                let up_view = view.clone();
                // Window position of the visual's top-left (it is centered in
                // the frame): wheel zoom keeps the point under the pointer.
                let origin = geometry.origin;
                let menu_view = view;
                let is_photo = item.kind.is_picture();
                let menu_protected = protected;
                let menu_can_delete = can_delete;
                let menu_profile = profile_view;
                let menu_set_main = can_set_main;
                let menu_can_report = can_report;
                let menu_playable = item.kind.is_playable();
                let menu_chat_source = self.viewer.state.source() == ViewerSource::Chat;
                let menu_rotate_video = self.viewer_video_rotatable();
                let menu_share_at: Option<String> = (self.viewer_can_share_at_time() && !protected)
                    .then(|| {
                        tr_with(
                            keys::MEDIAVIEW_SHARE_AT_TIME,
                            &[(
                                "time",
                                &quill::message_menu::timecode_text(f64::from(
                                    self.viewer_share_position(),
                                )),
                            )],
                        )
                    });
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
                    .children(picture_overlay)
                    .on_scroll(move |event, _window, cx| {
                        if let Some(view) = scroll_view.upgrade() {
                            view.update(cx, |this, cx| this.viewer_scroll_wheel(event, origin, cx));
                        }
                    })
                    .on_pinch(move |event, _window, cx| {
                        if let Some(view) = pinch_view.upgrade() {
                            view.update(cx, |this, cx| this.viewer_pinch(event, origin, cx));
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
                                this.viewer_note_pointer(false, cx);
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
                        } else if this.viewer.extra.video_fullscreen.is_on() {
                            this.toggle_viewer_video(cx);
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
                            menu = menu
                                .item(item(tr(keys::MEDIAVIEW_SET_USERPIC), |this, _, cx| {
                                    this.set_viewer_photo_as_main(cx)
                                }));
                        }
                        if menu_can_report {
                            menu = menu.item(item(
                                tr(keys::MEDIAVIEW_REPORT_PROFILE_PHOTO),
                                |this, _, cx| this.report_viewer_profile_photo(cx),
                            ));
                        }
                        if !menu_protected && !menu_profile {
                            menu = menu.item(item(tr(keys::Q_AV_FORWARD), |this, window, cx| {
                                this.share_viewer_media(window, cx)
                            }));
                        }
                        if menu_can_delete {
                            menu = menu.item(item(tr(keys::BOX_DELETE), |this, window, cx| {
                                this.delete_viewer_media(window, cx)
                            }));
                        }
                        if !menu_protected {
                            menu = menu
                                .item(PopupMenuItem::separator())
                                .item(item(tr(keys::Q_AV_SAVE_AS_2), |this, _, cx| {
                                    this.save_viewer_media(cx)
                                }));
                            if is_photo {
                                menu = menu.item(item(tr(keys::Q_AV_COPY), |this, _, cx| {
                                    this.copy_viewer_photo(cx)
                                }));
                            } else if menu_playable {
                                menu = menu.item(item(tr(keys::Q_AV_COPY_FRAME), |this, _, cx| {
                                    this.copy_viewer_frame(cx)
                                }));
                            }
                        }
                        if let Some(label) = menu_share_at.clone() {
                            let view = menu_view.clone();
                            menu = menu.item(PopupMenuItem::new(label).on_click(
                                move |_, window, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.share_viewer_video_at_time(window, cx)
                                    });
                                },
                            ));
                        }
                        if menu_attached {
                            menu = menu
                                .item(item(tr(keys::Q_AV_ATTACHED_STICKERS), |this, _, cx| {
                                    this.show_viewer_attached_stickers(cx)
                                }));
                        }
                        if !menu_profile {
                            menu = menu.item(item(tr(keys::Q_AV_SHOW_IN_CHAT), |this, _, cx| {
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
                                .item(item(tr(keys::Q_AV_ROTATE), |this, _, cx| {
                                    this.rotate_viewer_photo(cx)
                                }))
                                .item(item(tr(keys::Q_AV_FLIP_HORIZONTALLY), |this, _, cx| {
                                    this.flip_viewer_horizontal(cx)
                                }))
                                .item(item(tr(keys::Q_AV_FLIP_VERTICALLY), |this, _, cx| {
                                    this.flip_viewer_vertical(cx)
                                }));
                        }
                        if menu_rotate_video {
                            menu = menu
                                .item(PopupMenuItem::separator())
                                .item(item(tr(keys::Q_AV_ROTATE), |this, _, cx| {
                                    this.rotate_viewer_video(cx)
                                }));
                        }
                        menu
                    })
            })
        } else {
            None
        };
        let chrome = part.draws_chrome().then(|| {
            let facts = ChromeFacts {
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
            };
            self.viewer_chrome(&item, facts, geometry, window, cx)
        });
        let open_gen = self.viewer.open_gen as usize;
        let viewport = window.viewport_size();
        let swipe = self
            .viewer
            .swipe
            .indicator()
            .filter(|_| part.draws_chrome())
            .map(|shown| swipe_indicator(shown, geometry.fit, f32::from(viewport.width)));
        // Layered, the panel takes no clicks itself: the backdrop under it
        // closes the viewer.
        let root = if part == ViewerPart::Inline {
            div().absolute().inset_0().child(
                div()
                    .id("media-viewer-backdrop")
                    .occlude()
                    .absolute()
                    .inset_0()
                    .bg(gpui_kit::black())
                    .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                        this.viewer_note_pointer(false, cx);
                    }))
                    .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                        this.viewer_scroll_wheel(event, geometry.origin, cx);
                    }))
                    .on_pinch(cx.listener(move |this, event: &PinchEvent, _, cx| {
                        this.viewer_pinch(event, geometry.origin, cx);
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.viewer_backdrop_click(cx);
                    })),
            )
        } else {
            // The slice's box has no height of its own (a cached view over
            // absolute content): size the panel to the window, or `bottom_0`
            // puts the controller above the top edge.
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(viewport.width)
                .h(viewport.height)
        };
        let (left, top) = geometry.origin;
        let panel = root
            .children(
                visual.map(|visual| div().absolute().left(px(left)).top(px(top)).child(visual)),
            )
            .children(chrome.into_iter().flatten())
            .children(swipe)
            // Fade in over 200 ms when it opens (tdesktop
            // `mediaviewShowDuration`). Closing is instant: a fade-out
            // would have to keep the closed viewer's state alive.
            .with_animation(
                ("media-viewer-panel-fade-in", open_gen),
                Animation::new(Duration::from_millis(VIEWER_SHOW_MS)),
                |panel, t| panel.opacity(if still_frame() { 1.0 } else { t }),
            );
        if part == ViewerPart::Inline {
            div()
                .id("media-viewer-overlay")
                .occlude()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(panel)
                .into_any_element()
        } else {
            panel.into_any_element()
        }
    }
}

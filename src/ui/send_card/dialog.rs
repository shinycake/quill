//! The send box itself: a dialog over the chat that holds the picked files
//! until they are sent or dropped (tdesktop `SendFilesBox`).
//!
//! It has no open flag of its own. It shows while the open chat's tray
//! holds files ([`QuillApp::send_box_shown`]), so it opens for every way
//! files get there (the attach menu, a paste, a drop) and closes with the
//! send or with Cancel, which empties the tray. The tray is per chat and
//! per window, and so is the box.
//!
//! The caption is the message field: one state, drawn here instead of in
//! the composer bar while the box shows. What was typed before the files
//! were picked is the caption, Enter sends through the composer's own path
//! with every option, and after Cancel the text is back in the bar.
//!
//! The box does not close on a click outside it (tdesktop
//! `setCloseByOutsideClick(false)`: a stray click must not drop the
//! files). A box opened from it (the video editor, the price) ranks before
//! it and shows instead until it closes (`DialogSpec` priority), and the
//! photo editor covers the window while the box steps aside.
#![deny(clippy::indexing_slicing)]

use super::layout::{BoxRows, MOSAIC_WIDTH, TrayLayout, media_room, scroll_room};
use super::options::tray_title;
use crate::ui::app::{PaneMode, QuillApp};
use crate::ui::composer_sends::SendOnce;
use crate::ui::dismiss_layer::DismissLayer;
use crate::ui::media_panel::PanelTab;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::window_actions::WindowActions;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::send_button::{SendButtonKind, format_slowmode};

/// The box's width: the mosaic's room, the previews' inset and border (18)
/// and the dialog's own padding and border (34).
const BOX_WIDTH: f32 = MOSAIC_WIDTH as f32 + 52.;
/// The caption takes the focus when the box appears. Kept as the box's
/// element state, so it happens once each time the box is put on screen.
struct CaptionFocused;

impl QuillApp {
    /// Whether the send box shows in this window: its chat's tray holds
    /// files and can be sent from. A photo being edited covers the window
    /// (`photo_editor_overlay`), so the box steps aside for it and comes
    /// back with the result.
    pub(in crate::ui) fn send_box_shown(&self) -> bool {
        !self.pending_attachments().is_empty()
            && !self.chat_ui.is_editing()
            && matches!(self.pane_mode(), PaneMode::Ready)
            && self.composer_available(PaneMode::Ready)
            && !self
                .viewer
                .photo_editor
                .as_ref()
                .is_some_and(|editor| !editor.is_profile())
    }

    /// The emoji panel is open on the box's caption.
    pub(in crate::ui) fn send_box_emoji_shown(&self) -> bool {
        self.send_box_shown() && self.pickers.media_panel.composing_emoji()
    }

    /// The panel the box hosts goes with the box (the tray was sent or
    /// dropped): it would otherwise pop up over the composer.
    pub(in crate::ui) fn close_send_box_emoji(&mut self) {
        if self.send_box_emoji_shown() {
            self.set_media_panel_mode(crate::ui::media_panel::PanelMode::Closed);
            self.pickers.media_panel.hovered = None;
        }
    }

    fn toggle_send_box_emoji(&mut self, cx: &mut Context<Self>) {
        if self.send_box_emoji_shown() {
            self.close_media_panel(cx);
        } else {
            self.open_media_panel(PanelTab::Emoji, cx);
        }
    }

    /// The lines typed in the caption (a wrapped line counts once).
    fn caption_lines(&self, cx: &App) -> usize {
        self.composer.read(cx).value().split('\n').count()
    }

    fn focus_caption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
    }

    /// Cancel, the close button or Escape: the files go, the text stays in
    /// the field, which is the composer's again and takes the focus.
    pub(in crate::ui) fn cancel_send_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_send_menu(cx);
        self.clear_attachment(cx);
        self.focus_caption(window, cx);
    }

    /// Escape closes what is open over the box first. `true` when the box
    /// itself may close.
    fn send_box_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.send_box_emoji_shown() {
            self.close_media_panel(cx);
            self.focus_caption(window, cx);
            return false;
        }
        if self.chat_ui.schedule_popup_shown() {
            self.close_schedule_picker(cx);
            return false;
        }
        true
    }

    /// The Send button: the composer's own send, with the caption and
    /// every option of the tray.
    fn send_from_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submit_from_send_button(SendOnce::default(), window, cx);
        // Sent (not held for a render, not refused): the box is done, and
        // closing it now puts the focus where the kit found it before the
        // composer takes it.
        if self.pending_attachments().is_empty() {
            self.close_kit_dialog_if_done(DialogKind::SendBox, window, cx);
            self.focus_caption(window, cx);
        }
    }

    /// What the box shows above the caption: the files as they will be
    /// sent, then every send option. In a box that may stand `box_max`
    /// tall the previews take what the rest leaves, so a short window
    /// shrinks them before any option row scrolls out of view.
    fn send_box_body(&self, box_max: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let layout = TrayLayout::of(
            self.pending_attachments(),
            self.composer_group_media_effective(),
        );
        let rows = self.send_card_option_rows(cx);
        let room = media_room(
            f32::from(box_max) as i32,
            BoxRows {
                options: rows.len(),
                files: layout.files.len(),
                caption_lines: self.caption_lines(cx),
            },
        );
        let theme = cx.theme();
        let (surface, accent) = (theme.muted, theme.primary);
        let previews = div()
            .id("send-box-previews")
            .debug_selector(|| "send-box-previews".to_string())
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(surface)
            .bg(surface)
            // Files dragged over the previews will join them
            // (`send_box_drop_layer` takes the drop).
            .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_color(accent))
            .children(self.send_card_media(&layout, layout.max_height(room), cx))
            .children(self.send_card_files(&layout, cx));
        div()
            .id("send-box-body")
            .flex()
            .flex_col()
            .gap_3()
            .child(previews)
            .when(!rows.is_empty(), |body| {
                body.child(
                    div()
                        .id("send-box-options")
                        .debug_selector(|| "send-box-options".to_string())
                        .flex()
                        .flex_col()
                        .gap_2()
                        .children(rows),
                )
            })
            .into_any_element()
    }

    /// The caption and the buttons, under the scrolling part: always in
    /// view, whatever the tray holds.
    fn send_box_footer(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        let scope = self.windows.scope;
        div()
            .id("send-box-footer")
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            // The field's keys and the paste that adds files: the kit's
            // dialog layer is outside the window root that binds them.
            .caption_actions(owner, scope)
            .children(self.composer_options_row(cx))
            .children(self.text_limit_bar(cx))
            .child(self.send_box_caption(cx))
            .child(self.send_box_buttons(cx))
            .into_any_element()
    }

    /// The message field as the caption, with the emoji toggle. What hangs
    /// off the field in the composer (mentions, emoji and command menus,
    /// inline results, the schedule picker) floats over the box from the
    /// caption's top edge.
    fn send_box_caption(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (border, surface, radius) = (theme.input, theme.background, theme.radius);
        let floats: Vec<AnyElement> = [
            self.mention_menu_dropdown(cx),
            self.suggest_menu_dropdown(cx),
            self.command_menu_dropdown(cx),
            self.inline_results_dropdown(cx),
            self.chat_ui
                .schedule_popup_shown()
                .then(|| self.schedule_popup(cx)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let field = div()
            .id("send-box-caption")
            .debug_selector(|| "send-box-caption".to_string())
            .relative()
            .flex()
            .items_end()
            .gap_1()
            .pl_2()
            .pr_1()
            .py_1()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(surface)
            .child(self.composer_field(cx))
            .child(self.send_box_emoji_toggle(cx))
            // Emoji for `:word` or a pointed-at word, as wide as the field.
            .when_some(self.emoji_bar_overlay(cx), |row, bar| row.child(bar));
        div()
            .relative()
            .child(field)
            .when(!floats.is_empty(), |wrap| {
                wrap.child(
                    div()
                        .id("send-box-floats")
                        .occlude()
                        .absolute()
                        .bottom_full()
                        .left_0()
                        .right_0()
                        .pb_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(floats),
                )
            })
            .into_any_element()
    }

    /// The emoji toggle at the caption's end; the open panel hangs over
    /// the box from its top-right corner.
    fn send_box_emoji_toggle(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let shown = self.send_box_emoji_shown();
        let toggle = Button::new("send-box-emoji")
            .debug_selector(|| "send-box-emoji".to_string())
            .icon(IconName::FaceSlightlySmiling)
            .ghost()
            .small()
            .selected(shown)
            .tooltip(tr(keys::SWITCH_EMOJI))
            .accessibility_label(tr(keys::SWITCH_EMOJI))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_send_box_emoji(cx)));
        let panel = shown.then(|| {
            let panel = self.media_panel(cx);
            // A point at the toggle's top-right corner to hang the panel
            // from. The layer that closes it on a click elsewhere covers
            // the window under it (`send_box_emoji_cover`).
            div().absolute().bottom_full().right_0().size_0().child(
                deferred(
                    anchored()
                        .anchor(Anchor::BottomRight)
                        .offset(point(px(0.), px(-8.)))
                        .snap_to_window_with_margin(px(8.))
                        .child(
                            div()
                                .id("send-box-emoji-panel")
                                .debug_selector(|| "send-box-emoji-panel".to_string())
                                .occlude()
                                .child(panel),
                        ),
                )
                .with_priority(gpui_base::POPUP_PRIORITY),
            )
        });
        div()
            .relative()
            .flex_none()
            .child(toggle)
            .children(panel)
            .into_any_element()
    }

    /// Add, what the tray waits on, Cancel and Send.
    fn send_box_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let kind = self.send_button_kind_now(false, true);
        let wait = self.slow_mode_wait_secs();
        let label = match kind {
            SendButtonKind::Slowmode => format_slowmode(wait.unwrap_or(0)),
            SendButtonKind::Schedule => tr(keys::SCHEDULE_BUTTON).to_string(),
            _ => tr(keys::SEND_BUTTON).to_string(),
        };
        let send = Button::new("send-box-send")
            .debug_selector(|| "send-box-send".to_string())
            .label(label)
            .primary()
            .disabled(kind == SendButtonKind::Slowmode)
            .tooltip(kind.tooltip(wait))
            .on_click(cx.listener(|this, _, window, cx| this.send_from_box(window, cx)));
        let options = Button::new("send-box-send-options")
            .debug_selector(|| "send-box-send-options".to_string())
            .icon(IconName::ChevronDown)
            .primary()
            .tooltip(tr(keys::Q_CHAT_SEND_OPTIONS))
            .accessibility_label(tr(keys::Q_CHAT_SEND_OPTIONS))
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                this.open_send_menu(event.position(), true, window, cx);
            }));
        div()
            .id("send-box-buttons")
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::new("send-box-add")
                    .debug_selector(|| "send-box-add".to_string())
                    .icon(IconName::Plus)
                    .label(tr(keys::Q_MEDIA_ADD))
                    .ghost()
                    .tooltip(tr(keys::Q_CHAT_CHOOSE_FILES))
                    .on_click(cx.listener(|this, _, _, cx| this.pick_attachments(false, cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .when_some(self.send_card_status(), |row, status| {
                        row.child(Spinner::new().xsmall().color(muted))
                            .child(div().truncate().child(status))
                    }),
            )
            .child(
                Button::new("send-box-cancel")
                    .debug_selector(|| "send-box-cancel".to_string())
                    .label(tr(keys::CANCEL))
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.cancel_send_box(window, cx);
                        this.close_kit_dialog_if_done(DialogKind::SendBox, window, cx);
                        // After the kit handed the focus back to where it
                        // was before the box.
                        this.focus_caption(window, cx);
                    })),
            )
            .child(
                // Right-click, or the arrow: send without sound, schedule
                // and, in a chat with a user, the effects (`send_effects`).
                div()
                    .id("send-box-send-wrap")
                    .flex()
                    .items_center()
                    .gap(px(1.))
                    .when(kind.has_send_menu(), |wrap| {
                        wrap.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.open_send_menu(event.position, true, window, cx);
                            }),
                        )
                    })
                    .child(send)
                    .when(kind.has_send_menu(), |wrap| wrap.child(options))
                    .children(self.send_menu_overlay(cx)),
            )
            .into_any_element()
    }

    fn build_send_box(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::SendBox, |this, window, cx| {
                this.cancel_send_box(window, cx);
            });
        let (title, footer, emoji_shown) = app.update(cx, |this, cx| {
            (
                tray_title(this.pending_attachments()),
                this.send_box_footer(cx),
                this.send_box_emoji_shown(),
            )
        });
        let composer = app.read(cx).composer.clone();
        let escape = app.downgrade();
        let dismiss = app.downgrade();
        let drops = app.downgrade();
        let body_app = app.clone();
        dialog
            .overlay(true)
            // A click outside must not drop the files.
            .overlay_closable(false)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(title))
            .content(move |content, window, cx| {
                let composer = composer.clone();
                window.use_keyed_state("send-box-caption-focus", cx, move |window, cx| {
                    composer.update(cx, |input, cx| input.focus(window, cx));
                    CaptionFocused
                });
                let viewport = window.viewport_size();
                // The kit keeps this margin clear at the window's top and
                // bottom: the box is at most the rest.
                let box_max = viewport.height - cx.theme().spacing_tokens().lg * 2.;
                // The body follows the window's height, known only here,
                // and is the window's own: its state is put in place.
                let (body, lines) = body_app
                    .update(cx, |this, cx| {
                        this.in_window_of(window, cx, |this, _, cx| {
                            (this.send_box_body(box_max, cx), this.caption_lines(cx))
                        })
                    })
                    .unwrap_or_else(|| (div().into_any_element(), 1));
                let available = px(scroll_room(f32::from(box_max) as i32, lines) as f32);
                let border = cx.theme().border;
                let content = content
                    .min_h_0()
                    .child(send_box_drop_layer(viewport, drops.clone()))
                    .child(
                        div()
                            .id("send-box-scroll")
                            .debug_selector(|| "send-box-scroll".to_string())
                            .role(Role::ScrollView)
                            .aria_label(tr(keys::Q_MISC_DIALOG_CONTENT))
                            .w_full()
                            .max_h(available)
                            .overflow_y_scroll()
                            .pb_2()
                            .border_b_1()
                            .border_color(border)
                            .child(body),
                    );
                if emoji_shown {
                    content.child(send_box_emoji_cover(viewport, dismiss.clone()))
                } else {
                    content
                }
            })
            .footer(footer)
            .on_cancel(move |_, window, cx| {
                escape
                    .update(cx, |this, cx| this.send_box_escape(window, cx))
                    .unwrap_or(true)
            })
            .on_close(on_close)
    }
}

/// Above the kit dialog's layer (10), under every popup.
const DROP_LAYER_PRIORITY: usize = 11;

/// Files dropped anywhere in the window join the box, not only on the box
/// itself. The layer is as large as the window and over the dialog, and
/// takes nothing but a file drop: it does not occlude, so clicks, hover and
/// scroll reach what is under it.
fn send_box_drop_layer(viewport: gpui_kit::Size<Pixels>, app: WeakEntity<QuillApp>) -> AnyElement {
    deferred(
        anchored().position(point(px(0.), px(0.))).child(
            div().w(viewport.width).h(viewport.height).on_drop(
                move |paths: &ExternalPaths, _, cx| {
                    let _ = app.update(cx, |this, cx| this.drop_files_plain(paths.paths(), cx));
                },
            ),
        ),
    )
    .with_priority(DROP_LAYER_PRIORITY)
    .into_any_element()
}

/// The layer under the open emoji panel: it covers the window, so the box
/// under it takes no click, hover or scroll, and a click on it closes the
/// panel. The panel is drawn over it and occludes it.
fn send_box_emoji_cover(viewport: gpui_kit::Size<Pixels>, app: WeakEntity<QuillApp>) -> AnyElement {
    let layer = DismissLayer::new("send-box-emoji-cover", move |_, window, cx| {
        let _ = app.update(cx, |this, cx| {
            this.close_media_panel(cx);
            this.focus_caption(window, cx);
        });
    });
    deferred(
        anchored().position(point(px(0.), px(0.))).child(
            div()
                .relative()
                .w(viewport.width)
                .h(viewport.height)
                .child(layer),
        ),
    )
    .with_priority(gpui_base::POPUP_PRIORITY - 1)
    .into_any_element()
}

crate::ui::shell::register_dialogs! {
    // Lower wins, and this is the last of all: every box opened from the
    // send box (the video editor 5442, the price 5441, a link, a schedule
    // confirmation) shows over it, and it comes back when that one closes.
    //
    // No `close`: that runs when a chat window closes, and the files wait
    // for their chat as they did above the composer (a photo edit or a held
    // Send still lands in the tray). They show their box again wherever the
    // chat is opened next.
    SendBox => DialogSpec::new(
        7900,
        |app| app.send_box_shown(),
        QuillApp::build_send_box,
    )
    .per_window(),
}

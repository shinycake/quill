//! The Style tab's pills with the "Create" pill after them and the custom
//! styles' context menu (tdesktop compose_ai_box.cpp `setupStyleTabs` and
//! `fillToneMenu`), the remove / delete confirmation
//! (create_ai_tone_box.cpp `ConfirmDeleteAiTone`) and the apply hotkey's
//! binding.

use super::pill_radius;
use crate::ui::actions::AiComposeApply;
use crate::ui::ai_compose_state::AiComposeView;
use crate::ui::app::QuillApp;
use crate::ui::shell::DialogKind;
use crate::ui::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ai_compose::{StyleChoice, TextCompositionStyle, prefs, style_link};
use quill::lang::{keys, tr, tr_with};
use quill::settings_catalog::Page;

/// The apply hotkey as the app shows chords (`⌘⇧A`), or `None` while
/// the action has no binding (tdesktop `AiApplyShortcutText`).
pub(super) fn ai_apply_shortcut_text(cx: &App) -> Option<String> {
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let binding = keymap.bindings_for_action(&AiComposeApply).last()?;
    let text: Vec<String> = binding
        .keystrokes()
        .iter()
        .map(|keystroke| Kbd::format(keystroke.inner()))
        .collect();
    (!text.is_empty()).then(|| text.join(" "))
}

pub(super) fn bound_to(name: &str) -> bool {
    prefs::current().bound_style_name.as_deref() == Some(name)
}

pub(super) fn set_bound_style(name: Option<&str>) {
    prefs::update(|prefs| prefs.bound_style_name = name.map(str::to_string));
}

impl QuillApp {
    /// The pills (tdesktop `LabeledEmojiScrollTabs`) and "Create" last.
    pub(super) fn ai_compose_style_row(
        &self,
        picked: &StyleChoice,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(session) = self.session() else {
            return div().into_any_element();
        };
        let owner = cx.entity().downgrade();
        let pills = session
            .messages
            .ai_compose
            .styles
            .styles()
            .iter()
            .map(|style| {
                let selected = matches!(picked, StyleChoice::Style(name) if *name == style.name);
                let name = style.name.clone();
                let pill = Button::new(SharedString::from(format!("ai-style-{}", style.name)))
                    .small()
                    .rounded(pill_radius())
                    .map(|pill| {
                        if selected {
                            pill.primary()
                        } else {
                            pill.outline()
                        }
                    })
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(self.ai_style_glyph(style))
                            .child(style.title.clone()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        prefs::update(|prefs| prefs.style_tooltip_hidden = true);
                        this.ai_compose_update(window, cx, |ai| ai.select_style(&name));
                    }));
                let menu_style = style.clone();
                let owner = owner.clone();
                let slot = div()
                    .id(SharedString::from(format!("ai-style-menu-{}", style.name)))
                    .flex_none()
                    .child(pill);
                if style.is_custom {
                    slot.context_menu(move |menu, _, cx| {
                        ai_style_menu(menu, &owner, &menu_style, cx)
                    })
                    .into_any_element()
                } else {
                    slot.into_any_element()
                }
            });
        let create = div()
            .debug_selector(|| "ai-style-create".into())
            .flex_none()
            .child(
                Button::new("ai-style-create")
                    .small()
                    .outline()
                    .rounded(pill_radius())
                    .icon(IconName::Plus)
                    .label(tr(keys::AI_COMPOSE_TONE_CREATE))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_ai_style_editor(None, window, cx);
                    })),
            );
        h_flex()
            .id("ai-compose-styles")
            .gap_1p5()
            .pb_1()
            .overflow_x_scroll()
            .children(pills)
            .child(create)
            .into_any_element()
    }

    /// "Remove Style" for an added style, "Delete Style" for the user's
    /// own, after tdesktop's confirmation. `from_preview`: the preview
    /// asked, so its toast shows and it closes once the server agrees; the
    /// editor goes back to the card.
    pub(super) fn confirm_remove_ai_style(
        &mut self,
        name: &str,
        from_preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let creator = self
            .session()
            .and_then(|s| s.messages.ai_compose.styles.find(name))
            .is_some_and(|style| style.is_creator);
        let app = cx.entity().downgrade();
        let name = name.to_string();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, name) = (app.clone(), name.clone());
            let alert = if creator {
                alert
                    .title(tr(keys::AI_COMPOSE_TONE_DELETE))
                    .description(tr(keys::AI_COMPOSE_TONE_DELETE_SURE))
                    .ok_text(tr(keys::BOX_DELETE))
                    .ok_variant(ButtonVariant::Danger)
            } else {
                alert
                    .description(tr(keys::AI_COMPOSE_TONE_REMOVE_SURE))
                    .ok_text(tr(keys::BOX_REMOVE))
            };
            alert
                .cancel_text(tr(keys::CANCEL))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| {
                        this.remove_ai_style(&name, creator, from_preview, cx);
                    });
                    true
                })
        });
    }

    fn remove_ai_style(
        &mut self,
        name: &str,
        creator: bool,
        from_preview: bool,
        cx: &mut Context<Self>,
    ) {
        if from_preview && let Some(preview) = self.ai_compose.preview.as_mut() {
            preview.removing = true;
        }
        if let Some(open) = self.ai_compose.open.as_mut()
            && open.editor.as_ref().and_then(|e| e.editing.as_deref()) == Some(name)
        {
            open.editor = None;
            open.view = AiComposeView::Card;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = if creator {
                live.driver.delete_text_composition_style(name)
            } else {
                live.driver.remove_text_composition_style(name)
            };
        }
        cx.notify();
    }

    fn share_ai_style(&mut self, link: String, window: &mut Window, cx: &mut Context<Self>) {
        self.close_ai_compose(cx);
        self.close_kit_dialog_if_done(DialogKind::AiCompose, window, cx);
        self.share.link_text = Some(link);
        self.connection.set_note(StatusNote::silent(tr(
            keys::Q_MISC_CHOOSE_A_CHAT_TO_SHARE_2,
        )));
        cx.notify();
    }

    fn open_ai_apply_hotkey_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_ai_compose(cx);
        self.close_kit_dialog_if_done(DialogKind::AiCompose, window, cx);
        self.open_settings(Some(Page::Shortcuts));
        cx.notify();
    }
}

fn ai_style_menu(
    mut menu: PopupMenu,
    owner: &WeakEntity<QuillApp>,
    style: &TextCompositionStyle,
    cx: &App,
) -> PopupMenu {
    let act = |f: fn(&mut QuillApp, &TextCompositionStyle, &mut Window, &mut Context<QuillApp>)| {
        let owner = owner.clone();
        let style = style.clone();
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            let _ = owner.update(cx, |this, cx| f(this, &style, window, cx));
        }
    };
    if !style.name.is_empty() {
        menu = match ai_apply_shortcut_text(cx) {
            Some(shortcut) => menu.item(
                PopupMenuItem::new(tr_with(
                    keys::AI_COMPOSE_BIND_USE_HOTKEY,
                    &[("keys", &shortcut)],
                ))
                .checked(bound_to(&style.name))
                .on_click(act(|_, style, _, cx| {
                    let bound = bound_to(&style.name);
                    set_bound_style((!bound).then_some(style.name.as_str()));
                    cx.notify();
                })),
            ),
            None => menu.item(
                PopupMenuItem::new(tr(keys::AI_COMPOSE_BIND_SET_HOTKEY_SHORT))
                    .icon(IconName::Keyboard)
                    .on_click(act(|this, _, window, cx| {
                        this.open_ai_apply_hotkey_settings(window, cx);
                    })),
            ),
        };
    }
    if style.is_creator {
        menu = menu.item(
            PopupMenuItem::new(tr(keys::AI_COMPOSE_TONE_EDIT))
                .icon(IconName::Pencil)
                .on_click(act(|this, style, window, cx| {
                    this.open_ai_style_editor(Some(style.clone()), window, cx);
                })),
        );
    }
    if let Some(link) = style_link(&style.name) {
        menu = menu.item(
            PopupMenuItem::new(tr(keys::AI_COMPOSE_TONE_SHARE))
                .icon(IconName::Share2)
                .on_click({
                    let owner = owner.clone();
                    move |_, window, cx| {
                        let link = link.clone();
                        let _ = owner.update(cx, |this, cx| this.share_ai_style(link, window, cx));
                    }
                }),
        );
    }
    let label = if style.is_creator {
        tr(keys::AI_COMPOSE_TONE_DELETE)
    } else {
        tr(keys::AI_COMPOSE_TONE_REMOVE)
    };
    let danger = cx.theme().danger;
    menu.item(
        PopupMenuItem::element(move |_, _| div().text_color(danger).child(label))
            .icon(Icon::new(IconName::Trash).text_color(danger))
            .on_click(act(|this, style, window, cx| {
                this.confirm_remove_ai_style(&style.name, false, window, cx);
            })),
    )
}

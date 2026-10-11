//! Message translation: the translate box (original, target language, the
//! translated text with entities, copy), the language chooser, the chat
//! translate bar with its menu, translated bubbles, and the translation
//! options in Settings. Modelled on Telegram Desktop's `boxes/translate_box`,
//! `history_view_translate_bar` and `history_view_translate_tracker`.

use super::app::QuillApp;
use super::message_text::rich_text_line;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::{HistoryMessage, Session, Translation};
use quill::telegram::envelope::ChatKind;
use quill::text::TextEntity;
use quill::translate::{
    TranslatePrefs, bar_label_for, choose_translate_to, detect_language, language_name,
    offer_language, replace_content_text, search_languages, tracking_enabled, translatable_content,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

/// Messages with a translation request in flight at once (tdesktop sends
/// batches of 20).
const MAX_IN_FLIGHT: usize = 20;
/// How long the bar's toast stays.
const TOAST_DURATION: Duration = Duration::from_secs(4);
/// The original text shows this many lines before "Show".
const ORIGINAL_LINES: f32 = 3.;
/// The original is considered long (offers "Show") past this many chars.
const LONG_ORIGINAL: usize = 140;

/// What the translate box translates.
#[derive(Clone, Debug)]
pub(super) enum TranslateSource {
    /// A whole message (`translateMessageText`).
    Message {
        chat_id: ChatId,
        message_id: MessageId,
        text: String,
        entities: Vec<TextEntity>,
    },
    /// Selected text (`translateText`).
    Selection { chat_id: ChatId, text: String },
}

/// Which page of the translate dialog is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TranslateView {
    /// Original and translation.
    Box,
    /// Pick the "Translate to" language.
    ChooseTo,
    /// Pick the "Do Not Translate" languages (Settings).
    SkipList,
}

pub(super) struct TranslateDialog {
    pub(super) view: TranslateView,
    pub(super) source: Option<TranslateSource>,
    pub(super) to: &'static str,
    /// `translateText` job of a selection.
    pub(super) job: Option<u64>,
    pub(super) original_expanded: bool,
    /// Chooser opened from the bar menu: close it after a pick instead of
    /// returning to the box.
    pub(super) close_after_choose: bool,
    pub(super) note: Option<String>,
    pub(super) search: Entity<TextareaState>,
    _search_subscription: Subscription,
}

/// A line under the bar with one action ("Undo", "Settings").
#[derive(Clone, Debug)]
pub(super) struct TranslateToast {
    chat_id: ChatId,
    text: String,
    label: &'static str,
    action: ToastAction,
    id: u64,
}

#[derive(Clone, Copy, Debug)]
enum ToastAction {
    ShowBar,
    OpenSkipList,
}

#[derive(PartialEq, Eq)]
struct OfferKey {
    chat: i64,
    revision: u64,
    skip: Vec<&'static str>,
}

/// What the bar shows.
pub(super) struct TranslateBarModel {
    to: &'static str,
    from: Option<&'static str>,
    translated: bool,
    /// The channel translates automatically (`autoTranslation`): the bar
    /// works without Premium and also covers the user's own messages.
    automatic: bool,
}

pub(super) struct TranslateUi {
    pub(super) prefs: TranslatePrefs,
    pub(super) dialog: Option<TranslateDialog>,
    toast: Option<TranslateToast>,
    toast_seq: u64,
    /// Screenshot demo: the Appearance dialog shows only the translation
    /// options.
    pub(super) settings_only: bool,
    offer: RefCell<Option<(OfferKey, Option<&'static str>)>>,
}

impl TranslateUi {
    pub(super) fn load() -> Self {
        Self {
            prefs: quill::settings::load_translate_prefs(&QuillApp::appearance_paths()),
            dialog: None,
            toast: None,
            toast_seq: 0,
            settings_only: false,
            offer: RefCell::new(None),
        }
    }
}

fn muted_text(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

impl QuillApp {
    // ---------------------------------------------------------------------
    // State helpers
    // ---------------------------------------------------------------------

    /// The app language tag ("Translate to" and "Do Not Translate" default
    /// to it).
    fn translate_ui_language(&self) -> String {
        self.session()
            .map(|s| s.settings.language_prefs.system_language_code.clone())
            .unwrap_or_else(|| quill::settings::DEFAULT_LANGUAGE_CODE.to_string())
    }

    fn set_translate_prefs(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut TranslatePrefs)) {
        f(&mut self.settings.translate.prefs);
        if let Err(err) = quill::settings::save_translate_prefs(
            &Self::appearance_paths(),
            &self.settings.translate.prefs,
        ) {
            self.connection.status_note = format!("Couldn't save translation settings: {err}");
        }
        cx.notify();
    }

    /// `translateOfferedFrom`: the language most recent incoming messages
    /// are in, unless it is one the user does not translate. Cached per
    /// loaded-messages revision.
    fn translate_offer(&self, chat_id: ChatId, skip: &[&'static str]) -> Option<&'static str> {
        let session = self.session()?;
        let history = session.histories.get(&chat_id.0)?;
        let key = OfferKey {
            chat: chat_id.0,
            revision: history.messages.revision(),
            skip: skip.to_vec(),
        };
        if let Some((cached, value)) = self.settings.translate.offer.borrow().as_ref()
            && *cached == key
        {
            return *value;
        }
        let texts: Vec<&str> = history
            .ordered()
            .into_iter()
            .rev()
            .filter(|message| !message.is_outgoing || session.chat_auto_translate(chat_id))
            .filter_map(|message| translatable_content(&message.content).map(|(text, _)| text))
            .collect();
        let value = offer_language(texts, skip);
        *self.settings.translate.offer.borrow_mut() = Some((key, value));
        value
    }

    /// The chat's translate bar, or `None` when it is not offered: the
    /// setting is off, the account has no Premium, TDLib does not suggest
    /// translating the chat, the user hid the bar, or the recent messages
    /// are not in a language to translate.
    pub(super) fn translate_bar_model(&self, chat_id: ChatId) -> Option<TranslateBarModel> {
        let session = self.session()?;
        let prefs = &self.settings.translate.prefs;
        let automatic = session.chat_auto_translate(chat_id);
        if !tracking_enabled(prefs.translate_chats, session.is_premium(), automatic)
            || !session.chat_is_translatable(chat_id)
            || prefs.bar_hidden(chat_id.0)
            || session
                .chats
                .get(&chat_id.0)
                .is_none_or(|chat| matches!(chat.kind, ChatKind::Secret { .. }))
        {
            return None;
        }
        let ui = self.translate_ui_language();
        let skip = prefs.skip(&ui);
        let from = self.translate_offer(chat_id, &skip);
        let translated = session.chat_translated_to(chat_id).is_some();
        if from.is_none() && !translated {
            return None;
        }
        Some(TranslateBarModel {
            to: choose_translate_to(from, prefs.to_language(&ui), &skip),
            from,
            translated,
            automatic,
        })
    }

    /// The "Translate to" language for a box opened in `chat_id`.
    fn translate_target(&self, chat_id: ChatId) -> &'static str {
        let ui = self.translate_ui_language();
        let prefs = &self.settings.translate.prefs;
        let skip = prefs.skip(&ui);
        let from = self.translate_offer(chat_id, &skip);
        choose_translate_to(from, prefs.to_language(&ui), &skip)
    }

    fn new_translate_dialog(
        &mut self,
        view: TranslateView,
        source: Option<TranslateSource>,
        to: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Search")
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        let subscription = cx.subscribe_in(
            &search,
            window,
            |_this, _state, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        );
        self.settings.translate.dialog = Some(TranslateDialog {
            view,
            source,
            to,
            job: None,
            original_expanded: false,
            close_after_choose: false,
            note: None,
            search,
            _search_subscription: subscription,
        });
    }

    // ---------------------------------------------------------------------
    // Opening
    // ---------------------------------------------------------------------

    /// Message menu → Translate: the whole message.
    pub(super) fn open_translate_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let content = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0))
            .and_then(|h| h.messages.get(&message_id.0))
            .and_then(|m| {
                translatable_content(&m.content)
                    .map(|(text, entities)| (text.to_string(), entities.to_vec()))
            });
        let Some((text, entities)) = content else {
            return;
        };
        let to = self.translate_target(chat_id);
        self.new_translate_dialog(
            TranslateView::Box,
            Some(TranslateSource::Message {
                chat_id,
                message_id,
                text,
                entities,
            }),
            to,
            window,
            cx,
        );
        self.request_dialog_translation(cx);
        cx.notify();
    }

    /// Message menu → Translate Selected Text.
    pub(super) fn open_translate_selection(
        &mut self,
        chat_id: ChatId,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.translate_target(chat_id);
        self.new_translate_dialog(
            TranslateView::Box,
            Some(TranslateSource::Selection { chat_id, text }),
            to,
            window,
            cx,
        );
        self.request_dialog_translation(cx);
        cx.notify();
    }

    /// Settings → Do Not Translate.
    pub(super) fn open_translate_skip_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let to = self
            .settings
            .translate
            .prefs
            .to_language(&self.translate_ui_language());
        self.new_translate_dialog(TranslateView::SkipList, None, to, window, cx);
        cx.notify();
    }

    /// Bar menu → Translate To.
    fn open_translate_chooser(
        &mut self,
        to: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_translate_dialog(TranslateView::ChooseTo, None, to, window, cx);
        if let Some(dialog) = self.settings.translate.dialog.as_mut() {
            dialog.close_after_choose = true;
        }
        cx.notify();
    }

    pub(super) fn close_translate_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(job) = self.settings.translate.dialog.as_ref().and_then(|d| d.job)
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.drop_text_translation(job);
        }
        self.settings.translate.dialog = None;
        cx.notify();
    }

    /// Ask TDLib to translate what the box shows into its language.
    fn request_dialog_translation(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.settings.translate.dialog.as_mut() else {
            return;
        };
        let to = dialog.to;
        match dialog.source.clone() {
            Some(TranslateSource::Message {
                chat_id,
                message_id,
                ..
            }) => {
                if let Some(live) = self.live.as_mut()
                    && let Err(err) = live.driver.translate_message(chat_id, message_id, to)
                {
                    self.connection.status_note = format!("Translate failed: {err:?}");
                }
            }
            Some(TranslateSource::Selection { text, .. }) => {
                if let Some(live) = self.live.as_mut() {
                    if let Some(old) = dialog.job.take() {
                        live.driver.session.drop_text_translation(old);
                    }
                    match live.driver.translate_selection(&text, to) {
                        Ok(job) => dialog.job = Some(job),
                        Err(err) => {
                            self.connection.status_note = format!("Translate failed: {err:?}")
                        }
                    }
                }
            }
            None => {}
        }
        cx.notify();
    }

    /// A language was picked in the chooser.
    fn choose_translate_language(&mut self, code: &'static str, cx: &mut Context<Self>) {
        self.set_translate_prefs(cx, |prefs| prefs.translate_to = code.to_string());
        let Some(dialog) = self.settings.translate.dialog.as_mut() else {
            return;
        };
        dialog.to = code;
        if dialog.close_after_choose {
            self.settings.translate.dialog = None;
        } else {
            dialog.view = TranslateView::Box;
            self.request_dialog_translation(cx);
        }
        cx.notify();
    }

    fn toggle_skip_language(&mut self, code: &'static str, cx: &mut Context<Self>) {
        let ui = self.translate_ui_language();
        let mut accepted = true;
        self.set_translate_prefs(cx, |prefs| accepted = prefs.toggle_skip(code, &ui));
        if let Some(dialog) = self.settings.translate.dialog.as_mut() {
            dialog.note = (!accepted).then(|| {
                "Please choose at least one language so that it can be used as the \"Translate to\" language."
                    .to_string()
            });
        }
        cx.notify();
    }

    // ---------------------------------------------------------------------
    // Dialog
    // ---------------------------------------------------------------------

    pub(super) fn build_translate_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::Translate, |this, _, cx| {
                this.close_translate_dialog(cx);
            });
        app.update(cx, |this, cx| {
            let Some(view) = this.settings.translate.dialog.as_ref().map(|d| d.view) else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Translate"))
                    .on_close(on_close);
            };
            let (title, body, footer) = match view {
                TranslateView::Box => (
                    "Translate",
                    this.translate_box_body(cx),
                    this.translate_box_footer(cx),
                ),
                TranslateView::ChooseTo => (
                    "Language",
                    this.translate_language_list(false, cx),
                    this.translate_back_footer("Cancel", cx),
                ),
                TranslateView::SkipList => (
                    "Do Not Translate",
                    this.translate_language_list(true, cx),
                    this.translate_back_footer("Done", cx),
                ),
            };
            dialog
                .overlay(true)
                .width(px(480.))
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::scrollable_dialog_content({
                    // `content` needs an `Fn` closure, but the body is built once
                    // per dialog render — hand it over through a one-shot cell.
                    let body = Rc::new(RefCell::new(Some(body)));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    fn translate_back_footer(&self, label: &'static str, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .justify_end()
            .child(
                Button::new("translate-footer-close")
                    .label(label)
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        let back_to_box =
                            this.settings.translate.dialog.as_ref().is_some_and(|d| {
                                d.view == TranslateView::ChooseTo && d.source.is_some()
                            });
                        if back_to_box {
                            if let Some(dialog) = this.settings.translate.dialog.as_mut() {
                                dialog.view = TranslateView::Box;
                            }
                            cx.notify();
                        } else {
                            this.close_translate_dialog(cx);
                            this.close_kit_dialog_if_done(DialogKind::Translate, window, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    fn translate_box_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .justify_between()
            .child(
                Button::new("translate-language")
                    .label("Language")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(dialog) = this.settings.translate.dialog.as_mut() {
                            dialog.view = TranslateView::ChooseTo;
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("translate-ok")
                    .label("OK")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_translate_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::Translate, window, cx);
                    })),
            )
            .into_any_element()
    }

    /// The translation the box shows right now.
    fn dialog_translation(&self) -> Option<Translation> {
        let dialog = self.settings.translate.dialog.as_ref()?;
        let session = self.session()?;
        match dialog.source.as_ref()? {
            TranslateSource::Message {
                chat_id,
                message_id,
                ..
            } => session
                .message_translation(*chat_id, *message_id, dialog.to)
                .cloned(),
            TranslateSource::Selection { .. } => dialog
                .job
                .and_then(|job| session.text_translation(job))
                .cloned(),
        }
    }

    fn translate_box_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.settings.translate.dialog.as_ref() else {
            return div().into_any_element();
        };
        let (chat_id, original, entities): (ChatId, String, Vec<TextEntity>) =
            match dialog.source.as_ref() {
                Some(TranslateSource::Message {
                    chat_id,
                    text,
                    entities,
                    ..
                }) => (*chat_id, text.clone(), entities.clone()),
                Some(TranslateSource::Selection { chat_id, text }) => {
                    (*chat_id, text.clone(), Vec::new())
                }
                None => return div().into_any_element(),
            };
        let font = self.msg_font();
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let copy_blocked = self
            .session()
            .is_some_and(|s| s.chat_has_protected_content(chat_id));
        let long = original.chars().count() > LONG_ORIGINAL || original.contains('\n');
        let collapsed = long && !dialog.original_expanded;
        let seen: HashSet<(i64, u64, u64, bool)> = HashSet::new();
        let no_emoji = HashMap::new();

        let mut original_box =
            div()
                .id("translate-original")
                .w_full()
                .min_w_0()
                .child(rich_text_line(
                    &original,
                    &entities,
                    (0, 1),
                    false,
                    &seen,
                    font,
                    &no_emoji,
                    cx,
                ));
        if collapsed {
            original_box = original_box
                .max_h(font * (ORIGINAL_LINES * 1.5))
                .overflow_hidden();
        }
        let original_row = div()
            .flex()
            .items_start()
            .gap_2()
            .child(div().flex_1().min_w_0().child(original_box))
            .when(collapsed, |row| {
                row.child(
                    Button::new("translate-show-original")
                        .label("Show")
                        .ghost()
                        .small()
                        .text_color(accent())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(dialog) = this.settings.translate.dialog.as_mut() {
                                dialog.original_expanded = true;
                            }
                            cx.notify();
                        })),
                )
            });

        let translation = self.dialog_translation();
        let copy_text = match &translation {
            Some(Translation::Done { text, .. }) if !copy_blocked => Some(text.clone()),
            _ => None,
        };
        let heading = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .font_semibold()
                    .text_sm()
                    .text_color(accent())
                    .child(language_name(dialog.to)),
            )
            .when_some(copy_text, |row, text| {
                row.child(
                    Button::new("translate-copy")
                        .icon(gpui_kit::assets::IconName::Copy)
                        .ghost()
                        .small()
                        .accessibility_label("Copy translation")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            this.connection.status_note = "translation copied".into();
                            cx.notify();
                        })),
                )
            });
        let result: AnyElement = match translation {
            Some(Translation::Done { text, entities }) => div()
                .id("translate-result")
                .w_full()
                .min_w_0()
                .child(rich_text_line(
                    &text,
                    &entities,
                    (0, 2),
                    false,
                    &seen,
                    font,
                    &no_emoji,
                    cx,
                ))
                .into_any_element(),
            Some(Translation::Failed(_)) => div()
                .id("translate-failed")
                .italic()
                .text_sm()
                .text_color(muted)
                .child("Translate failed.")
                .into_any_element(),
            // Pending, or the request could not be sent (demo, offline):
            // tdesktop's placeholder is a few grey lines.
            Some(Translation::Pending) | None => {
                let lines = (original.chars().count() / 40 + 1).clamp(1, 3);
                div()
                    .id("translate-loading")
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .children((0..lines).map(|line| {
                        div()
                            .h(px(10.))
                            .w(relative(if line + 1 == lines && lines > 1 {
                                0.55
                            } else {
                                1.
                            }))
                            .rounded_md()
                            .bg(border)
                    }))
                    .into_any_element()
            }
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(original_row)
            .child(div().h(px(1.)).w_full().bg(border))
            .child(heading)
            .child(result)
            .into_any_element()
    }

    /// The searchable language list: one pick (the "Translate to" language)
    /// or several (the "Do Not Translate" languages).
    fn translate_language_list(&self, multi: bool, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.settings.translate.dialog.as_ref() else {
            return div().into_any_element();
        };
        let query = dialog.search.read(cx).value().to_string();
        let ui = self.translate_ui_language();
        let skip = self.settings.translate.prefs.skip(&ui);
        let current = dialog.to;
        let theme = cx.theme();
        let (row_hover, border) = (theme.accent, theme.border);
        let rows = search_languages(&query)
            .into_iter()
            .map(|(code, english, native)| {
                let selected = if multi {
                    skip.contains(&code)
                } else {
                    code == current
                };
                div()
                    .id(SharedString::from(format!("translate-lang-{code}")))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |style| style.bg(row_hover))
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!(
                        "{english}{}",
                        if selected { ", selected" } else { "" }
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if multi {
                            this.toggle_skip_language(code, cx);
                        } else {
                            this.choose_translate_language(code, cx);
                        }
                    }))
                    .child(div().w(px(18.)).flex_none().when(selected, |slot| {
                        slot.child(
                            Icon::new(gpui_kit::assets::IconName::Check)
                                .size(px(16.))
                                .text_color(accent()),
                        )
                    }))
                    .child(div().flex_1().min_w_0().text_sm().child(english))
                    .when(native != english, |row| {
                        row.child(div().text_xs().text_color(muted_fg(cx)).child(native))
                    })
            });
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Textarea::new(&dialog.search)
                    .aria_label("Search languages")
                    .h(px(40.)),
            )
            .when_some(dialog.note.clone(), |column, note| {
                column.child(div().text_xs().text_color(danger_bright()).child(note))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(border)
                    .pt_1()
                    .children(rows),
            )
            .into_any_element()
    }

    // ---------------------------------------------------------------------
    // Message menu
    // ---------------------------------------------------------------------

    /// Whether the message menu offers Translate for `text` here.
    pub(super) fn translate_menu_offered(&self, chat_id: ChatId, text: &str) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let secret = session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        !secret
            && !quill::translate::skip_translate(
                text,
                &self.settings.translate.prefs,
                &self.translate_ui_language(),
            )
    }

    // ---------------------------------------------------------------------
    // Bar
    // ---------------------------------------------------------------------

    /// The translate bar (and its toast) for the open chat.
    pub(super) fn translate_bar_views(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut views = Vec::new();
        if let Some(bar) = self.translate_bar_view(chat_id, cx) {
            views.push(bar);
        }
        if let Some(toast) = self.translate_toast_view(chat_id, cx) {
            views.push(toast);
        }
        views
    }

    fn translate_bar_view(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let model = self.translate_bar_model(chat_id)?;
        let label = bar_label_for(model.translated, model.to, model.automatic, model.from);
        let to = model.to;
        let from = model.from;
        let owner = cx.entity().downgrade();
        let theme = cx.theme();
        let border = theme.border;
        let menu = Button::new("translate-bar-menu")
            .icon(gpui_kit::assets::IconName::EllipsisVertical)
            .ghost()
            .small()
            .accessibility_label("Translation options")
            .on_click(|event, window, cx| {
                if matches!(event, ClickEvent::Keyboard(_)) {
                    window.dispatch_action(
                        Box::new(gpui_kit::base::actions::Confirm { secondary: false }),
                        cx,
                    );
                }
            })
            .dropdown_menu(move |mut menu, _, _| {
                let chooser = owner.clone();
                menu = menu
                    .item(
                        PopupMenuItem::new(format!("Translate To: {}", language_name(to)))
                            .on_click(move |_, window, cx| {
                                let _ = chooser.update(cx, |this, cx| {
                                    this.open_translate_chooser(to, window, cx);
                                });
                            }),
                    )
                    .separator();
                if let Some(from) = from {
                    let skip = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("Don't translate {}", language_name(from)))
                            .on_click(move |_, _, cx| {
                                let _ = skip.update(cx, |this, cx| {
                                    this.skip_translating(chat_id, from, cx);
                                });
                            }),
                    );
                }
                let hide = owner.clone();
                menu.item(PopupMenuItem::new("Hide").on_click(move |_, _, cx| {
                    let _ = hide.update(cx, |this, cx| this.hide_translate_bar(chat_id, cx));
                }))
            });
        Some(
            div()
                .id("translate-bar")
                .flex()
                .flex_none()
                .items_center()
                .min_h(px(40.))
                .border_b_1()
                .border_color(border)
                .bg(bg_canvas())
                .child(
                    div()
                        .id("translate-bar-toggle")
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .cursor_pointer()
                        .role(gpui_kit::Role::Button)
                        .aria_label(label.clone())
                        .tab_index(0)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_chat_translation(chat_id, cx);
                        }))
                        .child(
                            Icon::new(gpui_kit::assets::IconName::Languages)
                                .size(px(16.))
                                .text_color(accent()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(accent())
                                .truncate()
                                .child(label),
                        ),
                )
                .child(div().flex_none().px_1().child(menu))
                .into_any_element(),
        )
    }

    fn translate_toast_view(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let toast = self
            .settings
            .translate
            .toast
            .as_ref()
            .filter(|toast| toast.chat_id == chat_id)?
            .clone();
        let action = toast.action;
        Some(
            div()
                .id("translate-toast")
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .min_h(px(40.))
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .child(div().flex_1().min_w_0().text_sm().child(toast.text.clone()))
                .child(
                    Button::new("translate-toast-action")
                        .label(toast.label)
                        .ghost()
                        .small()
                        .text_color(accent())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.settings.translate.toast = None;
                            match action {
                                ToastAction::ShowBar => {
                                    this.set_translate_bar_hidden(chat_id, false, cx)
                                }
                                ToastAction::OpenSkipList => {
                                    this.open_translate_skip_list(window, cx);
                                }
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    fn show_translate_toast(
        &mut self,
        chat_id: ChatId,
        text: String,
        label: &'static str,
        action: ToastAction,
        cx: &mut Context<Self>,
    ) {
        self.settings.translate.toast_seq += 1;
        let id = self.settings.translate.toast_seq;
        self.settings.translate.toast = Some(TranslateToast {
            chat_id,
            text,
            label,
            action,
            id,
        });
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOAST_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .settings
                    .translate
                    .toast
                    .as_ref()
                    .is_some_and(|t| t.id == id)
                {
                    this.settings.translate.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Click on the bar: translate the chat, or show it in the original.
    fn toggle_chat_translation(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let Some(model) = self.translate_bar_model(chat_id) else {
            return;
        };
        let to = (!model.translated).then_some(model.to);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.set_chat_translated_to(chat_id, to);
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.set_chat_translated_to(chat_id, to);
        }
        cx.notify();
    }

    /// Bar menu → Don't translate {language}.
    fn skip_translating(
        &mut self,
        chat_id: ChatId,
        language: &'static str,
        cx: &mut Context<Self>,
    ) {
        let ui = self.translate_ui_language();
        self.set_translate_prefs(cx, |prefs| prefs.add_skip(language, &ui));
        self.show_translate_toast(
            chat_id,
            format!(
                "{} added to the Do Not Translate list.",
                language_name(language)
            ),
            "Settings",
            ToastAction::OpenSkipList,
            cx,
        );
    }

    /// Hide or show the chat's translate bar. A live account sends
    /// `toggleChatIsTranslatable` (the server flag, as in tdesktop); the
    /// demo keeps the choice locally.
    fn set_translate_bar_hidden(&mut self, chat_id: ChatId, hidden: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live
                .driver
                .toggle_chat_is_translatable(chat_id, !hidden)
                .is_ok()
        {
            self.set_translate_prefs(cx, |p| p.set_bar_hidden(chat_id.0, false));
            return;
        }
        self.set_translate_prefs(cx, |p| p.set_bar_hidden(chat_id.0, hidden));
    }

    /// Bar menu → Hide.
    fn hide_translate_bar(&mut self, chat_id: ChatId, cx: &mut Context<Self>) {
        let kind = self
            .session()
            .and_then(|s| s.chats.get(&chat_id.0))
            .map(|chat| chat.kind.clone());
        let phrase = match kind {
            Some(ChatKind::Supergroup {
                is_channel: true, ..
            }) => "Translation bar is now hidden for this channel.",
            Some(ChatKind::Private { .. } | ChatKind::Secret { .. }) | None => {
                "Translation bar hidden for this chat."
            }
            _ => "Translation bar is now hidden for this group.",
        };
        self.set_translate_bar_hidden(chat_id, true, cx);
        self.show_translate_toast(
            chat_id,
            phrase.to_string(),
            "Undo",
            ToastAction::ShowBar,
            cx,
        );
    }

    // ---------------------------------------------------------------------
    // Translated chat
    // ---------------------------------------------------------------------

    /// Show the open chat's translated messages in place of the originals.
    pub(super) fn apply_chat_translation(&self, chat_id: ChatId, messages: &mut [HistoryMessage]) {
        let Some(session) = self.session() else {
            return;
        };
        let Some(to) = session.chat_translated_to(chat_id) else {
            return;
        };
        let automatic = session.chat_auto_translate(chat_id);
        for message in messages.iter_mut().filter(|m| automatic || !m.is_outgoing) {
            if let Some(Translation::Done { text, entities }) =
                session.message_translation(chat_id, message.id, to)
            {
                replace_content_text(&mut message.content, text, entities);
            }
        }
    }

    /// The revision the history rows' cache must include.
    pub(super) fn translate_revision(&self) -> u64 {
        self.session().map_or(0, |s| s.messages.translate.revision)
    }

    /// Per-tick work: keep the open chat's translation going (request what
    /// is missing, follow a changed target language, stop when the bar is
    /// no longer offered). Returns whether anything changed.
    pub(super) fn pump_translation(&mut self) -> bool {
        let Some(chat_id) = self.session().and_then(|s| s.open_chat) else {
            return false;
        };
        let Some(current) = self
            .session()
            .and_then(|s| s.chat_translated_to(chat_id).map(str::to_string))
        else {
            return false;
        };
        let Some(model) = self.translate_bar_model(chat_id) else {
            if let Some(live) = self.live.as_mut() {
                live.driver.session.set_chat_translated_to(chat_id, None);
            }
            return true;
        };
        if current != model.to {
            if let Some(live) = self.live.as_mut() {
                live.driver
                    .session
                    .set_chat_translated_to(chat_id, Some(model.to));
            }
            return true;
        }
        let ui = self.translate_ui_language();
        let skip = self.settings.translate.prefs.skip(&ui);
        let wanted: Vec<MessageId> = {
            let Some(session) = self.session() else {
                return false;
            };
            let Some(history) = session.histories.get(&chat_id.0) else {
                return false;
            };
            let room = MAX_IN_FLIGHT.saturating_sub(session.messages.translate.jobs.len());
            history
                .ordered()
                .into_iter()
                .rev()
                .filter(|m| (model.automatic || !m.is_outgoing) && !m.pending && m.id.0 > 0)
                .filter(|m| {
                    translatable_content(&m.content).is_some_and(|(text, _)| {
                        detect_language(text)
                            .is_none_or(|lang| lang != model.to && !skip.contains(&lang))
                            && text.chars().any(char::is_alphabetic)
                    })
                })
                .filter(|m| session.needs_translation(chat_id, m.id, model.to))
                .take(room)
                .map(|m| m.id)
                .collect()
        };
        let mut sent = false;
        if let Some(live) = self.live.as_mut() {
            for id in wanted {
                sent |= live.driver.translate_message(chat_id, id, model.to).is_ok();
            }
        }
        sent
    }

    // ---------------------------------------------------------------------
    // Screenshot demo
    // ---------------------------------------------------------------------

    // ---------------------------------------------------------------------
    // Settings
    // ---------------------------------------------------------------------

    /// Settings → Translation: Show Translate Button, Translate Entire Chats
    /// (Premium), Do Not Translate.
    pub(super) fn translate_settings_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = &self.settings.translate.prefs;
        let premium = self.session().is_some_and(Session::is_premium);
        let ui = self.translate_ui_language();
        let skip = prefs.skip(&ui);
        let skip_label = if skip.len() > 1 {
            format!("{} languages", skip.len())
        } else {
            skip.first()
                .map(|code| language_name(code))
                .unwrap_or_default()
        };
        let switch_row = |title: &'static str, hint: SharedString, control: AnyElement| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_sm().child(title))
                        .child(muted_text(hint, cx)),
                )
                .child(control)
        };
        let show = switch_row(
            "Show Translate Button",
            "The \"Translate\" button will appear in the context menu of messages containing text."
                .into(),
            Switch::new("translate-show-button")
                .checked(prefs.show_button)
                .accessibility_label("Show Translate Button")
                .on_click(cx.listener(|this, &on, _, cx| {
                    this.set_translate_prefs(cx, |p| p.show_button = on);
                }))
                .into_any_element(),
        );
        let chats = switch_row(
            "Translate Entire Chats",
            if premium {
                "Offer to translate whole conversations with a bar above the chat.".into()
            } else {
                "Translating entire chats requires Telegram Premium.".into()
            },
            Switch::new("translate-chats")
                .checked(prefs.translate_chats && premium)
                .disabled(!premium)
                .accessibility_label("Translate Entire Chats")
                .on_click(cx.listener(|this, &on, _, cx| {
                    if !this.session().is_some_and(Session::is_premium) {
                        this.connection.status_note =
                            "Translating entire chats requires Telegram Premium".into();
                        cx.notify();
                        return;
                    }
                    this.set_translate_prefs(cx, |p| p.translate_chats = on);
                }))
                .into_any_element(),
        );
        let skip_row = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(div().text_sm().child("Do Not Translate"))
            .child(
                Button::new("translate-skip-languages")
                    .label(skip_label)
                    .ghost()
                    .small()
                    .accessibility_label("Choose languages not to translate")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_translate_skip_list(window, cx);
                    })),
            );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().font_semibold().text_sm().child("Translate Messages"))
            .child(show)
            .child(chats)
            .when(prefs.show_button || prefs.translate_chats, |column| {
                column.child(skip_row)
            })
            .into_any_element()
    }
}

fn muted_fg(cx: &App) -> Hsla {
    cx.theme().muted_foreground
}

crate::ui::shell::register_dialogs! {
    /// Batch 7: the translate box and its language choosers.
    Translate => DialogSpec::new(
        // Opened from the Appearance dialog's translation options: it
        // takes over and Appearance returns when it closes.
        6500,
        |app| app.settings.translate.dialog.is_some(),
        QuillApp::build_translate_dialog,
    ),
}

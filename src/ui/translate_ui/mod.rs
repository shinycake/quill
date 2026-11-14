//! Message translation: the translate box (original, target language, the
//! translated text with entities, copy), the language chooser, the chat
//! translate bar with its menu, translated bubbles, and the translation
//! options in Settings. Modelled on Telegram Desktop's `boxes/translate_box`,
//! `history_view_translate_bar` and `history_view_translate_tracker`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::message_text::rich_text_line;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId, RequestId};
use quill::lang::{keys, tr, tr_with};
use quill::state::{HistoryMessage, Session, Translation};
use quill::telegram::envelope::ChatKind;
use quill::text::TextEntity;
use quill::translate::{
    TranslatePrefs, bar_label_for, choose_translate_to, detect_language, language_name,
    offer_language, replace_content_text, search_languages, tracking_enabled, translatable_content,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use dialog_state::{TranslateAsk, TranslateBox, TranslateDialog, TranslateStage};

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

/// A line under the bar with one action ("Undo", "Settings").
pub(super) struct TranslateToast {
    chat_id: ChatId,
    text: String,
    label: &'static str,
    action: ToastAction,
}

/// The Hide the bar sent and has not heard an answer to: the request
/// outlives the toast, so a refusal after the toast went still says so, and
/// an unrelated change of the flag is not taken for one.
#[derive(Default)]
pub(super) enum HideStage {
    #[default]
    Idle,
    /// `toggleChatIsTranslatable` is out: its answer (an error, a lost
    /// request) ends the stage.
    Sent(quill::ids::RequestId),
}

/// The toast under the bar: shown with the timer that hides it, or not
/// there. Replacing or clearing the stage drops the timer, so a newer
/// toast restarts it and a timer never clears a later toast.
#[derive(Default)]
pub(super) enum ToastStage {
    #[default]
    None,
    Shown {
        toast: TranslateToast,
        _hide: Task<()>,
    },
}

impl ToastStage {
    fn toast(&self) -> Option<&TranslateToast> {
        match self {
            Self::None => None,
            Self::Shown { toast, .. } => Some(toast),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ToastAction {
    ShowBar,
    OpenSkipList,
}

/// Chats whose offered language is cached at once.
const OFFER_CACHE_CHATS: usize = 32;

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

/// The app's translation prefs (`translate_prefs.json`), the same for
/// every account.
pub(super) struct TranslateUi {
    pub(super) prefs: TranslatePrefs,
}

impl TranslateUi {
    pub(super) fn load() -> Self {
        Self {
            prefs: quill::settings::load_translate_prefs(&QuillApp::appearance_paths()),
        }
    }
}

/// What translating shows for the account on screen: the dialog (with
/// the message it translates), the bar's toast, the offer cache and the
/// pump's scan, all keyed by that account's chats and messages. The screen
/// owns it, so an account switch drops it with the rest.
#[derive(Default)]
pub(super) struct TranslateScreen {
    pub(super) dialog: Option<TranslateDialog>,
    toast: ToastStage,
    /// The Hide whose answer is awaited (`end_refused_hide`).
    hide: HideStage,
    /// The offered language per chat: each window shows its own chat, and
    /// one slot thrashed when two showed different ones.
    offer: RefCell<HashMap<i64, (OfferKey, Option<&'static str>)>>,
    /// What `pump_translation` tested last (`pump_scan`), by chat: each
    /// window pumps the chat it shows.
    pub(super) pump: std::collections::HashMap<i64, pump_scan::PumpScan>,
    #[cfg(test)]
    asked: Vec<(MessageId, &'static str)>,
}

/// The dialog's `translateText` job is dropped from the old client's
/// session (it counts toward the pump's in-flight cap and keeps its result),
/// and a Hide still out is forgotten with it.
impl super::on_screen::Farewell for TranslateScreen {
    fn farewell(app: &mut QuillApp, _: &mut Window, cx: &mut Context<QuillApp>) {
        app.close_translate_dialog(cx);
        // A Hide still out is not awaited by the next account: its answer
        // would stay in this client's acks with no owner.
        app.forget_hide();
    }
}

impl QuillApp {
    // ---------------------------------------------------------------------
    // State helpers
    // ---------------------------------------------------------------------

    /// The app language tag ("Translate to" and "Do Not Translate" default
    /// to it).
    pub(in crate::ui) fn translate_ui_language(&self) -> String {
        let saved = self
            .session()
            .map(|s| s.settings.language_prefs.system_language_code.clone())
            .unwrap_or_else(|| quill::settings::DEFAULT_LANGUAGE_CODE.to_string());
        let pack = quill::lang::current();
        quill::translate::interface_language(pack.as_deref().map(|p| p.plural_code()), &saved)
    }

    fn set_translate_prefs(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut TranslatePrefs)) {
        f(&mut self.preferences.translate.prefs);
        quill::settings::save_translate_prefs(
            &Self::appearance_paths(),
            &self.preferences.translate.prefs,
        );
        quill::wake::UPDATES.poke();
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
        if let Ok(offers) = self.translate.offer.try_borrow()
            && let Some((cached, value)) = offers.get(&chat_id.0)
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
        // A re-entrant call skips caching: the value is recomputed next time.
        if let Ok(mut offers) = self.translate.offer.try_borrow_mut() {
            // A chat per window is a handful; a long session of chats is not
            // kept whole.
            if offers.len() >= OFFER_CACHE_CHATS {
                offers.clear();
            }
            offers.insert(chat_id.0, (key, value));
        }
        value
    }

    /// The chat's translate bar, or `None` when it is not offered: the
    /// setting is off, the account has no Premium, TDLib does not suggest
    /// translating the chat, the user hid the bar, or the recent messages
    /// are not in a language to translate.
    pub(super) fn translate_bar_model(&self, chat_id: ChatId) -> Option<TranslateBarModel> {
        let session = self.session()?;
        let prefs = &self.preferences.translate.prefs;
        let automatic = session.chat_auto_translate(chat_id);
        if !tracking_enabled(prefs.translate_chats, session.is_premium(), automatic)
            || !session.chat_is_translatable(chat_id)
            || prefs.bar_hidden(&session.account, chat_id.0)
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

    /// tdesktop `Filler::addTranslate`: the header menu's Translate, offered
    /// when the user hid the bar of a chat that would otherwise show it.
    pub(in crate::ui) fn chat_translate_menu_offered(&self, chat_id: ChatId) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let prefs = &self.preferences.translate.prefs;
        let hidden =
            !session.chat_is_translatable(chat_id) || prefs.bar_hidden(&session.account, chat_id.0);
        let ui = self.translate_ui_language();
        hidden
            && prefs.translate_chats
            && session.is_premium()
            && session.chat_translated_to(chat_id).is_none()
            && session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| !matches!(chat.kind, ChatKind::Secret { .. }))
            && self.translate_offer(chat_id, &prefs.skip(&ui)).is_some()
    }

    /// The "Translate to" language for a box opened in `chat_id`.
    fn translate_target(&self, chat_id: ChatId) -> &'static str {
        let ui = self.translate_ui_language();
        let prefs = &self.preferences.translate.prefs;
        let skip = prefs.skip(&ui);
        let from = self.translate_offer(chat_id, &skip);
        choose_translate_to(from, prefs.to_language(&ui), &skip)
    }

    fn new_translate_dialog(
        &mut self,
        stage: TranslateStage,
        to: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The dialog is the screen's one: opening another (from another
        // window) releases the old box's selection job.
        self.release_dialog_job();
        let search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH))
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
        self.translate.dialog = Some(TranslateDialog {
            stage,
            to,
            search,
            _search_subscription: subscription,
        });
        self.open_box(DialogKind::Translate);
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
            TranslateStage::Box(TranslateBox::new(TranslateSource::Message {
                chat_id,
                message_id,
                text,
                entities,
            })),
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
            TranslateStage::Box(TranslateBox::new(TranslateSource::Selection {
                chat_id,
                text,
            })),
            to,
            window,
            cx,
        );
        self.request_dialog_translation(cx);
        cx.notify();
    }

    /// The name of the language the open box translates to.
    fn translate_to_label(&self) -> String {
        self.translate
            .dialog
            .as_ref()
            .map(|dialog| quill::translate::language_native_name(dialog.to))
            .unwrap_or_default()
    }

    /// Settings → Translate To.
    pub(super) fn open_translate_to_setting(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self
            .preferences
            .translate
            .prefs
            .to_language(&self.translate_ui_language());
        self.open_translate_chooser(to, window, cx);
    }

    /// Settings → Do Not Translate.
    pub(super) fn open_translate_skip_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let to = self
            .preferences
            .translate
            .prefs
            .to_language(&self.translate_ui_language());
        self.new_translate_dialog(TranslateStage::SkipList { note: None }, to, window, cx);
        cx.notify();
    }

    /// Bar menu → Translate To.
    fn open_translate_chooser(
        &mut self,
        to: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A chooser with no box under it (no source) closes after a pick.
        self.new_translate_dialog(TranslateStage::Choosing { under: None }, to, window, cx);
        cx.notify();
    }

    /// Drop the open box's selection job from the session, so it stops
    /// counting toward the pump's in-flight cap.
    fn release_dialog_job(&mut self) {
        if let Some(job) = self.translate.dialog.as_ref().and_then(|d| d.job())
            && let Some(live) = self.live.as_mut()
        {
            live.driver.session.drop_text_translation(job);
        }
    }

    pub(super) fn close_translate_dialog(&mut self, cx: &mut Context<Self>) {
        self.release_dialog_job();
        self.translate.dialog = None;
        cx.notify();
    }

    /// Ask TDLib to translate what the box shows into its language.
    fn request_dialog_translation(&mut self, cx: &mut Context<Self>) {
        // The screen's fields by name: the dialog stays borrowed beside
        // `live` and the note.
        let Some(dialog) = self.screen.translate.dialog.as_mut() else {
            return;
        };
        let to = dialog.to;
        let Some(shown) = dialog.current_box_mut() else {
            return;
        };
        match shown.source.clone() {
            TranslateSource::Message {
                chat_id,
                message_id,
                ..
            } => {
                shown.ask = TranslateAsk::Idle;
                if let Some(live) = self.live.as_mut()
                    && let Err(err) = live.driver.translate_message(chat_id, message_id, to)
                {
                    shown.ask = TranslateAsk::Refused;
                    self.set_note(StatusNote::error(user_error_for(
                        keys::Q_NOTES_FAILED_TRANSLATE,
                        &err,
                    )));
                }
            }
            TranslateSource::Selection { text, .. } => {
                if let Some(live) = self.live.as_mut() {
                    if let TranslateAsk::Job(old) = shown.ask {
                        live.driver.session.drop_text_translation(old);
                    }
                    match live.driver.translate_selection(&text, to) {
                        Ok(job) => shown.ask = TranslateAsk::Job(job),
                        Err(err) => {
                            shown.ask = TranslateAsk::Refused;
                            self.set_note(StatusNote::error(user_error_for(
                                keys::Q_NOTES_FAILED_TRANSLATE,
                                &err,
                            )));
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// A language was picked in the chooser.
    fn choose_translate_language(&mut self, code: &'static str, cx: &mut Context<Self>) {
        self.set_translate_prefs(cx, |prefs| prefs.translate_to = code.to_string());
        let Some(dialog) = self.translate.dialog.as_mut() else {
            return;
        };
        if dialog.pick(code) {
            self.request_dialog_translation(cx);
        } else {
            self.translate.dialog = None;
        }
        cx.notify();
    }

    fn toggle_skip_language(&mut self, code: &'static str, cx: &mut Context<Self>) {
        let ui = self.translate_ui_language();
        let mut accepted = true;
        self.set_translate_prefs(cx, |prefs| accepted = prefs.toggle_skip(code, &ui));
        if let Some(dialog) = self.translate.dialog.as_mut() {
            dialog.set_skip_note((!accepted).then(|| tr(keys::TRANSLATE_SETTINGS_ONE).to_string()));
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
            let Some(page) = this.translate.dialog.as_ref().map(|d| match d.stage {
                TranslateStage::Box(_) => Page::Box,
                TranslateStage::Choosing { .. } => Page::ChooseTo,
                TranslateStage::SkipList { .. } => Page::SkipList,
            }) else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title(tr(keys::CONTEXT_TRANSLATE)))
                    .on_close(on_close);
            };
            let (title, body, footer) = match page {
                Page::Box => (
                    tr(keys::CONTEXT_TRANSLATE),
                    this.translate_box_body(cx),
                    this.translate_box_footer(cx),
                ),
                Page::ChooseTo => (
                    tr(keys::Q_MISC_LANGUAGE),
                    this.translate_language_list(false, cx),
                    this.translate_back_footer(tr(keys::CANCEL), cx),
                ),
                Page::SkipList => (
                    tr(keys::TRANSLATE_SETTINGS_CHOOSE),
                    this.translate_language_list(true, cx),
                    this.translate_back_footer(tr(keys::BOX_DONE), cx),
                ),
            };
            dialog
                .overlay(true)
                .width(px(480.))
                .title(crate::ui::shell::dialog_title(title))
                .content(crate::ui::shell::one_shot_dialog_content(body))
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
                        let back_to_box = this
                            .translate
                            .dialog
                            .as_mut()
                            .is_some_and(|dialog| dialog.back_to_box());
                        if back_to_box {
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
                    .label(tr_with(
                        keys::Q_REST_TRANSLATE_TO,
                        &[("language_name", &self.translate_to_label())],
                    ))
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(dialog) = this.translate.dialog.as_mut() {
                            dialog.open_chooser();
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("translate-ok")
                    .label(tr(keys::BOX_OK))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_translate_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::Translate, window, cx);
                    })),
            )
            .into_any_element()
    }

    /// The translation the box shows right now.
    fn dialog_translation(&self) -> Option<Translation> {
        let dialog = self.translate.dialog.as_ref()?;
        let session = self.session()?;
        let shown = dialog.current_box()?;
        // The request never went out: failed, not pending.
        if shown.ask == TranslateAsk::Refused {
            return Some(Translation::failed(String::new()));
        }
        match &shown.source {
            TranslateSource::Message {
                chat_id,
                message_id,
                ..
            } => session
                .message_translation(*chat_id, *message_id, dialog.to)
                .cloned(),
            TranslateSource::Selection { .. } => match shown.ask {
                TranslateAsk::Job(job) => session.text_translation(job).cloned(),
                TranslateAsk::Idle | TranslateAsk::Refused => None,
            },
        }
    }

    fn translate_box_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.translate.dialog.as_ref() else {
            return div().into_any_element();
        };
        let TranslateStage::Box(shown) = &dialog.stage else {
            return div().into_any_element();
        };
        let (chat_id, original, entities): (ChatId, String, Vec<TextEntity>) = match &shown.source {
            TranslateSource::Message {
                chat_id,
                text,
                entities,
                ..
            } => (*chat_id, text.clone(), entities.clone()),
            TranslateSource::Selection { chat_id, text } => (*chat_id, text.clone(), Vec::new()),
        };
        let font = self.msg_font();
        let theme = cx.theme();
        let (muted, border) = (theme.muted_foreground, theme.border);
        let copy_blocked = self
            .session()
            .is_some_and(|s| s.chat_has_protected_content(chat_id));
        let long = original.chars().count() > LONG_ORIGINAL || original.contains('\n');
        let collapsed = long && !shown.expanded;
        let seen = crate::ui::message_text::revealed::Revealed::default();
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
                        .label(tr(keys::Q_MISC_SHOW))
                        .ghost()
                        .small()
                        .text_color(accent())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(dialog) = this.translate.dialog.as_mut()
                                && let TranslateStage::Box(shown) = &mut dialog.stage
                            {
                                shown.expanded = true;
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
                        .accessibility_label(tr(keys::Q_MISC_COPY_TRANSLATION))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            this.set_note(StatusNote::info(tr(keys::Q_MISC_TRANSLATION_COPIED)));
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
            Some(Translation::Failed { .. }) => div()
                .id("translate-failed")
                .italic()
                .text_sm()
                .text_color(muted)
                .child(tr(keys::TRANSLATE_BOX_ERROR))
                .into_any_element(),
            // Pending, or the request could not be sent (demo, offline):
            // tdesktop's placeholder is a few grey lines.
            Some(Translation::Pending { .. }) | None => {
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
        let Some(dialog) = self.translate.dialog.as_ref() else {
            return div().into_any_element();
        };
        let query = dialog.search.read(cx).value().to_string();
        let skip_note = match &dialog.stage {
            TranslateStage::SkipList { note } => note.clone(),
            TranslateStage::Box(_) | TranslateStage::Choosing { .. } => None,
        };
        let ui = self.translate_ui_language();
        let skip = self.preferences.translate.prefs.skip(&ui);
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
                        if selected {
                            tr(keys::Q_MISC_SELECTED)
                        } else {
                            ""
                        }
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
                    .aria_label(tr(keys::Q_MISC_SEARCH_LANGUAGES))
                    .h(px(40.)),
            )
            .when_some(skip_note, |column, note| {
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
                &self.preferences.translate.prefs,
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

    // ---------------------------------------------------------------------
    // Translated chat
    // ---------------------------------------------------------------------

    // ---------------------------------------------------------------------
    // Screenshot demo
    // ---------------------------------------------------------------------

    // ---------------------------------------------------------------------
    // Settings
    // ---------------------------------------------------------------------
}

/// Which page `build_translate_dialog` draws.
#[derive(Clone, Copy)]
enum Page {
    Box,
    ChooseTo,
    SkipList,
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
        |app| app.translate.dialog.is_some(),
        QuillApp::build_translate_dialog,
    )
    .close(|app, _, cx| app.close_translate_dialog(cx)),
}

mod demo;
mod dialog_state;
mod pump_scan;
mod translate_bar_view;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "pump_tests.rs"]
mod pump_tests;

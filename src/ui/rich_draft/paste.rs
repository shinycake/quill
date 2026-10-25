//! The rich paste notice (tdesktop `chat_helpers/rich_paste_toast.cpp`,
//! `HistoryWidget::offerRichPaste`): text pasted into the field with
//! headings, dividers or task lists lands as plain text, and a notice
//! offers the rich editor, or Premium to a user without it. GPUI reads no
//! HTML from the clipboard, so only Markdown text is recognized.

use crate::ui::ai_compose::marked_link;
use crate::ui::app::QuillApp;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::lang::{keys, tr};
use quill::premium_hub::PremiumFeature;
use quill::state::Session;
use std::time::{Duration, Instant};

/// tdesktop `kToastDuration`.
const DURATION: Duration = Duration::from_secs(9);
/// tdesktop `kOfferTimeout` (`RichPasteOfferThrottle`).
const THROTTLE: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct PasteNotice {
    shown_at: Option<Instant>,
    /// The field's text when the notice showed; any other text closes it.
    field: Option<String>,
}

/// Identifies the notice so it can be closed.
struct RichPasteToast;

impl QuillApp {
    /// A paste into the field, before the field takes it.
    pub(in crate::ui) fn offer_rich_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.composer.read(cx).focus_handle(cx).is_focused(window)
            || self.composer_ui.pending_edit.is_some()
            || self.composer_ui.rich_editor_open
            || self.open_chat_id().is_none()
        {
            return;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if !quill::rich::text_has_markdown_structure(&text) {
            return;
        }
        let locked = !self.session().is_some_and(Session::my_is_premium);
        if locked && self.chat_prefs.rich_paste_toast_hidden {
            return;
        }
        let was = self.composer.read(cx).value().to_string();
        // The field takes the paste after this handler: compare once it has.
        let scope = self.window_scope_of(window);
        cx.defer_in(window, move |this, window, cx| {
            this.in_window_scope(scope, cx, |this, cx| {
                this.show_rich_paste_notice(&was, locked, window, cx);
            });
        });
    }

    /// Show the notice once the paste changed the field, at most once per
    /// [`THROTTLE`].
    pub(super) fn show_rich_paste_notice(
        &mut self,
        was: &str,
        locked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = self.composer.read(cx).value().to_string();
        let notice = &mut self.composer_ui.rich_draft.paste;
        if now == was || notice.shown_at.is_some_and(|at| at.elapsed() < THROTTLE) {
            return;
        }
        let shown_at = Instant::now();
        notice.shown_at = Some(shown_at);
        notice.field = Some(now);
        let toast = self.rich_paste_toast(locked, cx);
        window.push_notification(toast, cx);
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            cx.background_executor().timer(DURATION).await;
            let _ = this.update_in(cx, |this, window, cx| {
                let notice = &mut this.composer_ui.rich_draft.paste;
                if notice.shown_at == Some(shown_at) {
                    notice.field = None;
                    window.remove_notification::<RichPasteToast>(cx);
                }
            });
        })
        .detach();
    }

    fn rich_paste_toast(&self, locked: bool, cx: &mut Context<Self>) -> Notification {
        let toast = if locked {
            let marked = marked_link(
                tr(keys::RICH_PASTE_TOAST_PREMIUM),
                "link",
                tr(keys::RICH_PASTE_TOAST_PREMIUM_LINK),
            );
            Self::marked_toast("rich-paste-toast", marked, cx, |this, _, cx| {
                this.open_premium_at(PremiumFeature::RichMessages, cx);
            })
            .icon(IconName::Star)
        } else {
            Notification::new().message(tr(keys::RICH_PASTE_TOAST_EDITOR_MARKDOWN))
        };
        let app = cx.entity().downgrade();
        toast
            .id::<RichPasteToast>()
            .title(tr(keys::RICH_PASTE_TOAST))
            .action(move |_, _, cx| {
                let app = app.clone();
                let label = if locked {
                    tr(keys::ARCHIVE_HINT_BUTTON)
                } else {
                    tr(keys::RICH_PASTE_TOAST_OPEN)
                };
                Button::new("rich-paste-action")
                    .label(label)
                    .small()
                    .on_click(cx.listener(move |note, _, window, cx| {
                        note.dismiss(window, cx);
                        let _ = app.update(cx, |this, cx| {
                            this.composer_ui.rich_draft.paste.field = None;
                            if locked {
                                // tdesktop `rich_paste_toast_hidden`.
                                this.set_chat_prefs(cx, |prefs| {
                                    prefs.rich_paste_toast_hidden = true;
                                });
                            } else {
                                this.open_rich_editor_with_field(window, cx);
                            }
                        });
                    }))
            })
    }

    /// The field changed after the notice showed: the notice goes, as
    /// tdesktop hides it on the field's next change.
    pub(in crate::ui) fn close_stale_rich_paste_notice(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        let notice = &mut self.composer_ui.rich_draft.paste;
        if notice.field.as_deref().is_some_and(|field| field != text) {
            notice.field = None;
            window.remove_notification::<RichPasteToast>(cx);
        }
    }

    /// The field's text, as block markup, in the rich editor.
    fn open_rich_editor_with_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let markup = self.composer_markup(cx);
        self.composer_ui.rich_editor_open = true;
        self.set_composer_markup(&markup, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }
}

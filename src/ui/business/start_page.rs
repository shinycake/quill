//! The Start Page screen ("Custom Intro", tdesktop
//! `settings_chat_intro.cpp`): a preview, the title and message fields,
//! the sticker (chosen in the sticker panel, a random hello sticker when
//! none) and "Reset to Default". Saved when the screen is left.

use super::{Leave, StartPageDraft, View};
use crate::ui::app::QuillApp;
use gpui_kit::component::input::Input;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::business_start_page::StartPage;
use quill::ids::FileId;
use quill::lang::{keys, tr};
use quill::local_path::sandboxed_display_path;
use quill::state::StickerTab;
use quill::telegram::envelope::StickerItem;

fn about() -> &'static str {
    tr(keys::CHAT_INTRO_ABOUT)
}

impl QuillApp {
    pub(super) fn new_start_page_draft(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> StartPageDraft {
        let saved = self.my_business().start_page.unwrap_or_default();
        let (title_max, message_max) = self.business_start_page_limits();
        StartPageDraft {
            title: super::limited_input(
                window,
                cx,
                tr(keys::Q_OPS_ENTER_TITLE),
                &saved.title,
                title_max,
            ),
            message: super::limited_input(
                window,
                cx,
                tr(keys::Q_OPS_ENTER_MESSAGE),
                &saved.message,
                message_max,
            ),
            sticker: saved.sticker,
        }
    }

    fn business_start_page_limits(&self) -> (usize, usize) {
        self.session().map_or(
            (
                quill::business_start_page::TITLE_MAX,
                quill::business_start_page::MESSAGE_MAX,
            ),
            |s| {
                let extra = &s.settings.business_extra;
                (extra.start_page_title_max(), extra.start_page_message_max())
            },
        )
    }

    /// The sticker panel opens on the installed sets.
    pub(super) fn open_start_page_stickers(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.select_sticker_tab(StickerTab::Installed);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.stickers.tab = StickerTab::Installed;
        }
        cx.notify();
    }

    /// A sticker clicked in the panel while it chooses the start page
    /// sticker. `true` when it was taken here (it is not sent).
    pub(in crate::ui) fn route_business_start_page_sticker(
        &mut self,
        file_id: FileId,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.business_view() != Some(View::StickerPick) {
            return false;
        }
        let found = self.session().and_then(|s| {
            let panel = &s.stickers.stickers;
            [
                &panel.stickers,
                &panel.recent,
                &panel.favorites,
                &panel.found_stickers,
                &panel.suggestions,
            ]
            .into_iter()
            .flatten()
            .find(|sticker| sticker.file_id == file_id)
            .cloned()
        });
        if let Some(d) = self.business_mut() {
            if let (Some(sticker), Some(page)) = (found, d.start_page.as_mut()) {
                page.sticker = Some(sticker);
            }
            d.view = View::StartPage;
        }
        cx.notify();
        true
    }

    /// A sticker's still image (no click), or its emoji while it loads.
    pub(super) fn business_sticker_still(
        &mut self,
        sticker: &StickerItem,
        size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(file) = sticker.display_file_id()
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.ensure_media_files(&[file]);
        }
        let roots = self.media_display_roots();
        let path = [sticker.display_file_id(), sticker.thumb_file_id]
            .into_iter()
            .flatten()
            .filter_map(|id| self.session()?.media.files.get(&id.0)?.usable_path())
            .find_map(|path| sandboxed_display_path(path, &roots));
        // The settings preview, not a chat.
        let head = crate::ui::sticker_player::panel_sticker_head(sticker.file_id);
        let animated = self.sticker_image(sticker.file_id, sticker.format, head, size, cx);
        let source = animated
            .map(ImageSource::Render)
            .or_else(|| path.map(ImageSource::from));
        let label = if sticker.emoji.is_empty() {
            tr(keys::Q_OPS_STICKER).to_string()
        } else {
            sticker.emoji.clone()
        };
        match source {
            Some(source) => img(source)
                .w(px(size))
                .h(px(size))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .size(px(size))
                .flex()
                .items_center()
                .justify_center()
                .child(label)
                .into_any_element(),
        }
    }

    /// The hello sticker a start page without its own sticker shows.
    pub(super) fn business_random_greeting(&mut self, seed: i64) -> Option<StickerItem> {
        let session = self.session()?;
        if !session.stickers.stickers.greeting_loaded
            && let Some(live) = self.live.as_mut()
        {
            let _ = live.driver.fetch_greeting_stickers();
        }
        let list = &self.session()?.stickers.stickers.greeting;
        let at = (seed.unsigned_abs() as usize).checked_rem(list.len())?;
        list.get(at).cloned()
    }

    /// The intro card's title and message, as an empty chat shows them;
    /// callers add the sticker.
    pub(super) fn business_intro_texts(&self, page: &StartPage, cx: &App) -> Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .px_4()
            .py_3()
            .rounded_lg()
            .bg(cx.theme().secondary)
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_center()
                    .child(page.shown_title().to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .text_center()
                    .child(page.shown_message().to_string()),
            )
    }

    /// The settings preview: the card with the chosen sticker, or a random
    /// hello sticker (tdesktop `IntroWithRandomSticker`).
    fn business_intro_card(&mut self, page: &StartPage, seed: i64, cx: &mut Context<Self>) -> Div {
        let sticker = page
            .sticker
            .clone()
            .or_else(|| self.business_random_greeting(seed));
        let sticker = sticker.map(|s| self.business_sticker_still(&s, 96., cx));
        self.business_intro_texts(page, cx).children(sticker)
    }

    pub(super) fn business_start_page_body(&mut self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some((title, message, sticker)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.start_page.as_ref())
            .map(|p| (p.title.clone(), p.message.clone(), p.sticker.clone()))
        else {
            return div();
        };
        let (title_max, message_max) = self.business_start_page_limits();
        let page = StartPage {
            title: title.read(cx).value().trim().to_string(),
            message: message.read(cx).value().trim().to_string(),
            sticker: sticker.clone(),
        };
        let me = self.session().and_then(|s| s.my_user_id).unwrap_or(0);
        let preview = self.business_intro_card(&page, me, cx);
        let counter = |len: usize, max: usize| {
            div()
                .text_xs()
                .text_color(muted)
                .child(format!("{len}/{max}"))
        };
        let sticker_side = match &sticker {
            Some(sticker) => self.business_sticker_still(sticker, 28., cx),
            None => div()
                .text_color(cx.theme().primary)
                .child(tr(keys::Q_OPS_RANDOM))
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .justify_center()
                    .py_2()
                    .rounded_md()
                    .bg(cx.theme().muted)
                    .child(preview),
            )
            .child(Input::new(&title).aria_label(tr(keys::Q_OPS_TITLE)))
            .child(counter(page.title.chars().count(), title_max))
            .child(Input::new(&message).aria_label(tr(keys::Q_OPS_MESSAGE)))
            .child(counter(page.message.chars().count(), message_max))
            .child(
                crate::ui::pressable::action_row(
                    "business-intro-sticker",
                    None,
                    tr(keys::Q_OPS_CHOOSE_STICKER),
                    false,
                    cx,
                )
                .justify_between()
                .child(div().flex_1())
                .child(sticker_side)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.business_go(View::StickerPick, window, cx);
                })),
            )
            .child(div().text_xs().text_color(muted).child(about()))
            .when(!page.is_empty(), |this| {
                this.child(
                    crate::ui::pressable::action_row(
                        "business-intro-reset",
                        None,
                        tr(keys::Q_OPS_RESET_TO_DEFAULT),
                        true,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(page) = this.business_mut().and_then(|d| d.start_page.as_mut())
                        {
                            page.sticker = None;
                            let (title, message) = (page.title.clone(), page.message.clone());
                            title.update(cx, |input, cx| input.set_value("", window, cx));
                            message.update(cx, |input, cx| input.set_value("", window, cx));
                        }
                        cx.notify();
                    })),
                )
            })
    }

    /// The sticker panel while it picks the start page sticker.
    pub(super) fn business_sticker_pick_body(&mut self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.sticker_picker_panel(cx).into_any_element())
    }

    /// Save of the start page (when it is left).
    pub(super) fn save_business_start_page(&mut self, cx: &mut Context<Self>) -> Leave {
        let Some((title, message, sticker)) = self
            .dialogs
            .business_dialog
            .as_ref()
            .and_then(|d| d.start_page.as_ref())
            .map(|p| {
                (
                    p.title.read(cx).value().trim().to_string(),
                    p.message.read(cx).value().trim().to_string(),
                    p.sticker.clone(),
                )
            })
        else {
            return Leave::Go(None);
        };
        let saved = self.my_business().start_page.unwrap_or_default();
        let same_sticker =
            sticker.as_ref().map(|s| s.file_id) == saved.sticker.as_ref().map(|s| s.file_id);
        if title == saved.title && message == saved.message && same_sticker {
            return Leave::Go(None);
        }
        let file = sticker.map(|s| s.file_id.0);
        self.business_send(tr(keys::Q_OPS_START_PAGE_2), |driver| {
            driver
                .save_business_start_page(&title, &message, file)
                .is_ok()
        })
    }
}

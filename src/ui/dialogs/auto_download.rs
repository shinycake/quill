//! Settings › Data and storage › Automatic media download (Telegram
//! Desktop `BuildAutoDownloadSection` and `AutoDownloadBox`): a master
//! switch, then one row per chat source with its own switch, each opening
//! the source's box in the page (download and autoplay kinds with their
//! size sliders, and the always / never exceptions with a chat picker).
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::super::index_sliders::StepSlider;
use crate::ui::settings_window::rows::{group, link_row, switch_row};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn};
use quill::limit_steps::{cap_at, cap_index};
use quill::settings::{
    AutoDownloadDraft, AutoDownloadKind, AutoDownloadLimitGroup, AutoDownloadOverride,
    AutoDownloadSettings, AutoDownloadSource,
};
use std::collections::BTreeSet;

/// The source box over the Data and storage page. Leaving the page, or the
/// screen, closes it from any stage; nothing applies before Save.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum AutoDownloadEditor {
    #[default]
    Closed,
    Open(AutoDownloadDraft),
    /// The chat picker of one exception list, over the box: `picked` is
    /// what its Save makes the list.
    Picking {
        draft: AutoDownloadDraft,
        which: AutoDownloadOverride,
        picked: BTreeSet<i64>,
    },
}

impl AutoDownloadEditor {
    #[cfg(test)]
    pub(crate) fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub(crate) fn draft_mut(&mut self) -> Option<&mut AutoDownloadDraft> {
        match self {
            Self::Closed => None,
            Self::Open(draft) | Self::Picking { draft, .. } => Some(draft),
        }
    }

    /// The picker's Cancel, or Escape: back to the box as it was.
    pub(crate) fn close_picker(&mut self) {
        if let Self::Picking { draft, .. } = self {
            *self = Self::Open(draft.clone());
        }
    }

    /// The picker's Save: the picked chats become the list.
    pub(crate) fn save_picker(&mut self) {
        if let Self::Picking {
            draft,
            which,
            picked,
        } = self
        {
            let mut draft = draft.clone();
            draft.set_exceptions(*which, std::mem::take(picked));
            *self = Self::Open(draft);
        }
    }
}

fn source_title(source: AutoDownloadSource) -> &'static str {
    match source {
        AutoDownloadSource::Private => tr(keys::MEDIA_AUTO_IN_PRIVATE),
        AutoDownloadSource::Groups => tr(keys::MEDIA_AUTO_IN_GROUPS),
        AutoDownloadSource::Channels => tr(keys::MEDIA_AUTO_IN_CHANNELS),
    }
}

fn source_icon(source: AutoDownloadSource) -> IconName {
    match source {
        AutoDownloadSource::Private => IconName::User,
        AutoDownloadSource::Groups => IconName::Users,
        AutoDownloadSource::Channels => IconName::Megaphone,
    }
}

fn source_slug(source: AutoDownloadSource) -> &'static str {
    match source {
        AutoDownloadSource::Private => "private",
        AutoDownloadSource::Groups => "groups",
        AutoDownloadSource::Channels => "channels",
    }
}

fn kind_title(kind: AutoDownloadKind) -> &'static str {
    match kind {
        AutoDownloadKind::Photo => tr(keys::MEDIA_PHOTO_TITLE),
        AutoDownloadKind::File => tr(keys::MEDIA_FILE_TITLE),
        AutoDownloadKind::VideoMessage => tr(keys::MEDIA_VIDEO_MESSAGES_TITLE),
        AutoDownloadKind::Video => tr(keys::MEDIA_VIDEO_TITLE),
        AutoDownloadKind::Gif => tr(keys::MEDIA_ANIMATION_TITLE),
        AutoDownloadKind::Voice | AutoDownloadKind::Music => "",
    }
}

fn kind_slug(kind: AutoDownloadKind) -> &'static str {
    match kind {
        AutoDownloadKind::Photo => "photo",
        AutoDownloadKind::File => "file",
        AutoDownloadKind::Voice => "voice",
        AutoDownloadKind::Music => "music",
        AutoDownloadKind::VideoMessage => "video-message",
        AutoDownloadKind::Video => "video",
        AutoDownloadKind::Gif => "gif",
    }
}

/// "N users" for a list, or "Add users" while it is empty.
fn exceptions_value(source: AutoDownloadSource, count: usize) -> String {
    match (source, count) {
        (AutoDownloadSource::Private, 0) => tr(keys::MEDIA_AUTO_EXCEPTIONS_ADD_USERS).into(),
        (AutoDownloadSource::Groups, 0) => tr(keys::MEDIA_AUTO_EXCEPTIONS_ADD_GROUPS).into(),
        (AutoDownloadSource::Channels, 0) => tr(keys::MEDIA_AUTO_EXCEPTIONS_ADD_CHANNELS).into(),
        (AutoDownloadSource::Private, n) => trn(keys::MEDIA_AUTO_EXCEPTIONS_USERS, n),
        (AutoDownloadSource::Groups, n) => trn(keys::MEDIA_AUTO_EXCEPTIONS_GROUPS, n),
        (AutoDownloadSource::Channels, n) => trn(keys::MEDIA_AUTO_EXCEPTIONS_CHANNELS, n),
    }
}

fn exceptions_title(which: AutoDownloadOverride) -> &'static str {
    match which {
        AutoDownloadOverride::Always => tr(keys::MEDIA_AUTO_ALWAYS),
        AutoDownloadOverride::Never => tr(keys::MEDIA_AUTO_NEVER),
    }
}

fn slider_key(group: AutoDownloadLimitGroup) -> StepSlider {
    StepSlider::AutoDownloadLimit(group)
}

fn slider_id(group: AutoDownloadLimitGroup) -> &'static str {
    match group {
        AutoDownloadLimitGroup::Download => "auto-download-limit-download",
        AutoDownloadLimitGroup::Autoplay => "auto-download-limit-autoplay",
    }
}

impl QuillApp {
    fn auto_download_settings(&self) -> AutoDownloadSettings {
        self.session()
            .map(|s| (*s.settings.media_prefs.auto_download).clone())
            .unwrap_or_default()
    }

    /// The source a chat this account knows belongs to.
    fn auto_download_source_of(&self) -> impl Fn(ChatId) -> Option<AutoDownloadSource> + '_ {
        move |chat: ChatId| {
            self.session()
                .and_then(|s| s.chats.get(&chat.0))
                .map(|chat| AutoDownloadSource::of_chat(&chat.kind))
        }
    }

    /// The section on the Data and storage page.
    pub(crate) fn auto_download_section(&self, cx: &mut Context<Self>) -> AnyElement {
        let paused = self
            .session()
            .is_some_and(|s| s.settings.media_prefs.data_saver);
        let settings = self.auto_download_settings();
        let mut rows = vec![self.setting_spot(
            &["data.auto_download"],
            switch_row(
                "auto-download-master",
                tr(keys::Q_AUTO_DOWNLOAD_MASTER),
                None,
                !paused,
                cx.listener(|this, &on: &bool, _, cx| {
                    this.set_media_pref(|prefs| prefs.data_saver = !on, cx);
                }),
                cx,
            ),
            cx,
        )];
        for source in AutoDownloadSource::ALL {
            let ids: &'static [&'static str] = match source {
                AutoDownloadSource::Private => &["data.auto_download_private"],
                AutoDownloadSource::Groups => &["data.auto_download_groups"],
                AutoDownloadSource::Channels => &["data.auto_download_channels"],
            };
            let on = settings.source(source).any_enabled();
            let row = self.auto_download_source_row(source, on, paused, cx);
            rows.push(self.setting_spot(ids, row, cx));
        }
        group(Some(tr(keys::MEDIA_AUTO_SETTINGS)), rows).into_any_element()
    }

    /// One source row: the title opens the box, the switch beside it turns
    /// the whole source on (tdesktop defaults) or off.
    fn auto_download_source_row(
        &self,
        source: AutoDownloadSource,
        on: bool,
        paused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slug = source_slug(source);
        let title = source_title(source);
        let muted = cx.theme().muted_foreground;
        let open = Button::new(format!("auto-download-open-{slug}"))
            .ghost()
            .flex_1()
            .h(px(40.))
            .disabled(paused)
            .accessibility_label(title)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .w_full()
                    .child(Icon::new(source_icon(source)).size_4().text_color(muted))
                    .child(div().flex_1().min_w_0().text_sm().truncate().child(title))
                    .child(Icon::new(IconName::ChevronRight).size_4().text_color(muted)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_auto_download(source, cx)));
        div()
            .flex()
            .items_center()
            .gap_2()
            .w_full()
            .child(open)
            .child(div().w(px(1.)).h(px(24.)).bg(cx.theme().border))
            .child(
                div().px_3().child(
                    Switch::new(format!("auto-download-switch-{slug}"))
                        .checked(on && !paused)
                        .disabled(paused)
                        .accessibility_label(title)
                        .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                            this.set_media_pref(
                                |prefs| prefs.auto_download.set_source_enabled(source, on),
                                cx,
                            );
                        })),
                ),
            )
            .into_any_element()
    }

    pub(crate) fn open_auto_download(
        &mut self,
        source: AutoDownloadSource,
        cx: &mut Context<Self>,
    ) {
        let draft = AutoDownloadDraft::new(
            &self.auto_download_settings(),
            source,
            self.auto_download_source_of(),
        );
        for group in [
            AutoDownloadLimitGroup::Download,
            AutoDownloadLimitGroup::Autoplay,
        ] {
            let start = cap_index(draft.group_limit(group));
            self.set_step_slider(slider_key(group), start, cx);
        }
        self.settings.auto_download_editor = AutoDownloadEditor::Open(draft);
        cx.notify();
    }

    /// Leaving the page or closing the box drops the box and its sliders.
    pub(crate) fn close_auto_download(&mut self) {
        self.settings.auto_download_editor = AutoDownloadEditor::Closed;
        for group in [
            AutoDownloadLimitGroup::Download,
            AutoDownloadLimitGroup::Autoplay,
        ] {
            self.index_sliders.remove(slider_key(group));
        }
    }

    fn save_auto_download(&mut self, cx: &mut Context<Self>) {
        let AutoDownloadEditor::Open(draft) = self.settings.auto_download_editor.clone() else {
            return;
        };
        let mut settings = self.auto_download_settings();
        draft.apply(&mut settings, self.auto_download_source_of());
        self.set_media_pref(move |prefs| *prefs.auto_download = settings, cx);
        self.close_auto_download();
        cx.notify();
    }

    /// A slider of the box moved.
    pub(in crate::ui) fn on_auto_download_slider(
        &mut self,
        group: AutoDownloadLimitGroup,
        index: usize,
    ) {
        if let Some(draft) = self.settings.auto_download_editor.draft_mut() {
            draft.set_group_limit(group, cap_at(index));
        }
    }

    /// The page body while the box is open: the box, or its picker.
    pub(crate) fn auto_download_editor_page(&self, cx: &mut Context<Self>) -> Option<Div> {
        match &self.settings.auto_download_editor {
            AutoDownloadEditor::Closed => None,
            AutoDownloadEditor::Open(draft) => Some(self.auto_download_box(draft, cx)),
            AutoDownloadEditor::Picking {
                draft,
                which,
                picked,
            } => Some(self.auto_download_picker(draft.source, *which, picked, cx)),
        }
    }

    fn auto_download_box(&self, draft: &AutoDownloadDraft, cx: &mut Context<Self>) -> Div {
        let source = draft.source;
        let mut download = Vec::new();
        let mut autoplay = Vec::new();
        for kind in AutoDownloadKind::SHOWN {
            let row = switch_row(
                format!("auto-download-kind-{}", kind_slug(kind)),
                kind_title(kind),
                None,
                draft.is_on(kind),
                cx.listener(move |this, &on: &bool, _, cx| {
                    if let Some(draft) = this.settings.auto_download_editor.draft_mut() {
                        draft.set_on(kind, on);
                    }
                    cx.notify();
                }),
                cx,
            );
            match AutoDownloadLimitGroup::of(kind) {
                AutoDownloadLimitGroup::Download => download.push(row),
                AutoDownloadLimitGroup::Autoplay => autoplay.push(row),
            }
        }
        download.extend(self.auto_download_slider(draft, AutoDownloadLimitGroup::Download, cx));
        autoplay.extend(self.auto_download_slider(draft, AutoDownloadLimitGroup::Autoplay, cx));
        let exceptions = [AutoDownloadOverride::Always, AutoDownloadOverride::Never].map(|which| {
            let slug = match which {
                AutoDownloadOverride::Always => "always",
                AutoDownloadOverride::Never => "never",
            };
            link_row(
                format!("auto-download-exceptions-{slug}"),
                exceptions_title(which),
                Some(exceptions_value(source, draft.exceptions(which).len()).into()),
                cx.listener(move |this, _, _, cx| {
                    if let AutoDownloadEditor::Open(draft) = &this.settings.auto_download_editor {
                        let picked = draft.exceptions(which).clone();
                        this.settings.auto_download_editor = AutoDownloadEditor::Picking {
                            draft: draft.clone(),
                            which,
                            picked,
                        };
                    }
                    cx.notify();
                }),
                cx,
            )
        });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_lg().font_semibold().child(source_title(source)))
            .child(group(Some(tr(keys::MEDIA_AUTO_TITLE)), download))
            .child(group(Some(tr(keys::MEDIA_AUTO_PLAY)), autoplay))
            .child(group(Some(tr(keys::MEDIA_AUTO_EXCEPTIONS)), exceptions))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("auto-download-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_auto_download();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("auto-download-save")
                            .label(tr(keys::SETTINGS_SAVE))
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| this.save_auto_download(cx))),
                    ),
            )
    }

    /// "Limit by size" with the group's value and slider.
    fn auto_download_slider(
        &self,
        draft: &AutoDownloadDraft,
        group: AutoDownloadLimitGroup,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let megabytes = draft.group_limit(group) / (1024 * 1024);
        let size = format!("{megabytes} {}", tr(keys::SIZE_MEGABYTES));
        let value = tr_with(keys::MEDIA_SIZE_UP_TO, &[("size", size.as_str())]);
        let row = self.step_slider_row(
            slider_key(group),
            slider_id(group),
            tr(keys::MEDIA_SIZE_LIMIT).to_string(),
            value,
            cx,
        )?;
        Some(div().px_3().py_2().child(row).into_any_element())
    }

    /// The chats of the box's source to pick from (tdesktop
    /// `ExceptionsBoxController`): the chat list's, and any already picked.
    fn auto_download_picker(
        &self,
        source: AutoDownloadSource,
        which: AutoDownloadOverride,
        picked: &BTreeSet<i64>,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut chats: Vec<(i64, String)> = self
            .session()
            .map(|s| {
                let mut seen = BTreeSet::new();
                let listed = s.ordered_chats().into_iter().map(|chat| chat.id.0);
                listed
                    .chain(picked.iter().copied())
                    .filter(|id| seen.insert(*id))
                    .filter_map(|id| s.chats.get(&id))
                    .filter(|chat| AutoDownloadSource::of_chat(&chat.kind) == source)
                    .filter(|chat| !s.is_saved_messages(chat.id))
                    .map(|chat| (chat.id.0, chat.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        chats.sort_by_key(|(id, _)| !picked.contains(id));
        let mut list = div().flex().flex_col().gap_1();
        if chats.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_AUTO_DOWNLOAD_NO_CHATS)),
            );
        }
        for (chat_id, title) in chats {
            list = list.child(
                div().px_3().py_1().child(
                    Checkbox::new(format!("auto-download-pick-{chat_id}"))
                        .checked(picked.contains(&chat_id))
                        .label(title)
                        .on_click(cx.listener(move |this, &on: &bool, _, cx| {
                            if let AutoDownloadEditor::Picking { picked, .. } =
                                &mut this.settings.auto_download_editor
                            {
                                if on {
                                    picked.insert(chat_id);
                                } else {
                                    picked.remove(&chat_id);
                                }
                            }
                            cx.notify();
                        })),
                ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child(exceptions_title(which)),
            )
            .child(group(None, [list.into_any_element()]))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("auto-download-pick-cancel")
                            .label(tr(keys::CANCEL))
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.auto_download_editor.close_picker();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("auto-download-pick-save")
                            .label(tr(keys::SETTINGS_SAVE))
                            .primary()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.auto_download_editor.save_picker();
                                cx.notify();
                            })),
                    ),
            )
    }
}

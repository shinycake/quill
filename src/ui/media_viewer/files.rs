//! Documents and the viewer: opening a file the way Telegram Desktop's
//! `ResolveDocument` does (a picture it can read opens in the viewer, any
//! other file in another app), the viewer over the chat's files, and the
//! card the viewer shows for a file that is not a picture (tdesktop's
//! document bubble in `OverlayWidget`).

use super::*;
use crate::ui::image_decode::{self, READ_BYTES_LIMIT, Sniffed};
use quill::lang::{keys, tr, tr_with};
use quill::media_viewer::{collect_file_items, opens_as_picture};
use quill::telegram::envelope::{DocumentContent, MessageContent};
use std::io::Read as _;
use std::path::Path;

impl QuillApp {
    /// Open a document a chat row shows: download it first, then the
    /// viewer for a picture Quill can show, else another app.
    pub(in crate::ui) fn open_document(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        file_id: FileId,
        cx: &mut Context<Self>,
    ) {
        let path: Option<PathBuf> = self
            .session()
            .and_then(|s| s.media.files.get(&file_id.0))
            .and_then(|f| f.usable_path())
            .map(PathBuf::from);
        let Some(path) = path else {
            self.request_media_download(file_id, None, cx);
            return;
        };
        let picture = self
            .session()
            .and_then(|s| s.histories.get(&chat_id.0)?.messages.get(&message_id.0))
            .is_some_and(|message| match &message.content {
                MessageContent::Document(doc) => picture_opens_in_app(&path, doc),
                _ => false,
            });
        if picture && self.open_file_viewer(chat_id, message_id, cx) {
            return;
        }
        self.open_file_guarded(path, cx);
    }

    /// Open the viewer on a document message, paging over the chat's
    /// files (tdesktop `SharedMediaType::File`). `false` when the message
    /// is not in the loaded history.
    pub(in crate::ui) fn open_file_viewer(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> bool {
        let items = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .map(|history| {
                collect_file_items(&history.ordered().into_iter().cloned().collect::<Vec<_>>())
            })
            .unwrap_or_default();
        let Some(index) = items.iter().position(|item| item.message_id == message_id) else {
            return false;
        };
        self.viewer.state = MediaViewer::open(items, index);
        self.viewer.open_gen += 1;
        self.viewer_note_activity(cx);
        self.reset_viewer_item_state(cx);
        cx.notify();
        true
    }

    /// The card for the current file item, or `None` when it shows as a
    /// picture.
    pub(super) fn viewer_file_card(
        &self,
        item: &MediaViewerItem,
        files: &quill::state::FileMap,
        downloading: &quill::state::DownloadIntents,
    ) -> Option<FileCard> {
        let file = files.get(&item.download_file_id.0);
        let size = file.map(ParsedFile::display_size).unwrap_or(0);
        let too_large = size > READ_BYTES_LIMIT;
        let shown_as_picture = item.kind == MediaViewerKind::ImageFile && !too_large;
        if !matches!(
            item.kind,
            MediaViewerKind::ImageFile | MediaViewerKind::File
        ) || shown_as_picture
        {
            return None;
        }
        Some(FileCard::new(item, files, downloading))
    }
}

/// tdesktop `CheckImageOpenInApp`: smaller than `kReadBytesLimit`, named
/// or typed as a picture, and its first bytes are one Quill decodes. (The
/// read is a few bytes of a local file, at a click.)
fn picture_opens_in_app(path: &Path, doc: &DocumentContent) -> bool {
    if !opens_as_picture(&doc.file_name, &doc.mime_type) {
        return false;
    }
    let small_enough =
        std::fs::metadata(path).is_ok_and(|meta| meta.len() <= READ_BYTES_LIMIT as u64);
    if !small_enough {
        return false;
    }
    let mut head = [0_u8; 64];
    let read = std::fs::File::open(path)
        .and_then(|mut file| file.read(&mut head))
        .unwrap_or(0);
    match image_decode::sniff(head.get(..read).unwrap_or_default(), Some(path)) {
        // ImageIO also reads camera RAW, PSD and the rest by content.
        Sniffed::Other => cfg!(target_os = "macos") && read > 0,
        _ => true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FileState {
    Remote,
    Downloading(Option<f32>),
    Local,
}

/// What the viewer's file card shows. Built up front, so a picture that
/// fails to decode can show the same card from its fallback.
#[derive(Clone, Debug)]
pub(super) struct FileCard {
    row_id: u64,
    file_id: FileId,
    name: String,
    size: i64,
    state: FileState,
}

impl FileCard {
    pub(super) fn new(
        item: &MediaViewerItem,
        files: &quill::state::FileMap,
        downloading: &quill::state::DownloadIntents,
    ) -> Self {
        let file_id = item.download_file_id;
        let file = files.get(&file_id.0);
        let state = if file.and_then(|f| f.usable_path()).is_some() {
            FileState::Local
        } else if file_is_downloading(file_id, files, downloading) {
            FileState::Downloading(file.and_then(ParsedFile::download_progress))
        } else {
            FileState::Remote
        };
        Self {
            row_id: item.message_id.0 as u64,
            file_id,
            name: item
                .file_name
                .clone()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| tr(keys::Q_AV_FILE).to_string()),
            size: file.map(ParsedFile::display_size).unwrap_or(0),
            state,
        }
    }

    /// The card: a file glyph, the name, the size or progress, and the
    /// actions. `view` runs them (no `cx` here, so the picture fallback
    /// can build it).
    pub(super) fn render(self, view: WeakEntity<QuillApp>) -> AnyElement {
        let Self {
            row_id,
            file_id,
            name,
            size,
            state,
        } = self;
        let status = match state {
            FileState::Downloading(Some(progress)) => tr_with(
                keys::Q_AV_DOWNLOADING_3,
                &[("round", &((progress * 100.0).round() as i32).to_string())],
            ),
            FileState::Downloading(None) => tr(keys::Q_AV_DOWNLOADING_4).to_string(),
            _ => format_bytes(size),
        };
        let act = move |action: fn(&mut QuillApp, FileId, &mut Context<QuillApp>)| {
            let view = view.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |this, cx| action(this, file_id, cx));
            }
        };
        let button = |id: &'static str, label: &'static str| {
            Button::new((id, row_id))
                .label(label)
                .ghost()
                .text_color(gpui_kit::white())
        };
        let actions = match state {
            FileState::Remote => div().child(
                button("viewer-file-download", tr(keys::MEDIA_DOWNLOAD)).on_click(act(
                    |this, id, cx| this.request_media_download(id, None, cx),
                )),
            ),
            FileState::Downloading(_) => div().child(
                button("viewer-file-cancel", tr(keys::MEDIA_CANCEL))
                    .on_click(act(|this, id, cx| this.cancel_media_download(id, cx))),
            ),
            FileState::Local => div()
                .flex()
                .gap_1()
                .child(
                    button("viewer-file-open", tr(keys::Q_AV_OPEN_IN_ANOTHER_APP))
                        .on_click(act(|this, id, cx| this.open_downloaded_file(id, cx))),
                )
                .child(
                    button("viewer-file-reveal", tr(keys::Q_AV_SHOW_IN_FOLDER_2))
                        .on_click(act(|this, id, cx| this.reveal_downloaded_file(id, cx))),
                ),
        };
        div()
            .id(("viewer-file-card", row_id))
            .flex()
            .items_center()
            .gap_4()
            .p_4()
            .max_w(px(480.))
            .rounded_lg()
            .bg(gpui_kit::white().opacity(0.08))
            .child(
                div()
                    .size(px(56.))
                    .flex_none()
                    .rounded_full()
                    .bg(gpui_kit::white().opacity(0.14))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(gpui_kit::assets::IconName::File)
                            .size(px(24.))
                            .text_color(gpui_kit::white()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_color(gpui_kit::white())
                            .font_semibold()
                            .truncate()
                            .child(name),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(gpui_kit::white().opacity(0.72))
                            .child(status),
                    )
                    .child(actions),
            )
            .into_any_element()
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "files_tests.rs"]
mod tests;

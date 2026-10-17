//! Screenshot demos for pictures sent as files: the thumbed file rows in a
//! chat (`ready-image-files`), the viewer open on a HEIC file
//! (`ready-image-file-viewer`), and the same with an HDR gain-map HEIC
//! (`ready-hdr-files`, `ready-hdr-file-viewer`). Injected updates, no live
//! Telegram.

use crate::ui::app::QuillApp;
use crate::ui::demo::demo_media_allowlist;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, MessageId};
use quill::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;

register_demos![
    DemoSpec::chats(
        "ready-image-files",
        "screenshot demo — pictures sent as files"
    )
    .setup(QuillApp::demo_image_files),
    DemoSpec::chats(
        "ready-image-file-viewer",
        "screenshot demo — a HEIC file in the viewer"
    )
    .setup(QuillApp::demo_image_file_viewer),
    DemoSpec::chats(
        "ready-hdr-files",
        "screenshot demo — an HDR picture sent as a file"
    )
    .setup(QuillApp::demo_hdr_files),
    DemoSpec::chats(
        "ready-hdr-file-viewer",
        "screenshot demo — an HDR file in the viewer"
    )
    .setup(QuillApp::demo_hdr_file_viewer),
];

const CHAT: i64 = 11;
const HEIC_MESSAGE: i64 = 901;
pub(super) const HDR_MESSAGE: i64 = 904;

fn fixture(name: &str) -> String {
    demo_media_allowlist()
        .join("showcase")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// A TDLib `file`: local and complete when `path` is set, remote otherwise.
fn file(id: i32, path: Option<&str>, size: i64) -> Value {
    json!({
        "@type": "file",
        "id": id,
        "size": size,
        "expected_size": size,
        "local": {
            "@type": "localFile",
            "path": path.unwrap_or_default(),
            "can_be_downloaded": true,
            "can_be_deleted": false,
            "is_downloading_active": false,
            "is_downloading_completed": path.is_some(),
            "download_offset": 0,
            "downloaded_prefix_size": if path.is_some() { size } else { 0 },
            "downloaded_size": if path.is_some() { size } else { 0 }
        },
        "remote": {
            "@type": "remoteFile",
            "id": "x",
            "unique_id": "u",
            "is_uploading_active": false,
            "is_uploading_completed": true,
            "uploaded_size": size
        }
    })
}

fn document_message(
    id: i64,
    outgoing: bool,
    name: &str,
    mime: &str,
    document: Value,
    thumbnail: Option<Value>,
    caption: &str,
) -> String {
    let thumbnail = thumbnail.map_or(Value::Null, |file| {
        json!({
            "@type": "thumbnail",
            "format": { "@type": "thumbnailFormatJpeg" },
            "width": 320,
            "height": 215,
            "file": file
        })
    });
    json!({
        "@type": "updateNewMessage",
        "message": {
            "id": id,
            "chat_id": CHAT,
            "date": 1790631120,
            "is_outgoing": outgoing,
            "content": {
                "@type": "messageDocument",
                "document": {
                    "@type": "document",
                    "file_name": name,
                    "mime_type": mime,
                    "minithumbnail": null,
                    "thumbnail": thumbnail,
                    "document": document
                },
                "caption": { "@type": "formattedText", "text": caption, "entities": [] }
            }
        }
    })
    .to_string()
}

impl QuillApp {
    /// A downloaded HEIC with its thumbnail, one still on the server and a
    /// text file without a thumbnail.
    fn demo_image_files(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let heic = fixture("ridge.heic");
        let thumb = fixture("ridge-thumb.jpg");
        let notes = demo_media_allowlist()
            .join("demo-notes.txt")
            .to_string_lossy()
            .into_owned();
        let updates = [
            document_message(
                HEIC_MESSAGE,
                false,
                "IMG_2041.HEIC",
                "image/heic",
                file(911, Some(&heic), 11_208),
                Some(file(912, Some(&thumb), 11_360)),
                "Ridge at golden hour, straight off the phone",
            ),
            document_message(
                902,
                true,
                "IMG_2042.HEIC",
                "image/heic",
                file(913, None, 3_481_204),
                Some(file(914, Some(&thumb), 11_360)),
                "",
            ),
            document_message(
                903,
                false,
                "demo-notes.txt",
                "text/plain",
                file(915, Some(&notes), 68),
                None,
                "",
            ),
        ];
        for json in updates {
            if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(owned);
            }
        }
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — pictures sent as files",
        ));
    }

    /// An HDR sunset (an ISO gain-map HEIC) sent as a file.
    fn demo_hdr_files(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        let sink: Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let json = document_message(
            HDR_MESSAGE,
            true,
            "IMG_2077.HEIC",
            "image/heic",
            file(921, Some(&fixture("sunset-hdr.heic")), 6_978),
            Some(file(922, Some(&fixture("sunset-hdr-thumb.jpg")), 6_473)),
            "Sunset from the ridge, in HDR",
        );
        if let Some(owned) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
            session.apply(owned);
        }
        self.connection.set_note(StatusNote::silent(
            "screenshot demo — an HDR picture sent as a file",
        ));
    }

    fn demo_hdr_file_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_hdr_files(window, cx);
        self.open_file_viewer(ChatId(CHAT), MessageId(HDR_MESSAGE), cx);
    }

    fn demo_image_file_viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.demo_image_files(window, cx);
        self.open_file_viewer(ChatId(CHAT), MessageId(HEIC_MESSAGE), cx);
    }
}

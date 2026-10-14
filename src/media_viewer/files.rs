//! Files in the viewer. Telegram Desktop opens a document in its media
//! viewer when the picture in it is readable (`CheckImageOpenInApp`,
//! data_document_resolver.cpp), and the viewer then pages over the chat's
//! files (`SharedMediaType::File`): image files show as pictures, the
//! other files as a card with their name, size and a download / open
//! action.

use super::*;
use crate::telegram::envelope::DocumentContent;

/// tdesktop `DetectNameType` (core/mime_type.cpp): the file extensions it
/// treats as images.
const IMAGE_EXTENSIONS: &[&str] = &[
    "afdesign",
    "ai",
    "avif",
    "bmp",
    "dng",
    "gif",
    "heic",
    "icns",
    "ico",
    "jfif",
    "jpeg",
    "jpg",
    "jpg-large",
    "jxl",
    "nef",
    "png",
    "png-large",
    "psd",
    "qoi",
    "raw",
    "sketch",
    "svg",
    "tga",
    "tif",
    "tiff",
    "webp",
];

/// tdesktop `Core::FileIsImage`: by the name's extension, or by the MIME
/// type when the file has no name.
pub fn is_image_file(file_name: &str, mime_type: &str) -> bool {
    if file_name.is_empty() {
        return mime_type.to_ascii_lowercase().starts_with("image/");
    }
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    extension.is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension.as_str()))
}

/// Whether the viewer should try to show a document as a picture: an image
/// by name (above) or by MIME type (`CheckImageOpenInApp` checks the type of
/// the file's contents, so `photo.heif` or an extensionless `image/png`
/// qualifies too). Whether it really decodes is found out when it is shown;
/// a picture that does not falls back to the file card.
pub fn opens_as_picture(file_name: &str, mime_type: &str) -> bool {
    is_image_file(file_name, mime_type) || mime_type.to_ascii_lowercase().starts_with("image/")
}

/// tdesktop `Images::kReadBytesLimit` (lib_ui `ui/image/image_prepare.h`):
/// bigger image files open in another app, not the viewer (`file.size() >
/// kReadBytesLimit` in `image_prepare.cpp`).
pub const READ_BYTES_LIMIT: i64 = 64 * 1024 * 1024;

/// Picture files up to this size download on their own where photos do
/// (they show as a photo in the chat). A Quill choice: tdesktop has no
/// photo-like file to apply it to.
pub const PICTURE_FILE_AUTO_DOWNLOAD_MAX: i64 = 10 * 1024 * 1024;

/// A picture sent as a file shows in the chat as a photo (with a "file"
/// pill), not a file row: it is a picture by name or type, it has a
/// thumbnail to show until the file loads, and it is small enough to open
/// in the viewer. Anything else keeps the file row.
pub fn shows_as_photo(doc: &DocumentContent) -> bool {
    opens_as_picture(&doc.file_name, &doc.mime_type)
        && doc
            .thumbnail
            .as_ref()
            .is_some_and(|thumb| thumb.width > 0 && thumb.height > 0 && thumb.file_id.0 != 0)
        && doc.size <= READ_BYTES_LIMIT
}

/// The picture format a photo-like file shows on its pill: its extension in
/// capitals (`HEIC`, `JPG`), or the MIME subtype without one.
pub fn picture_format_label(doc: &DocumentContent) -> String {
    let from_name = doc
        .file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|ext| !ext.is_empty() && ext.len() <= 5);
    let from_mime = doc
        .mime_type
        .split_once('/')
        .map(|(_, subtype)| subtype.trim_start_matches("x-"))
        .filter(|subtype| !subtype.is_empty() && subtype.len() <= 5);
    from_name
        .or(from_mime)
        .map(|label| label.to_ascii_uppercase())
        .unwrap_or_else(|| "FILE".to_string())
}

/// The viewer item of a document message.
pub fn file_viewer_item(message: &HistoryMessage) -> Option<MediaViewerItem> {
    let MessageContent::Document(doc) = &message.content else {
        return None;
    };
    if doc.file_id.0 == 0 {
        return None;
    }
    let picture = opens_as_picture(&doc.file_name, &doc.mime_type);
    let thumb = doc.thumbnail.as_ref().filter(|thumb| thumb.file_id.0 != 0);
    let mut display_file_ids = Vec::new();
    if picture {
        display_file_ids.push(doc.file_id);
        display_file_ids.extend(thumb.map(|thumb| thumb.file_id));
    }
    Some(MediaViewerItem {
        chat_id: message.chat_id,
        message_id: message.id,
        kind: if picture {
            MediaViewerKind::ImageFile
        } else {
            MediaViewerKind::File
        },
        display_file_ids,
        download_file_id: doc.file_id,
        play_file_id: None,
        duration_secs: None,
        mime_type: Some(doc.mime_type.clone()),
        start_timestamp: None,
        caption: doc.caption.clone(),
        caption_entities: doc.caption_entities.clone(),
        duration_label: None,
        // The thumbnail has the picture's proportions (its pixel size is
        // only known once the file is decoded).
        natural_size: thumb
            .map(|thumb| (thumb.width, thumb.height))
            .filter(|(w, h)| *w > 0 && *h > 0),
        quality_options: Vec::new(),
        file_name: Some(doc.file_name.clone()),
    })
}

/// The chat's files, oldest first, for a viewer opened on one of them.
pub fn collect_file_items(messages: &[HistoryMessage]) -> Vec<MediaViewerItem> {
    messages.iter().filter_map(file_viewer_item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_viewer::test_support::test_message;
    use crate::telegram::envelope::AlbumCoverThumb;

    fn document(id: i64, name: &str, mime: &str, thumb: Option<i32>) -> HistoryMessage {
        test_message(
            5,
            id,
            MessageContent::Document(DocumentContent {
                thumbnail: thumb.map(|file| AlbumCoverThumb {
                    width: 320,
                    height: 240,
                    file_id: FileId(file),
                }),
                minithumbnail: None,
                size: 0,
                file_name: name.to_string(),
                mime_type: mime.to_string(),
                caption: String::new(),
                caption_entities: Vec::new(),
                file_id: FileId(id as i32 * 10),
            }),
        )
    }

    #[test]
    fn image_files_follow_tdesktop_extensions_then_mime() {
        assert!(is_image_file("IMG_0001.HEIC", "application/octet-stream"));
        assert!(is_image_file("shot.jxl", ""));
        assert!(is_image_file("scan.tiff", ""));
        assert!(!is_image_file("photo.heif", "image/heif"));
        assert!(!is_image_file("notes.txt", "image/png"));
        assert!(is_image_file("", "image/avif"));
        assert!(!is_image_file("", "application/pdf"));
        // The viewer also tries pictures by MIME type.
        assert!(opens_as_picture("photo.heif", "image/heif"));
        assert!(!opens_as_picture("report.pdf", "application/pdf"));
    }

    #[test]
    fn picture_files_with_a_thumbnail_show_as_photos() {
        let MessageContent::Document(mut doc) =
            document(1, "IMG_0001.HEIC", "image/heic", Some(91)).content
        else {
            panic!("a document")
        };
        assert!(shows_as_photo(&doc));
        assert_eq!(picture_format_label(&doc), "HEIC");
        doc.size = READ_BYTES_LIMIT;
        assert!(shows_as_photo(&doc));
        doc.size = READ_BYTES_LIMIT + 1;
        assert!(!shows_as_photo(&doc));
        doc.size = 0;
        doc.thumbnail = None;
        assert!(!shows_as_photo(&doc));
        let MessageContent::Document(pdf) =
            document(2, "report.pdf", "application/pdf", Some(92)).content
        else {
            panic!("a document")
        };
        assert!(!shows_as_photo(&pdf));
        let MessageContent::Document(unnamed) = document(3, "", "image/x-jxl", Some(93)).content
        else {
            panic!("a document")
        };
        assert_eq!(picture_format_label(&unnamed), "JXL");
    }

    #[test]
    fn files_page_as_pictures_and_cards() {
        let messages = vec![
            document(1, "IMG_0001.HEIC", "image/heic", Some(91)),
            document(2, "report.pdf", "application/pdf", None),
        ];
        let items = collect_file_items(&messages);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, MediaViewerKind::ImageFile);
        assert_eq!(items[0].display_file_ids, vec![FileId(10), FileId(91)]);
        assert_eq!(items[0].download_file_id, FileId(10));
        assert_eq!(items[0].natural_size, Some((320, 240)));
        assert_eq!(items[0].file_name.as_deref(), Some("IMG_0001.HEIC"));
        assert_eq!(items[1].kind, MediaViewerKind::File);
        assert!(items[1].display_file_ids.is_empty());
        // Photos and videos stay out of the files list, and files out of
        // the photo list.
        assert!(collect_media_items(&messages).is_empty());
    }
}

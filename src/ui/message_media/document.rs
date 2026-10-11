//! Document chips and the small helpers they share with other rows
//! (accent color, inline links, the action disc, sizes and names).

use super::*;

/// Accent color for controls drawn inside a bubble: the theme primary on
/// incoming bubbles, white on the accent-filled outgoing ones.
pub(in crate::ui) fn bubble_accent(outgoing: bool, cx: &App) -> Hsla {
    if outgoing {
        gpui_kit::white()
    } else {
        cx.theme().primary
    }
}

/// A compact text action inside a bubble ("Show in folder", "Transcribe",
/// "1.5×") in the bubble accent color.
pub(in crate::ui) fn inline_link(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    color: Hsla,
) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .role(gpui_kit::Role::Button)
        .aria_label(label.clone())
        .tab_index(0)
        .cursor_pointer()
        .text_xs()
        .font_medium()
        .text_color(color)
        .hover(|style| style.underline())
        .child(label)
}

/// Round accent action disc used by voice, audio and document rows: an
/// icon, optionally inside a progress ring (`ring: Some(None)` spins).
pub(in crate::ui) fn action_disc(
    id: impl Into<ElementId>,
    outgoing: bool,
    icon: gpui_kit::assets::IconName,
    ring: Option<Option<f32>>,
    label: &'static str,
    cx: &App,
) -> Stateful<Div> {
    let id = id.into();
    let (bg, fg) = if outgoing {
        (gpui_kit::white().opacity(0.22), gpui_kit::white())
    } else {
        (cx.theme().primary, gpui_kit::white())
    };
    div()
        .id(id.clone())
        .relative()
        .size(px(44.))
        .flex_none()
        .rounded_full()
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Button)
        .aria_label(label)
        .tab_index(0)
        .cursor_pointer()
        .when_some(ring, |this, fraction| {
            this.child(
                div().absolute().inset(px(3.)).child(
                    ProgressCircle::new(ElementId::NamedChild(Arc::new(id), "ring".into()))
                        .size_full()
                        .color(fg)
                        .loading(fraction.is_none())
                        .value(fraction.unwrap_or(0.) * 100.),
                ),
            )
        })
        .child(Icon::new(icon).size(px(20.)).text_color(fg))
}

/// A document row (tdesktop `HistoryDocument`): a round action disc —
/// download, progress ring with cancel, open, or retry — then the name and
/// a compact meta line ("450 KB · PDF", "1.2 MB of 3.4 MB") carrying the
/// secondary actions (Show in folder, Pause/Resume).
pub(in crate::ui) fn document_chip(
    row_id: u64,
    doc: &quill::telegram::envelope::DocumentContent,
    // The disc is accent-filled; on an accent outgoing bubble it switches
    // to a translucent white one.
    outgoing: bool,
    files: &HashMap<i32, ParsedFile>,
    downloading: &std::collections::HashSet<i32>,
    failed: &std::collections::HashSet<i32>,
    // Slice media-downloads-pause: `None` when the file isn't a pausable
    // (user-initiated, listed) download; `Some(paused)` otherwise.
    paused: Option<bool>,
    sponsored: Option<(ChatId, i64)>,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let file_id = doc.file_id;
    let file = files.get(&file_id.0);
    let ready = file.and_then(|f| f.usable_path()).is_some();
    let downloading_now = file_is_downloading(file_id, files, downloading);
    let paused_now = downloading_now && paused == Some(true);
    let failed_now = !ready && !downloading_now && failed.contains(&file_id.0);
    let progress = file.and_then(|f| f.download_progress());
    let size = file.map(|f| f.display_size()).unwrap_or(0);
    let size_label = format_bytes(size);
    let kind = document_kind_label(&doc.file_name, &doc.mime_type);
    let meta = if downloading_now {
        let done = file.map(|f| f.local.downloaded_size).unwrap_or(0);
        let mut text = match (done > 0, size_label.is_empty()) {
            (true, false) => format!("{} of {}", format_bytes(done), size_label),
            _ => "Downloading…".to_string(),
        };
        if paused_now {
            text.push_str(" · paused");
        }
        text
    } else if failed_now {
        "Download failed".to_string()
    } else {
        [size_label.as_str(), kind.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let name = if doc.file_name.is_empty() {
        "Document".to_string()
    } else {
        doc.file_name.clone()
    };
    let (disc_bg, disc_fg) = if outgoing {
        (gpui_kit::white().opacity(0.22), gpui_kit::white())
    } else {
        (cx.theme().primary, gpui_kit::white())
    };
    let (disc_icon, disc_label) = if ready {
        (gpui_kit::assets::IconName::File, "Open")
    } else if downloading_now {
        (gpui_kit::assets::IconName::X, "Cancel download")
    } else if failed_now {
        (gpui_kit::assets::IconName::RotateCcw, "Retry download")
    } else {
        (gpui_kit::assets::IconName::ArrowDown, "Download")
    };
    let primary_action = move |this: &mut QuillApp, cx: &mut Context<QuillApp>| {
        if ready {
            this.open_downloaded_file(file_id, cx);
        } else if downloading_now {
            this.cancel_media_download(file_id, cx);
        } else {
            this.request_media_download(file_id, sponsored, cx);
        }
    };
    let disc = div()
        .id(("doc-disc", row_id))
        .relative()
        .size(px(44.))
        .flex_none()
        .rounded_full()
        .bg(disc_bg)
        .flex()
        .items_center()
        .justify_center()
        .role(gpui_kit::Role::Button)
        .aria_label(disc_label)
        .tab_index(0)
        .cursor_pointer()
        .pressable(cx.theme())
        .when(downloading_now, |this| {
            this.child(
                div().absolute().inset(px(3.)).child(
                    ProgressCircle::new(("doc-ring", row_id))
                        .size_full()
                        .color(disc_fg)
                        .loading(progress.is_none() && !paused_now)
                        .value(progress.unwrap_or(0.) * 100.),
                ),
            )
        })
        .child(Icon::new(disc_icon).size(px(20.)).text_color(disc_fg))
        .on_click(cx.listener(move |this, _, _, cx| primary_action(this, cx)));
    let link_color = (!outgoing).then_some(cx.theme().primary);
    let link = |id: (&'static str, u64), label: &'static str| {
        div()
            .id(id)
            .when_some(link_color, |this, color| this.text_color(color))
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .tab_index(0)
            .cursor_pointer()
            .font_medium()
            .hover(|style| style.underline())
            .child(label)
    };
    // Slice media-downloads-pause: Pause/Resume only for user-initiated
    // (listed) downloads — `None` hides it.
    let pause_label = if downloading_now {
        paused.map(|is_paused| if is_paused { "Resume" } else { "Pause" })
    } else {
        None
    };
    div()
        .id(("doc-chip", row_id))
        .mt_1()
        .flex()
        .items_center()
        .gap_3()
        .min_w(px(quill::bubble_layout::FILE_MIN_WIDTH as f32))
        .max_w(px(quill::bubble_layout::MSG_MAX_WIDTH as f32))
        .child(disc)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .id(("doc-chip-name", row_id))
                        .role(gpui_kit::Role::Button)
                        .aria_label(format!("Open document {name}"))
                        .tab_index(0)
                        .cursor_pointer()
                        .text_sm()
                        .font_medium()
                        .truncate()
                        .child(name)
                        .on_click(cx.listener(move |this, _, _, cx| primary_action(this, cx))),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_x_2()
                        .text_xs()
                        .child(div().opacity(0.7).child(meta))
                        .when(ready, |this| {
                            this.child(link(("doc-action", row_id), "Show in folder").on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.reveal_downloaded_file(file_id, cx);
                                }),
                            ))
                        })
                        .when(failed_now, |this| {
                            this.child(link(("doc-action", row_id), "Retry").on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.request_media_download(file_id, sponsored, cx);
                                },
                            )))
                        })
                        .when_some(pause_label, |this, label| {
                            this.child(link(("doc-pause", row_id), label).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if paused_now {
                                        this.resume_media_download(file_id, cx);
                                    } else {
                                        this.pause_media_download(file_id, cx);
                                    }
                                },
                            )))
                        }),
                ),
        )
        .into_any_element()
}

/// Short type tag for a document row: the file extension ("PDF", "ZIP"),
/// else the MIME subtype, else nothing.
pub(in crate::ui) fn document_kind_label(file_name: &str, mime_type: &str) -> String {
    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty() && ext.len() <= 6);
    match ext {
        Some(ext) => ext.to_ascii_uppercase(),
        None => mime_type
            .split('/')
            .nth(1)
            .filter(|sub| !sub.is_empty() && sub.len() <= 12)
            .map(str::to_ascii_uppercase)
            .unwrap_or_default(),
    }
}

/// MED3: display name for a downloads-manager row — the document's
/// `file_name` when the file belongs to a known message, else the local
/// path's file name, else a plain "File {id}" fallback.
pub(in crate::ui) fn download_display_name(session: &Session, file_id: i32) -> String {
    if let Some(name) = session.sync.download_names.get(&file_id) {
        return name.clone();
    }
    for history in session.histories.values() {
        for message in history.messages.values() {
            if let quill::telegram::envelope::MessageContent::Document(doc) = &message.content
                && doc.file_id.0 == file_id
            {
                return if doc.file_name.is_empty() {
                    "Document".to_string()
                } else {
                    doc.file_name.clone()
                };
            }
        }
    }
    session
        .media
        .files
        .get(&file_id)
        .and_then(|f| f.usable_path())
        .and_then(|p| std::path::Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("File {file_id}"))
}

pub(in crate::ui) fn format_bytes(n: i64) -> String {
    if n <= 0 {
        String::new()
    } else if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{} KB", n / 1024)
    } else {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::document_kind_label;

    #[test]
    fn document_kind_prefers_extension_then_mime_subtype() {
        assert_eq!(document_kind_label("report.pdf", "application/pdf"), "PDF");
        assert_eq!(document_kind_label("notes", "text/plain"), "PLAIN");
        assert_eq!(document_kind_label("", ""), "");
        assert_eq!(document_kind_label("weird.verylongext", ""), "");
    }
}

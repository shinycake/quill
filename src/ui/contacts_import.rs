//! The Contacts "Import" box: choose a `.vcf` file, see the contacts in it,
//! then import them with `importContacts`. The box renders from
//! `ImportContactsDialog::state` (`quill::vcard_import`) alone: choosing a
//! file only changes that value. Telegram Desktop has no vCard import
//! (docs/decisions/request-location-vcard-file.md).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialogs::contacts::ImportContactsDialog;
use super::*;
use crate::ui::status_note::StatusNote;
use crate::ui::user_error::user_error_for;
use gpui_kit::component::button::*;
use gpui_kit::*;
use quill::lang::{keys, tr, tr_with, trn, trn_with};
use quill::telegram::requests::{ImportedContact, VCARD_IMPORT_LIMIT};
use quill::vcard_import::{VcardError, VcardImport, VcardPreview, display_name, read_vcard_file};
use std::borrow::Cow;
use std::path::PathBuf;

/// The preview lists this many contacts before "and N more".
const PREVIEW_ROWS: usize = 6;

/// Where the box stands: what it shows, with the file dialog open over it,
/// or reading a chosen file. The two off-thread stages own their task, so a
/// box that closes or a new choice ends the work, and its answer can only
/// come to the stage that asked.
pub(crate) enum ImportStage {
    Showing(VcardImport),
    /// The system open dialog is up over what the box showed `before`.
    Choosing {
        before: VcardImport,
        _picker: Task<()>,
    },
    Reading {
        file_name: String,
        _read: Task<()>,
    },
}

impl Default for ImportStage {
    fn default() -> Self {
        Self::Showing(VcardImport::Empty)
    }
}

impl ImportStage {
    /// What the box draws.
    pub(crate) fn state(&self) -> Cow<'_, VcardImport> {
        match self {
            Self::Showing(state) | Self::Choosing { before: state, .. } => Cow::Borrowed(state),
            Self::Reading { file_name, .. } => Cow::Owned(VcardImport::Reading {
                file_name: file_name.clone(),
            }),
        }
    }

    /// What the box showed last that a file read produced (or nothing yet):
    /// where a cancelled open dialog leaves it.
    fn settled(&self) -> VcardImport {
        match self {
            Self::Showing(state) | Self::Choosing { before: state, .. } => state.clone(),
            Self::Reading { .. } => VcardImport::Empty,
        }
    }
}

impl QuillApp {
    /// Show the import box with no file chosen.
    pub(super) fn open_import_contacts_dialog(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dialogs.import_contacts_dialog = Some(ImportContactsDialog::new());
        self.open_box(crate::ui::shell::DialogKind::ImportContacts);
        if self.live.is_none()
            && let Some(path) = std::env::var_os("QUILL_DEMO_VCF")
        {
            self.read_vcard(PathBuf::from(path), cx);
        }
        cx.notify();
    }

    /// "Choose File…": the system open dialog, then the read. A second click
    /// while the dialog is open does nothing.
    pub(super) fn choose_vcard_file(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.import_contacts_dialog.as_mut() else {
            return;
        };
        if matches!(dialog.stage, ImportStage::Choosing { .. }) {
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(tr(keys::Q_PEOPLE_IMPORT_CONTACTS).into()),
        });
        let task = self.spawn_screen(cx, async move |this, cx| {
            let path = match picker.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            let _ = this.update(cx, |this, cx| this.vcard_file_chosen(path, cx));
        });
        let Some(dialog) = self.dialogs.import_contacts_dialog.as_mut() else {
            return;
        };
        let before = dialog.stage.settled();
        dialog.stage = ImportStage::Choosing {
            before,
            _picker: task,
        };
        cx.notify();
    }

    /// The open dialog answered: the chosen file is read, or the box shows
    /// what it showed before. The task of any other stage was dropped with
    /// it, so only the picker of the stage in place gets here.
    fn vcard_file_chosen(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialogs.import_contacts_dialog.as_mut() else {
            return;
        };
        let ImportStage::Choosing { before, .. } = std::mem::take(&mut dialog.stage) else {
            return;
        };
        match path {
            Some(path) => self.read_vcard(path, cx),
            None => {
                dialog.stage = ImportStage::Showing(before);
                cx.notify();
            }
        }
    }

    /// Read `path` off the UI thread; the box shows "Reading…" until the
    /// answer lands. Closing the box or choosing again drops the read.
    fn read_vcard(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let file_name = display_name(&path);
        let read = self.spawn_screen(cx, async move |this, cx| {
            let state = cx
                .background_executor()
                .spawn(async move { read_vcard_file(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(dialog) = this.dialogs.import_contacts_dialog.as_mut() {
                    dialog.stage = ImportStage::Showing(state);
                    cx.notify();
                }
            });
        });
        let Some(dialog) = self.dialogs.import_contacts_dialog.as_mut() else {
            return;
        };
        dialog.stage = ImportStage::Reading {
            file_name,
            _read: read,
        };
        cx.notify();
    }

    /// Send `importContacts` for what the box shows. The Import button is
    /// disabled until a file is ready, so an empty state only closes.
    pub(super) fn submit_import_contacts_dialog(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.dialogs.import_contacts_dialog.as_ref() else {
            return;
        };
        let (contacts, skipped, truncated) = match &dialog.stage {
            ImportStage::Showing(VcardImport::Ready(preview)) => {
                (preview.contacts.clone(), preview.skipped, preview.truncated)
            }
            _ => return,
        };
        if let Some(live) = self.live.as_mut() {
            match live.driver.import_contacts(&contacts) {
                Ok(Some(_)) => {
                    self.set_note(if skipped == 0 && truncated == 0 {
                        StatusNote::silent(tr(keys::Q_PEOPLE_IMPORTING_CONTACTS))
                    } else {
                        let mut details = Vec::new();
                        if skipped > 0 {
                            details.push(trn(keys::Q_PEOPLE_IMPORT_SKIPPED, skipped));
                        }
                        if truncated > 0 {
                            details.push(trn_with(
                                keys::Q_PEOPLE_IMPORT_TRUNCATED,
                                truncated,
                                &[("limit", &VCARD_IMPORT_LIMIT.to_string())],
                            ));
                        }
                        StatusNote::silent(tr_with(
                            keys::Q_PEOPLE_IMPORTING_CONTACTS_2,
                            &[("join", &details.join(", "))],
                        ))
                    });
                }
                Ok(None) => {
                    self.set_note(StatusNote::silent(tr(
                        keys::Q_PEOPLE_REQUEST_ALREADY_IN_FLIGHT,
                    )));
                    cx.notify();
                    return;
                }
                Err(err) => {
                    self.set_note(StatusNote::error(user_error_for(
                        keys::Q_NOTES_FAILED_IMPORT_CONTACTS,
                        &err,
                    )));
                    cx.notify();
                    return;
                }
            }
        }
        self.dialogs.import_contacts_dialog = None;
        cx.notify();
    }

    /// Dismiss the import box.
    pub(super) fn close_import_contacts_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialogs.import_contacts_dialog = None;
        cx.notify();
    }

    /// The box's content for `state`.
    pub(super) fn import_contacts_body(
        state: &VcardImport,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let choose = |id: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .label(label)
                .on_click(cx.listener(|this, _, _, cx| this.choose_vcard_file(cx)))
        };
        let note = |text: String, color: Rgba| div().text_sm().text_color(color).child(text);
        let mut body = div().flex().flex_col().gap_3();
        match state {
            VcardImport::Empty => {
                body = body
                    .child(note(
                        tr(keys::Q_PEOPLE_IMPORT_CHOOSE_TEXT).to_string(),
                        text_muted(),
                    ))
                    .child(div().flex().child(choose(
                        "import-contacts-choose",
                        tr(keys::Q_PEOPLE_IMPORT_CHOOSE_FILE),
                        cx,
                    )));
            }
            VcardImport::Reading { file_name } => {
                body = body.child(note(
                    tr_with(keys::Q_PEOPLE_IMPORT_READING, &[("file", file_name)]),
                    text_muted(),
                ));
            }
            VcardImport::Failed { file_name, error } => {
                body = body
                    .child(note(failure_text(file_name, *error), danger()))
                    .child(div().flex().child(choose(
                        "import-contacts-choose",
                        tr(keys::Q_PEOPLE_IMPORT_CHOOSE_OTHER),
                        cx,
                    )));
            }
            VcardImport::Ready(preview) => {
                body = body
                    .child(note(ready_text(preview), text_primary()))
                    .child(contact_preview(&preview.contacts))
                    .child(
                        div().flex().flex_col().gap_1().children(
                            skipped_lines(preview)
                                .into_iter()
                                .map(|line| note(line, text_muted())),
                        ),
                    )
                    .child(
                        div().flex().child(
                            choose(
                                "import-contacts-choose",
                                tr(keys::Q_PEOPLE_IMPORT_CHOOSE_OTHER),
                                cx,
                            )
                            .ghost(),
                        ),
                    );
            }
        }
        body
    }
}

fn failure_text(file_name: &str, error: VcardError) -> String {
    let key = match error {
        VcardError::NotVcard => keys::Q_PEOPLE_IMPORT_NOT_VCARD,
        VcardError::Unreadable => keys::Q_PEOPLE_IMPORT_UNREADABLE,
        VcardError::TooLarge => keys::Q_PEOPLE_IMPORT_TOO_LARGE,
        VcardError::NoContacts => return tr(keys::Q_PEOPLE_NO_PHONE_NUMBER_CONTACTS_FOUND).into(),
    };
    tr_with(key, &[("file", file_name)])
}

fn ready_text(preview: &VcardPreview) -> String {
    trn_with(
        keys::Q_PEOPLE_IMPORT_READY,
        preview.contacts.len(),
        &[("file", &preview.file_name)],
    )
}

fn skipped_lines(preview: &VcardPreview) -> Vec<String> {
    let mut lines = Vec::new();
    if preview.skipped > 0 {
        lines.push(trn(keys::Q_PEOPLE_IMPORT_SKIPPED, preview.skipped));
    }
    if preview.truncated > 0 {
        lines.push(trn_with(
            keys::Q_PEOPLE_IMPORT_TRUNCATED,
            preview.truncated,
            &[("limit", &VCARD_IMPORT_LIMIT.to_string())],
        ));
    }
    lines
}

/// The first contacts, one line each: name and number.
fn contact_preview(contacts: &[ImportedContact]) -> impl IntoElement {
    let mut list = div()
        .flex()
        .flex_col()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(border());
    for contact in contacts.iter().take(PREVIEW_ROWS) {
        let name = format!("{} {}", contact.first_name, contact.last_name);
        let name = name.trim();
        let name = if name.is_empty() {
            tr(keys::Q_PEOPLE_IMPORT_UNNAMED).to_string()
        } else {
            name.to_string()
        };
        list = list.child(
            div()
                .flex()
                .justify_between()
                .gap_3()
                .text_sm()
                .child(div().min_w_0().truncate().child(name))
                .child(
                    div()
                        .flex_none()
                        .text_color(text_muted())
                        .child(contact.phone_number.clone()),
                ),
        );
    }
    let more = contacts.len().saturating_sub(PREVIEW_ROWS);
    if more > 0 {
        list = list.child(
            div()
                .text_sm()
                .text_color(text_muted())
                .child(trn(keys::Q_PEOPLE_IMPORT_MORE, more)),
        );
    }
    list
}

#[cfg(test)]
mod tests {
    use super::skipped_lines;
    use quill::vcard_import::VcardPreview;

    fn preview(skipped: usize, truncated: usize) -> VcardPreview {
        VcardPreview {
            file_name: "a.vcf".into(),
            contacts: Vec::new(),
            skipped,
            truncated,
        }
    }

    #[test]
    fn only_the_dropped_cards_get_a_line() {
        assert!(skipped_lines(&preview(0, 0)).is_empty());
        assert_eq!(skipped_lines(&preview(2, 0)).len(), 1);
        assert_eq!(skipped_lines(&preview(2, 3)).len(), 2);
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "contacts_import_tests.rs"]
mod ui_tests;

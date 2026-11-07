//! The composer's Add Link, Choose Date and Code Language boxes over the
//! window (tdesktop `EditLinkBox`, the date box in
//! chat_helpers/message_field.cpp and `EditCodeLanguageBox`). They
//! used to be panels pushed in above the composer with an accent border;
//! now they are kit dialogs with Cancel and Save pinned underneath.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::dialog_focus::focus_after_mount;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// tdesktop `boxWidth`.
const BOX_WIDTH: f32 = 380.;

impl QuillApp {
    fn build_composer_link_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ComposerLink, |this, window, cx| {
                this.close_composer_link_dialog(window, cx);
            });
        let footer = app.update(cx, |this, cx| this.composer_link_footer(cx));
        let body_app = app.clone();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(tr(keys::Q_CHAT_ADD_LINK)))
            .content(move |content, window, cx| {
                let body = body_app.update(cx, |this, cx| {
                    let focus = this.take_link_focus();
                    focus_after_mount(focus, window, cx);
                    this.composer_link_dialog_panel(cx)
                });
                content.children(body)
            })
            .footer(footer)
            .on_close(on_close)
    }

    fn build_composer_date_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::ComposerDate, |this, window, cx| {
                this.close_composer_date_dialog(window, cx);
            });
        let footer = app.update(cx, |this, cx| this.composer_date_footer(cx));
        let body_app = app.clone();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(tr(
                keys::FORMATTING_DATE_TITLE,
            )))
            .content(move |content, _, cx| {
                content
                    .children(body_app.update(cx, |this, cx| this.composer_date_dialog_panel(cx)))
            })
            .footer(footer)
            .on_close(on_close)
    }

    fn build_code_language_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close = QuillShell::on_close_kind(
            app,
            shell,
            DialogKind::ComposerCodeLanguage,
            |this, window, cx| this.close_code_language_dialog(window, cx),
        );
        let footer = app.update(cx, |this, cx| this.code_language_footer(cx));
        let body_app = app.clone();
        dialog
            .overlay(true)
            .w(px(BOX_WIDTH))
            .title(crate::ui::shell::dialog_title(tr(
                keys::Q_CHAT_CODE_LANGUAGE,
            )))
            .content(move |content, window, cx| {
                let body = body_app.update(cx, |this, cx| {
                    // Focus with the current language selected, so typing
                    // replaces it.
                    if let Some(input) = this.take_code_language_focus() {
                        window.defer(cx, move |window, cx| {
                            input.update(cx, |input, cx| {
                                input.focus(window, cx);
                                input.select_all(window, cx);
                            });
                        });
                    }
                    this.code_language_panel(cx)
                });
                content.children(body)
            })
            .footer(footer)
            .on_close(on_close)
    }
}

crate::ui::shell::register_dialogs! {
    ComposerLink => DialogSpec::new(
        2940,
        |app| app.composer_ui.link_dialog.is_some(),
        QuillApp::build_composer_link_dialog,
    )
    .closes(|app, window, cx| app.close_composer_link_dialog(window, cx)),
    ComposerDate => DialogSpec::new(
        2941,
        |app| app.composer_ui.date_dialog.is_some(),
        QuillApp::build_composer_date_dialog,
    )
    .closes(|app, window, cx| app.close_composer_date_dialog(window, cx)),
    ComposerCodeLanguage => DialogSpec::new(
        2946,
        |app| app.composer_ui.code_language.is_some(),
        QuillApp::build_code_language_dialog,
    )
    .closes(|app, window, cx| app.close_code_language_dialog(window, cx)),
}

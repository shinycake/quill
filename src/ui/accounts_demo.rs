//! Screenshot demos of the Accounts dialog: the list with three accounts,
//! and a row being renamed.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::settings::{add_account, list_accounts, safe_app_root};

/// Three accounts in the demo's throwaway data root, the dialog open.
fn open_with_accounts(app: &mut QuillApp, cx: &mut Context<QuillApp>) {
    if let Some(root) = safe_app_root() {
        for name in ["Work", "Family"] {
            let _ = add_account(&root, name);
        }
    }
    app.open_accounts(cx);
}

register_demos![
    DemoSpec::chat_list("ready-accounts").setup(|app, _, cx| open_with_accounts(app, cx)),
    DemoSpec::chat_list("ready-accounts-rename").setup(|app, window, cx| {
        open_with_accounts(app, cx);
        let work = safe_app_root().and_then(|root| list_accounts(&root).into_iter().nth(1));
        if let Some(record) = work {
            app.start_account_rename(&record, window, cx);
        }
    }),
];

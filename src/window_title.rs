//! The main window's title, after `Window::MainWindow::updateTitle`
//! (`window/main_window.cpp`): the open chat's name with its own unread
//! count in front, then the account-wide total.

const APP_NAME: &str = "Quill";

/// The chat shown in the window, if any.
#[derive(Debug, Clone, Copy)]
pub struct TitleChat<'a> {
    pub name: &'a str,
    /// The chat's own unread count.
    pub unread: u32,
}

/// `Name`, `(3) Name – (12)` and so on. `account` is only passed with more
/// than one signed-in account (tdesktop prints `Name @ Account`).
pub fn window_title(
    chat: Option<TitleChat<'_>>,
    total_unread: u32,
    account: Option<&str>,
) -> String {
    let added = if total_unread > 0 {
        format!(" ({total_unread})")
    } else {
        String::new()
    };
    let account = account.map(single_line).filter(|name| !name.is_empty());
    let Some(chat) = chat else {
        return format!("{}{added}", account.as_deref().unwrap_or(APP_NAME));
    };
    let name = single_line(chat.name);
    let primary = if chat.unread > 0 {
        format!("({}) {name}", chat.unread)
    } else {
        name
    };
    let middle = match &account {
        Some(account) => format!(" @ {account}"),
        None if !added.is_empty() => " \u{2013}".to_string(),
        None => String::new(),
    };
    format!("{primary}{middle}{added}")
}

/// Title bars are one line: control characters become spaces.
fn single_line(text: &str) -> String {
    text.split(|c: char| c.is_control())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{TitleChat, window_title};

    fn chat(name: &str, unread: u32) -> Option<TitleChat<'_>> {
        Some(TitleChat { name, unread })
    }

    #[test]
    fn no_chat_is_the_app_name_with_the_total() {
        assert_eq!(window_title(None, 0, None), "Quill");
        assert_eq!(window_title(None, 7, None), "Quill (7)");
    }

    #[test]
    fn open_chat_leads_with_its_name_and_own_count() {
        assert_eq!(window_title(chat("Ada", 0), 0, None), "Ada");
        assert_eq!(window_title(chat("Ada", 3), 0, None), "(3) Ada");
    }

    #[test]
    fn total_follows_after_a_dash() {
        assert_eq!(window_title(chat("Ada", 0), 12, None), "Ada \u{2013} (12)");
        assert_eq!(
            window_title(chat("Ada", 3), 12, None),
            "(3) Ada \u{2013} (12)"
        );
    }

    #[test]
    fn several_accounts_name_the_account() {
        assert_eq!(window_title(None, 0, Some("Work")), "Work");
        assert_eq!(window_title(None, 2, Some("Work")), "Work (2)");
        assert_eq!(
            window_title(chat("Ada", 1), 2, Some("Work")),
            "(1) Ada @ Work (2)"
        );
    }

    #[test]
    fn names_stay_on_one_line() {
        assert_eq!(
            window_title(chat("Ada\nLovelace\t", 0), 0, None),
            "Ada Lovelace"
        );
        assert_eq!(window_title(None, 0, Some("\n")), "Quill");
    }
}

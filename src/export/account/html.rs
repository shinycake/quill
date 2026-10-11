//! The account export's HTML outside the chats: `export_results.html`
//! (personal information and one link per section, in tdesktop's order)
//! and the `lists/` pages for profile pictures, stories, profile music,
//! contacts, frequent contacts, sessions, web sessions and chats.
use super::dialogs::Dialog;
use super::sections::{
    Contact, PersonalInfo, SavedPhoto, Session, Story, TopPeer, Track, WebSession,
};
use crate::export::html::{
    Outcome, PAGE_END, full_stamp, human_size, noun_skip_note, page_start, url_path,
};
use crate::export::text::{escape, render};
use crate::export::userpic::{self, Userpic};

pub const INDEX: &str = "export_results.html";
pub const USERPICS: &str = "lists/profile_pictures.html";
pub const STORIES: &str = "lists/stories.html";
pub const MUSIC: &str = "lists/profile_music.html";
pub const CONTACTS: &str = "lists/contacts.html";
pub const FREQUENT: &str = "lists/frequent.html";
pub const SESSIONS: &str = "lists/sessions.html";
pub const WEB_SESSIONS: &str = "lists/web_sessions.html";
pub const CHATS: &str = "lists/chats.html";

/// Lists sit one folder down from the export root.
const LIST_BASE: &str = "../";

pub const ABOUT_EXPORT: &str = "This is the data you chose to export from your Telegram account \
     with Quill. The settings for most of it are in Settings › Privacy & Security in Telegram's \
     mobile apps.";
pub const ABOUT_CONTACTS: &str = "Phone contacts synced with your Telegram account. You can stop \
     syncing them, or delete the ones stored, in Settings › Privacy & Security on mobile devices.";
pub const ABOUT_CONTACTS_PARTIAL: &str = "Your Telegram contacts. The phone contacts synced with \
     your account need a Telegram takeout session, which was not available for this export, so \
     the dates they were added are missing.";
pub const ABOUT_FREQUENT: &str = "The people, inline bots and calls you use most, in order. \
     Telegram uses this for the row of people at the top of search; turn off Suggest Frequent \
     Contacts in Settings › Privacy & Security to delete it.";
pub const ABOUT_SESSIONS: &str = "The devices logged in to your account, as listed in Active \
     sessions.";
pub const ABOUT_WEB_SESSIONS: &str = "Websites where you logged in with Telegram, as listed in \
     Active sessions.";
pub const ABOUT_CHATS: &str = "This page lists all chats from this export.";

/// One link on the index page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionLink {
    /// tdesktop's order: chats 0, contacts 2, frequent 3, profile pictures
    /// 4, stories 5, music 6, sessions 7, web sessions 8.
    pub priority: u8,
    pub label: &'static str,
    pub count: usize,
    pub path: &'static str,
}

fn about(text: &str) -> String {
    format!("<div class=\"page_about\">{}</div>\n", escape(text))
}

fn list_page_start(title: &str, about_text: Option<&str>) -> String {
    let mut out = page_start(title, LIST_BASE, Some(INDEX));
    out.push_str("</header>\n");
    if let Some(text) = about_text {
        out.push_str(&about(text));
    }
    out.push_str("<div class=\"entry_list\">\n");
    out
}

fn list_page_end() -> String {
    format!("</div>\n{PAGE_END}")
}

/// A list row: picture, name, detail lines and a right-hand note, linking
/// to `link` (relative to the list page) when given.
fn entry(picture: &str, name: &str, details: &[String], info: &str, link: Option<&str>) -> String {
    entry_with(picture, name, "", details, info, link)
}

/// [`entry`] with ready HTML (a story's caption) under the name.
fn entry_with(
    picture: &str,
    name: &str,
    under_name: &str,
    details: &[String],
    info: &str,
    link: Option<&str>,
) -> String {
    let (open, close) = match link {
        Some(href) => (
            format!("<a class=\"entry\" href=\"{}#allow_back\">", escape(href)),
            "</a>",
        ),
        None => ("<div class=\"entry\">".to_string(), "</div>"),
    };
    let mut out = format!("{open}<div class=\"userpic_wrap\">{picture}</div><div class=\"body\">");
    if !info.is_empty() {
        out.push_str(&format!(
            "<div class=\"info details\">{}</div>",
            escape(info)
        ));
    }
    if !name.is_empty() {
        out.push_str(&format!("<div class=\"name bold\">{}</div>", escape(name)));
    }
    out.push_str(under_name);
    for detail in details.iter().filter(|d| !d.is_empty()) {
        out.push_str(&format!(
            "<div class=\"details_entry details\">{}</div>",
            escape(detail)
        ));
    }
    out.push_str(&format!("</div>{close}\n"));
    out
}

fn initials_picture(color: u8, name: &str, one_letter: bool) -> String {
    Userpic {
        color,
        size: userpic::ENTRY_SIZE,
        name,
        one_letter,
        ..Default::default()
    }
    .render()
}

/// The saved file's link from a list page, or none.
fn saved_link(outcome: &Outcome) -> Option<String> {
    match outcome {
        Outcome::Saved { path, .. } => Some(format!("{LIST_BASE}{}", url_path(path))),
        Outcome::Skipped(_) => None,
    }
}

fn status(noun: &str, outcome: &Outcome, size: i64) -> String {
    match outcome {
        Outcome::Saved { .. } => human_size(size),
        Outcome::Skipped(skip) => noun_skip_note(noun, *skip),
    }
}

/// `export_results.html`. `photo` is the newest saved profile picture, as
/// a path relative to the root.
pub fn index_page(
    personal: Option<&PersonalInfo>,
    photo: Option<&str>,
    has_userpics: bool,
    sections: &[SectionLink],
) -> String {
    let mut out = page_start("Exported Data", "", None);
    out.push_str("</header>\n");
    if let Some(info) = personal {
        let src = photo.map(url_path);
        let name = format!("{} {}", info.first_name, info.last_name);
        let picture = Userpic {
            color: userpic::color_index(info.user_id),
            size: userpic::PERSONAL_SIZE,
            name: &name,
            image: src.as_deref(),
            link: has_userpics.then_some(USERPICS),
            one_letter: false,
        }
        .render();
        out.push_str(&format!(
            "<div class=\"personal_info\"><div class=\"userpic_wrap\">{picture}</div><div class=\"rows\">\n"
        ));
        for (label, value) in [
            ("First name", &info.first_name),
            ("Last name", &info.last_name),
            ("Phone number", &info.phone),
            ("Username", &info.username),
            ("Bio", &info.bio),
        ] {
            if !value.is_empty() {
                out.push_str(&format!(
                    "<div class=\"row\"><div class=\"label details\">{label}</div><div class=\"value bold\">{}</div></div>\n",
                    escape(value)
                ));
            }
        }
        out.push_str("</div></div>\n");
    }
    let mut sorted: Vec<&SectionLink> = sections.iter().collect();
    sorted.sort_by_key(|s| s.priority);
    if !sorted.is_empty() {
        out.push_str("<div class=\"sections\">\n");
        for section in sorted {
            out.push_str(&format!(
                "<a class=\"section\" href=\"{}#allow_back\"><div class=\"label bold\">{}</div><div class=\"counter details\">{}</div></a>\n",
                section.path, section.label, section.count
            ));
        }
        out.push_str("</div>\n");
    }
    out.push_str(&format!(
        "<div class=\"page_about with_divider\">{}</div>\n{PAGE_END}",
        escape(ABOUT_EXPORT)
    ));
    out
}

pub fn userpics_page(list: &[SavedPhoto], offset_at: &dyn Fn(i64) -> i32) -> String {
    let mut out = list_page_start("Profile pictures", None);
    for photo in list {
        let link = saved_link(&photo.outcome);
        let name = match &photo.outcome {
            Outcome::Saved { path, .. } => path.clone(),
            Outcome::Skipped(_) => "Photo unavailable".into(),
        };
        let picture = match &link {
            Some(src) => Userpic {
                size: userpic::ENTRY_SIZE,
                image: Some(src),
                ..Default::default()
            }
            .render(),
            None => initials_picture(0, "", false),
        };
        let when = if photo.date > 0 {
            full_stamp(photo.date, offset_at(i64::from(photo.date)))
        } else {
            String::new()
        };
        out.push_str(&entry(
            &picture,
            &name,
            &[status("Photo", &photo.outcome, photo.size)],
            &when,
            link.as_deref(),
        ));
    }
    out.push_str(&list_page_end());
    out
}

pub fn stories_page(list: &[Story], offset_at: &dyn Fn(i64) -> i32) -> String {
    let mut out = list_page_start("Stories archive", None);
    for story in list {
        let link = saved_link(&story.outcome);
        let (name, preview) = match &story.outcome {
            Outcome::Saved { path, thumb } => {
                let image = thumb.as_deref().unwrap_or(path);
                let is_still = image.ends_with(".jpg") || image.ends_with(".png");
                (
                    path.clone(),
                    is_still.then(|| format!("{LIST_BASE}{}", url_path(image))),
                )
            }
            Outcome::Skipped(_) => ("Story unavailable".into(), None),
        };
        let picture = preview.map_or(String::new(), |src| {
            format!("<img class=\"story\" src=\"{}\" alt=\"\">", escape(&src))
        });
        let mut details = Vec::new();
        if story.pinned {
            details.push("Saved to Profile".to_string());
        }
        details.push(status("Story", &story.outcome, story.size));
        let when = if story.date > 0 {
            full_stamp(story.date, offset_at(i64::from(story.date)))
        } else {
            String::new()
        };
        let caption = if story.caption.is_empty() {
            String::new()
        } else {
            format!(
                "<div class=\"text\">{}</div>",
                render(&story.caption, &story.entities)
            )
        };
        out.push_str(&entry_with(
            &picture,
            &name,
            &caption,
            &details,
            &when,
            link.as_deref(),
        ));
    }
    out.push_str(&list_page_end());
    out
}

pub fn music_page(list: &[Track]) -> String {
    let mut out = list_page_start("Profile Music", None);
    for track in list {
        let link = saved_link(&track.outcome);
        let title = if !track.title.is_empty() {
            track.title.clone()
        } else if !track.file_name.is_empty() {
            track.file_name.clone()
        } else {
            "Unknown Track".into()
        };
        let performer = if track.performer.is_empty() {
            "Unknown Artist"
        } else {
            &track.performer
        };
        let duration = if track.duration > 0 {
            format!("{}:{:02}", track.duration / 60, track.duration % 60)
        } else {
            String::new()
        };
        out.push_str(&entry(
            "<div class=\"audio_icon\"></div>",
            &format!("{performer} - {title}"),
            &[status("File", &track.outcome, track.size)],
            &duration,
            link.as_deref(),
        ));
    }
    out.push_str(&list_page_end());
    out
}

pub fn contacts_page(list: &[Contact], complete: bool, offset_at: &dyn Fn(i64) -> i32) -> String {
    let about_text = if complete {
        ABOUT_CONTACTS
    } else {
        ABOUT_CONTACTS_PARTIAL
    };
    let mut out = list_page_start("Contacts", Some(about_text));
    for contact in list {
        let name = format!("{} {}", contact.first_name, contact.last_name)
            .trim()
            .to_string();
        let name = if name.is_empty() {
            "Deleted Account".to_string()
        } else {
            name
        };
        let color = userpic::color_index(contact.user_id);
        let when = if contact.date > 0 {
            full_stamp(contact.date, offset_at(i64::from(contact.date)))
        } else {
            String::new()
        };
        out.push_str(&entry(
            &initials_picture(color, &name, false),
            &name,
            std::slice::from_ref(&contact.phone),
            &when,
            None,
        ));
    }
    out.push_str(&list_page_end());
    out
}

pub fn frequent_page(list: &[TopPeer]) -> String {
    let mut out = list_page_start("Frequent contacts", Some(ABOUT_FREQUENT));
    for peer in list {
        let color = userpic::color_index(userpic::bare_chat_id(peer.chat_id));
        out.push_str(&entry(
            &initials_picture(color, &peer.name, false),
            &peer.name,
            &[],
            peer.category,
            None,
        ));
    }
    out.push_str(&list_page_end());
    out
}

fn stamp(date: i32, offset_at: &dyn Fn(i64) -> i32) -> String {
    if date > 0 {
        full_stamp(date, offset_at(i64::from(date)))
    } else {
        String::new()
    }
}

pub fn sessions_page(list: &[Session], offset_at: &dyn Fn(i64) -> i32) -> String {
    let mut out = list_page_start("Sessions", Some(ABOUT_SESSIONS));
    for session in list {
        let app = if session.application.is_empty() {
            "Unknown"
        } else {
            &session.application
        };
        let name = format!("{app} {}", session.version).trim().to_string();
        let device = format!(
            "{}, {} {}",
            session.device, session.platform, session.system
        );
        let color = userpic::color_index(session.api_id);
        out.push_str(&entry(
            &initials_picture(color, &name, true),
            &name,
            &[
                device.trim().to_string(),
                format!("{} \u{2013} {}", session.ip, session.location),
                format!("Last active: {}", stamp(session.last_active, offset_at)),
                format!("Created: {}", stamp(session.created, offset_at)),
            ],
            "",
            None,
        ));
    }
    out.push_str(&list_page_end());
    out
}

pub fn web_sessions_page(list: &[WebSession], offset_at: &dyn Fn(i64) -> i32) -> String {
    let mut out = list_page_start("Web sessions", Some(ABOUT_WEB_SESSIONS));
    for site in list {
        let name = if site.domain.is_empty() {
            "Unknown"
        } else {
            &site.domain
        };
        let color = userpic::color_index(name.bytes().map(i64::from).sum());
        out.push_str(&entry(
            &initials_picture(color, name, true),
            name,
            &[
                format!("{}, {}", site.platform, site.browser),
                format!("{} \u{2013} {}", site.ip, site.location),
                format!("Last active: {}", stamp(site.last_active, offset_at)),
                format!("Created: {}", stamp(site.created, offset_at)),
            ],
            &site.bot,
            None,
        ));
    }
    out.push_str(&list_page_end());
    out
}

/// The start of `lists/chats.html`; rows follow as each chat finishes.
pub fn chats_page_start() -> String {
    list_page_start("Chats", Some(ABOUT_CHATS))
}

pub fn chats_page_end() -> String {
    list_page_end()
}

/// tdesktop's message count wording in the chats list.
fn count_line(count: usize, only_my: bool) -> String {
    match (count, only_my) {
        (0, false) => "No messages".into(),
        (0, true) => "No outgoing messages".into(),
        (1, false) => "1 message".into(),
        (1, true) => "1 outgoing message".into(),
        (n, false) => format!("{n} messages"),
        (n, true) => format!("{n} outgoing messages"),
    }
}

/// One row of the chats list, linking to the chat's first page when it
/// has messages.
pub fn chat_row(dialog: &Dialog, messages: usize) -> String {
    use super::dialogs::ChatKind;
    let color = if dialog.kind == ChatKind::SavedMessages {
        3
    } else {
        userpic::color_index(userpic::bare_chat_id(dialog.chat_id))
    };
    let link = (messages > 0).then(|| format!("{LIST_BASE}{}messages.html", dialog.folder));
    entry(
        &initials_picture(
            color,
            &dialog.name,
            !matches!(dialog.kind, ChatKind::Personal | ChatKind::Bot),
        ),
        &dialog.name,
        &[count_line(messages, dialog.only_my)],
        dialog.kind.list_type(),
        link.as_deref(),
    )
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;

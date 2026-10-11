//! Rendering of the index and list pages, without TDLib.
use super::*;
use crate::export::account::dialogs::ChatKind;
use crate::export::settings::Skip;

fn link(priority: u8, label: &'static str, count: usize, path: &'static str) -> SectionLink {
    SectionLink {
        priority,
        label,
        count,
        path,
    }
}

#[test]
fn the_index_shows_personal_info_and_sorted_sections() {
    let info = PersonalInfo {
        user_id: 7,
        first_name: "Ann <A>".into(),
        last_name: String::new(),
        phone: "+1555".into(),
        username: "@ann".into(),
        bio: String::new(),
    };
    let html = index_page(
        Some(&info),
        Some("profile_pictures/photo 1.jpg"),
        true,
        &[
            link(7, "Sessions", 2, SESSIONS),
            link(0, "Chats", 30, CHATS),
            link(2, "Contacts", 5, CONTACTS),
        ],
    );
    assert!(html.contains("<title>Exported Data</title>"));
    assert!(html.contains("href=\"css/style.css\""));
    assert!(
        !html.contains("class=\"back\""),
        "the index has no back link"
    );
    assert!(html.contains("Ann &lt;A&gt;"));
    // Empty rows are left out.
    assert!(!html.contains("Last name") && !html.contains(">Bio<"));
    assert!(html.contains(
        "<a class=\"userpic_link\" href=\"lists/profile_pictures.html\"><img class=\"userpic\" src=\"profile_pictures/photo%201.jpg\""
    ));
    let chats = html.find("lists/chats.html").unwrap();
    let contacts = html.find("lists/contacts.html").unwrap();
    let sessions = html.find("lists/sessions.html").unwrap();
    assert!(chats < contacts && contacts < sessions);
    assert!(html.contains("<div class=\"counter details\">30</div>"));
    assert!(html.contains(&escape(ABOUT_EXPORT)));
}

#[test]
fn an_index_without_photos_shows_initials() {
    let info = PersonalInfo {
        user_id: 7,
        first_name: "Ann".into(),
        last_name: "Lee".into(),
        ..Default::default()
    };
    let html = index_page(Some(&info), None, false, &[]);
    assert!(html.contains(">AL</div>"));
    assert!(!html.contains("userpic_link"));
    assert!(!html.contains("class=\"sections\""));
}

#[test]
fn list_pages_lead_back_to_the_index_from_one_folder_down() {
    let html = sessions_page(
        &[Session {
            api_id: 2040,
            application: String::new(),
            version: "1.0".into(),
            device: "Phone".into(),
            platform: "Android".into(),
            system: "14".into(),
            ip: "203.0.113.1".into(),
            location: "Testland".into(),
            last_active: 1_700_000_000,
            created: 1_690_000_000,
        }],
        &|_| 0,
    );
    assert!(html.contains("href=\"../css/style.css\""));
    assert!(html.contains("<a class=\"back\" href=\"../export_results.html\""));
    assert!(html.contains("Unknown 1.0"));
    assert!(html.contains("203.0.113.1 \u{2013} Testland"));
    assert!(html.contains("Last active: 14.11.2023 22:13:20"));
    assert!(html.contains(&escape(ABOUT_SESSIONS)));
}

#[test]
fn profile_pictures_link_their_files_and_explain_skips() {
    let html = userpics_page(
        &[
            SavedPhoto {
                date: 1_700_000_000,
                size: 2048,
                outcome: Outcome::Saved {
                    path: "profile_pictures/photo_1@x.jpg".into(),
                    thumb: None,
                },
            },
            SavedPhoto {
                date: 0,
                size: 1,
                outcome: Outcome::Skipped(Skip::TooLarge),
            },
        ],
        &|_| 0,
    );
    assert!(html.contains("href=\"../profile_pictures/photo_1@x.jpg#allow_back\""));
    assert!(html.contains("<img class=\"userpic\" src=\"../profile_pictures/photo_1@x.jpg\""));
    assert!(html.contains("2.0 KB"));
    assert!(html.contains("Photo unavailable"));
    assert!(html.contains("(Photo exceeds maximum size."));
}

#[test]
fn contacts_explain_when_they_are_partial() {
    let contact = Contact {
        user_id: 0,
        first_name: String::new(),
        last_name: String::new(),
        phone: "+1".into(),
        date: 0,
    };
    let full = contacts_page(std::slice::from_ref(&contact), true, &|_| 0);
    assert!(full.contains(&escape(ABOUT_CONTACTS)));
    assert!(full.contains("Deleted Account"));
    let partial = contacts_page(&[contact], false, &|_| 0);
    assert!(partial.contains(&escape(ABOUT_CONTACTS_PARTIAL)));
}

#[test]
fn chat_rows_count_messages_like_tdesktop() {
    let dialog = |kind, only_my| Dialog {
        chat_id: -1_000_000_000_042,
        kind,
        name: "Team & co".into(),
        only_my,
        folder: "chats/chat_03/".into(),
    };
    let row = chat_row(&dialog(ChatKind::PrivateSupergroup, true), 1);
    assert!(row.contains("href=\"../chats/chat_03/messages.html#allow_back\""));
    assert!(row.contains("1 outgoing message"));
    assert!(row.contains(">group<"));
    assert!(row.contains("Team &amp; co"));
    let empty = chat_row(&dialog(ChatKind::PublicChannel, false), 0);
    assert!(empty.starts_with("<div class=\"entry\">"));
    assert!(empty.contains("No messages") && empty.contains(">channel<"));
    let many = chat_row(&dialog(ChatKind::Personal, false), 12);
    assert!(many.contains("12 messages") && many.contains(">private<"));
    let saved = chat_row(&dialog(ChatKind::SavedMessages, false), 1);
    assert!(saved.contains("class=\"userpic userpic4\""));
    // Groups and channels show one letter, people two.
    assert!(row.contains(">T</div>"));
    let mut person = dialog(ChatKind::Personal, false);
    person.name = "Bo Park".into();
    assert!(chat_row(&person, 1).contains(">BP</div>"));
}

#[test]
fn stories_and_music_show_their_details() {
    let stories = stories_page(
        &[Story {
            date: 1_700_000_000,
            pinned: true,
            size: 10,
            outcome: Outcome::Saved {
                path: "stories/story_1.mp4".into(),
                thumb: Some("stories/story_1.mp4_thumb.jpg".into()),
            },
            caption: "At <sea>".into(),
            entities: Vec::new(),
        }],
        &|_| 0,
    );
    assert!(stories.contains("<img class=\"story\" src=\"../stories/story_1.mp4_thumb.jpg\""));
    assert!(stories.contains("Saved to Profile"));
    assert!(stories.contains("<div class=\"text\">At &lt;sea&gt;</div>"));
    let music = music_page(&[Track {
        performer: String::new(),
        title: String::new(),
        file_name: "track.mp3".into(),
        mime: String::new(),
        duration: 125,
        size: 4,
        outcome: Outcome::Skipped(Skip::NotIncluded),
    }]);
    assert!(music.contains("Unknown Artist - track.mp3"));
    assert!(music.contains("2:05"));
    assert!(music.contains("(File not included."));
}

#[test]
fn frequent_and_web_sessions_render() {
    let top = frequent_page(&[TopPeer {
        chat_id: 5,
        name: "Bo".into(),
        category: "people",
    }]);
    assert!(top.contains(&escape(ABOUT_FREQUENT)) && top.contains(">people<"));
    let web = web_sessions_page(
        &[WebSession {
            domain: "example.org".into(),
            bot: "@login_bot".into(),
            ..Default::default()
        }],
        &|_| 0,
    );
    assert!(web.contains("example.org") && web.contains("@login_bot"));
}

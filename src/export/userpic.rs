//! Round profile pictures in the export pages. Like tdesktop's export
//! (`UserpicData` in `export_output_html.cpp`), a person or chat without a
//! saved photo gets a colored circle with initials; the color follows the
//! peer id, so one person keeps one color across every page.
use super::model::Who;
use super::text::escape;

/// tdesktop's `kHistoryUserpicSize`: next to messages.
pub const HISTORY_SIZE: u32 = 42;
/// tdesktop's `kEntryUserpicSize`: in lists (chats, contacts, sessions).
pub const ENTRY_SIZE: u32 = 48;
/// tdesktop's `kPersonalUserpicSize`: the account owner on the index page.
pub const PERSONAL_SIZE: u32 = 90;

/// TDLib chat ids carry the chat type in their range; this is the plain
/// number the color is based on (tdesktop's `PeerToBareId`).
pub fn bare_chat_id(chat_id: i64) -> i64 {
    const CHANNEL_SHIFT: i64 = 1_000_000_000_000;
    if chat_id <= -CHANNEL_SHIFT {
        -chat_id - CHANNEL_SHIFT
    } else {
        chat_id.abs()
    }
}

pub fn bare_id(who: Who) -> i64 {
    match who {
        Who::User(id) => id,
        Who::Chat(id) => bare_chat_id(id),
    }
}

/// One of eight colors, 0-based, chosen the way tdesktop's
/// `PeerColorIndex` does (seven peer colors in its fixed order).
pub fn color_index(bare: i64) -> u8 {
    const MAP: [u8; 7] = [0, 7, 4, 1, 6, 3, 5];
    MAP[bare.rem_euclid(7) as usize]
}

/// Up to two letters: the first of the first word and of the last word.
pub fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|w| w.chars().next());
    let last = words.last().and_then(|w| w.chars().next());
    first.into_iter().chain(last).collect()
}

/// What one round picture shows.
#[derive(Debug, Clone, Default)]
pub struct Userpic<'a> {
    pub color: u8,
    pub size: u32,
    pub name: &'a str,
    /// A saved picture, as an already encoded relative `src`.
    pub image: Option<&'a str>,
    /// Where clicking the picture leads, encoded and relative.
    pub link: Option<&'a str>,
    /// Groups, channels and Saved Messages show one letter, people two
    /// (tdesktop fills only a first name for them).
    pub one_letter: bool,
}

impl Userpic<'_> {
    pub fn render(&self) -> String {
        let size = self.size;
        let picture = match self.image {
            Some(src) => format!(
                "<img class=\"userpic\" src=\"{}\" style=\"width:{size}px;height:{size}px\" alt=\"\">",
                escape(src)
            ),
            None => format!(
                "<div class=\"userpic userpic{}\" style=\"width:{size}px;height:{size}px\">\
                 <div class=\"initials\" style=\"line-height:{size}px\">{}</div></div>",
                self.color % 8 + 1,
                escape(&if self.one_letter {
                    initials(self.name).chars().take(1).collect()
                } else {
                    initials(self.name)
                })
            ),
        };
        match self.link {
            Some(href) => format!(
                "<a class=\"userpic_link\" href=\"{}\">{picture}</a>",
                escape(href)
            ),
            None => picture,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_ids_strip_the_chat_type() {
        assert_eq!(bare_chat_id(-1_001_234_567_890), 1_234_567_890);
        assert_eq!(bare_chat_id(-4_321), 4_321);
        assert_eq!(bare_chat_id(777), 777);
        assert_eq!(bare_id(Who::User(9)), 9);
        assert_eq!(bare_id(Who::Chat(-1_000_000_000_042)), 42);
    }

    #[test]
    fn colors_follow_tdesktops_order() {
        assert_eq!(
            (0..7).map(color_index).collect::<Vec<_>>(),
            [0, 7, 4, 1, 6, 3, 5]
        );
        assert_eq!(color_index(7), 0);
    }

    #[test]
    fn initials_take_the_first_and_last_word() {
        assert_eq!(initials("Ann Lee"), "AL");
        assert_eq!(initials("  Ann  "), "A");
        assert_eq!(initials("Mary Ann Lee"), "ML");
        assert_eq!(initials("Émile Zola"), "ÉZ");
        assert_eq!(initials(""), "");
    }

    #[test]
    fn placeholders_and_pictures_render_escaped() {
        let plain = Userpic {
            color: 2,
            size: 42,
            name: "<b> Ann",
            ..Default::default()
        }
        .render();
        assert!(plain.contains("class=\"userpic userpic3\""));
        assert!(plain.contains("width:42px;height:42px"));
        assert!(plain.contains(">&lt;A</div>"));
        let photo = Userpic {
            size: 48,
            image: Some("profile_pictures/a.jpg"),
            link: Some("lists/profile_pictures.html"),
            ..Default::default()
        }
        .render();
        assert!(
            photo.starts_with("<a class=\"userpic_link\" href=\"lists/profile_pictures.html\">")
        );
        assert!(photo.contains("<img class=\"userpic\" src=\"profile_pictures/a.jpg\""));
    }
}

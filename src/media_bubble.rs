//! Whether a picture, video, GIF or album that leads a message keeps its
//! bubble. Telegram Desktop drops the bubble when nothing but the media
//! would sit in it (`Photo::needsBubble`, `GroupedMedia::computeNeedBubble`,
//! `Message::drawBubble`): the time then rides on the picture and the
//! reactions go under it, outside any bubble. Kept pure so it is unit
//! tested.

/// What a media-led message shows besides its media.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaSurroundings {
    /// A caption above or below the media.
    pub caption: bool,
    /// A reply, forward or "via @bot" header.
    pub header: bool,
    /// The sender's name inside the bubble (groups).
    pub sender_name: bool,
    /// The comments / replies bar under the bubble.
    pub comments_bar: bool,
    /// Inline keyboard rows.
    pub keyboard: bool,
    /// A fact-check block.
    pub fact_check: bool,
    /// A self-destruct or auto-delete countdown line.
    pub countdown: bool,
}

impl MediaSurroundings {
    /// The media needs a bubble around it: anything but reactions and the
    /// time (which the picture carries) sits with it.
    pub fn needs_bubble(self) -> bool {
        let Self {
            caption,
            header,
            sender_name,
            comments_bar,
            keyboard,
            fact_check,
            countdown,
        } = self;
        caption || header || sender_name || comments_bar || keyboard || fact_check || countdown
    }
}

#[cfg(test)]
mod tests {
    use super::MediaSurroundings;

    #[test]
    fn bare_media_has_no_bubble() {
        assert!(!MediaSurroundings::default().needs_bubble());
    }

    #[test]
    fn anything_beside_the_media_keeps_the_bubble() {
        let each = [
            MediaSurroundings {
                caption: true,
                ..Default::default()
            },
            MediaSurroundings {
                header: true,
                ..Default::default()
            },
            MediaSurroundings {
                sender_name: true,
                ..Default::default()
            },
            MediaSurroundings {
                comments_bar: true,
                ..Default::default()
            },
            MediaSurroundings {
                keyboard: true,
                ..Default::default()
            },
            MediaSurroundings {
                fact_check: true,
                ..Default::default()
            },
            MediaSurroundings {
                countdown: true,
                ..Default::default()
            },
        ];
        for surroundings in each {
            assert!(surroundings.needs_bubble(), "{surroundings:?}");
        }
    }
}

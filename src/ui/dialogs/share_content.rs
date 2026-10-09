use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

/// Which attach-menu item opened the share panel.
pub(crate) enum ShareContentKind {
    /// "Contact": pick an address-book entry to send as a card.
    Contact { query: Entity<TextareaState> },
    /// "Location": type coordinates and send a static location.
    Location {
        latitude: Entity<TextareaState>,
        longitude: Entity<TextareaState>,
    },
}

/// The composer's inline "Send Contact" / "Send Location" panel (the
/// attach menu's contact chooser and location picker). `error` is the
/// validation reason shown under the fields.
pub struct ShareContentDialog {
    pub(crate) kind: ShareContentKind,
    pub(crate) error: Option<&'static str>,
}

fn line(
    window: &mut Window,
    cx: &mut Context<QuillApp>,
    placeholder: &'static str,
) -> Entity<TextareaState> {
    cx.new(|cx| {
        TextareaState::new(window, cx)
            .placeholder(placeholder)
            .auto_grow(1, 1)
            .submit_on_enter(false)
    })
}

impl ShareContentDialog {
    pub(crate) fn contact(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            kind: ShareContentKind::Contact {
                query: line(window, cx, "Search contacts"),
            },
            error: None,
        }
    }

    pub(crate) fn location(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            kind: ShareContentKind::Location {
                latitude: line(window, cx, "Latitude, e.g. 37.7749"),
                longitude: line(window, cx, "Longitude, e.g. -122.4194"),
            },
            error: None,
        }
    }
}

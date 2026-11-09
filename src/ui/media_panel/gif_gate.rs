//! The session's one GIF gate, kept open while any window's media panel
//! shows the GIFs tab.
#![deny(clippy::indexing_slicing)]

use super::{MediaPanel, PanelTab};
use crate::ui::app::QuillApp;

impl MediaPanel {
    /// The panel is open on its GIFs tab.
    pub(in crate::ui) fn shows_gifs(&self) -> bool {
        self.is_open() && self.tab == PanelTab::Gifs
    }
}

impl QuillApp {
    /// GIFs download and play while some window's media panel shows their
    /// tab. The session keeps one gate for them (`gifs.open`): it opens
    /// when the first panel shows the tab and closes when the last one
    /// leaves it, so one window's close does not stop another's GIFs.
    pub(in crate::ui) fn sync_gif_downloads(&mut self) {
        let wanted = self.pickers.media_panel.shows_gifs() || self.windows.slot_shows_gifs();
        let open = self
            .session()
            .is_some_and(|session| session.stickers.gifs.open);
        if wanted == open {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            if wanted {
                let _ = live.driver.open_gif_panel();
            } else {
                live.driver.close_gif_panel();
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            if wanted {
                session.stickers.gifs.open = true;
            } else {
                session.stickers.gifs.close();
            }
        }
    }
}

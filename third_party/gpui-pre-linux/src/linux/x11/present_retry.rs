// Added by the Quill project (2026) to gpui-pre-linux. See QUILL-CHANGES.md
// next to this crate's Cargo.toml.

//! Whether to draw again after the swapchain refused a frame.
//!
//! `WgpuRenderer::draw` returns false when the surface was suboptimal, lost,
//! outdated, timed out or occluded. GPUI has already cleared the window's
//! dirty flag by then, so without a retry the window shows stale content
//! until something else invalidates it. A refused frame forces another one
//! on the next refresh, up to [`RETRY_LIMIT`] in a row, so a window that
//! stays occluded stops retrying and lets the refresh timer park.

pub(crate) const RETRY_LIMIT: u8 = 8;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PresentRetry {
    failures: u8,
    requested: bool,
}

impl PresentRetry {
    /// Records a draw; returns whether the next refresh must force a frame.
    /// A draw the last refusal did not ask for starts a new run of retries.
    pub(crate) fn after_draw(&mut self, presented: bool) -> bool {
        let chained = std::mem::take(&mut self.requested);
        if presented {
            self.failures = 0;
            return false;
        }
        self.failures = if chained {
            self.failures.saturating_add(1)
        } else {
            1
        };
        self.requested = self.failures <= RETRY_LIMIT;
        self.requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_frame_is_drawn_again() {
        let mut retry = PresentRetry::default();
        assert!(retry.after_draw(false));
        assert!(!retry.after_draw(true));
    }

    #[test]
    fn retries_stop_after_the_limit() {
        let mut retry = PresentRetry::default();
        for _ in 0..RETRY_LIMIT {
            assert!(retry.after_draw(false));
        }
        assert!(!retry.after_draw(false));
    }

    #[test]
    fn a_new_invalidation_after_giving_up_retries_again() {
        let mut retry = PresentRetry::default();
        while retry.after_draw(false) {}
        // Something else (expose, resize) drew the window and it was refused
        // too: that starts a fresh run.
        assert!(retry.after_draw(false));
    }

    #[test]
    fn a_presented_frame_resets_the_count() {
        let mut retry = PresentRetry::default();
        for _ in 0..RETRY_LIMIT {
            retry.after_draw(false);
        }
        retry.after_draw(true);
        for _ in 0..RETRY_LIMIT {
            assert!(retry.after_draw(false));
        }
    }
}

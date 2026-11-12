//! The line under the proxy list: one stage that hides itself.
use super::QuillApp;
use gpui_kit::*;

/// The line under the proxy list: hidden, or showing with the timer that
/// hides it. A newer line replaces the stage, which cancels the older
/// timer, so no timer hides a line that is not its own.
#[derive(Default)]
pub(crate) enum ProxyNotice {
    #[default]
    Hidden,
    Showing {
        text: String,
        _timer: Task<()>,
    },
}

impl ProxyNotice {
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Showing { text, .. } => Some(text),
            Self::Hidden => None,
        }
    }
}

/// How long the line under the list stays.
pub(super) const NOTICE: std::time::Duration = std::time::Duration::from_secs(4);

impl QuillApp {
    /// Say `text` under the list for a few seconds (`ProxyNotice`).
    pub(super) fn proxy_notice(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let timer = self.spawn_screen(cx, async move |this, cx| {
            cx.background_executor().timer(NOTICE).await;
            let _ = this.update(cx, |this, cx| {
                this.settings.proxy.notice = ProxyNotice::Hidden;
                cx.notify();
            });
        });
        self.settings.proxy.notice = ProxyNotice::Showing {
            text: text.into(),
            _timer: timer,
        };
    }
}

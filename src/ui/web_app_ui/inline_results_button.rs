//! The mini app button above inline bot results.

use super::*;

impl QuillApp {
    /// tdesktop `Inner::switchPm` with a web view url: open the bot's app
    /// at that url for the open chat (`attachWebView().open`). Quill runs
    /// it like an inline keyboard app, so the first open still asks for
    /// the bot's terms.
    pub(in crate::ui) fn open_inline_results_web_app(
        &mut self,
        bot_id: i64,
        url: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.session().and_then(|session| session.open_chat) else {
            return;
        };
        self.start_web_app(
            PendingOpen::Open {
                chat_id,
                bot_id,
                url: url.to_string(),
                source: LaunchSource::InlineButton,
            },
            cx,
        );
    }
}

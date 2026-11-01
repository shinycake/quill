//! Screenshot demo: a received album of two files, stacked in one bubble.
use super::*;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};

register_demos![
    DemoSpec::chats(
        "ready-file-album",
        "screenshot demo — a received two-file album in one bubble"
    )
    .setup(QuillApp::demo_file_album),
];

impl QuillApp {
    fn demo_file_album(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let sink: std::sync::Arc<dyn DiagnosticSink> = self.demo_ui.sink.clone();
        let Some(session) = self.demo_session.as_mut() else {
            return;
        };
        self.demo_ui
            .seq
            .store(session.last_seq, std::sync::atomic::Ordering::SeqCst);
        for (id, name, caption) in [
            (9301, "Quarterly report.pdf", ""),
            (9302, "Budget.xlsx", "Both files for Monday"),
        ] {
            let file = demo_file_json(id, "", false);
            let json = format!(
                r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"date":1790640000,"media_album_id":"9300","content":{{"@type":"messageDocument","document":{{"@type":"document","file_name":"{name}","mime_type":"application/pdf","minithumbnail":null,"thumbnail":null,"document":{file}}},"caption":{{"@type":"formattedText","text":"{caption}","entities":[]}}}}}}}}"#
            );
            if let Some(parsed) = copy_and_parse(&json, &self.demo_ui.seq, &sink) {
                session.apply(parsed);
            }
        }
        session.open_chat(ChatId(11));
        cx.notify();
    }
}

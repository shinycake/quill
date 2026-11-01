//! The peek preview's sender and body lose what message text drops.

use super::chat_preview_line;
use quill::telegram::envelope::{MessageContent, TextContent};

#[test]
fn the_peek_preview_drops_zalgo_marks_and_tags() {
    let content = MessageContent::Text(TextContent {
        text: format!("hi{} \u{1F4DA}\u{E0069}\u{E007F}", "\u{0300}".repeat(30)),
        entities: Vec::new(),
        link_preview: None,
        animated: None,
    });
    let zalgo = format!("Eve{}", "\u{0301}".repeat(30));
    let (name, body, _) = chat_preview_line(&zalgo, false, None, &content, None, 1);
    assert_eq!(name, "Eve\u{0301}\u{0301}");
    assert_eq!(body, "hi\u{0300}\u{0300} \u{1F4DA}");
}

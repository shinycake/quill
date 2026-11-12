//! Sweep fixes (r4-08).
use super::sweep_s3_payments_1::Flaky;
use serde_json::json;

fn link(url: &str) -> serde_json::Value {
    json!({"@type":"chatFolderInviteLink","invite_link":url,"name":"","chat_ids":[1]})
}

/// 16-003: Share Folder reopened while a link delete is still out does not
/// read the list before the delete lands; the list is asked once the delete
/// is answered, and the deleted link stays gone.
#[test]
fn reopening_share_folder_waits_for_a_delete_still_out() {
    let mut t = Flaky::new();
    t.driver
        .delete_folder_invite_link(3, "https://t.me/addlist/a")
        .unwrap();
    t.driver.fetch_folder_share(3).unwrap();
    assert_eq!(
        t.sender.count("getChatFolderInviteLinks"),
        0,
        "no list read beside the delete"
    );
    assert!(
        t.driver.session.chat_list.folder_shares[&3]
            .links
            .shows_loading(),
        "the box draws loading, not an empty list"
    );
    t.answer("deleteChatFolderInviteLink", json!({"@type":"ok"}));
    t.driver.refetch_outdated_folder_links(3);
    assert_eq!(t.sender.count("getChatFolderInviteLinks"), 1);
    t.answer_nth(
        "getChatFolderInviteLinks",
        0,
        json!({"@type":"chatFolderInviteLinks","invite_links":[link("https://t.me/addlist/b")]}),
    );
    let urls: Vec<String> = t.driver.session.chat_list.folder_shares[&3]
        .links
        .data()
        .map(|links| links.iter().map(|l| l.invite_link.clone()).collect())
        .unwrap_or_default();
    assert_eq!(urls, ["https://t.me/addlist/b"]);
}

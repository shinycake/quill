//! The stylesheet and script every HTML export carries in `css/style.css`
//! and `js/script.js`, where tdesktop puts its own. Both are Quill's own,
//! written for this export.
use std::fs;
use std::io;
use std::path::Path;

/// Write `css/style.css` and `js/script.js` into the export root.
pub fn write(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root.join("css"))?;
    fs::write(root.join("css/style.css"), STYLE)?;
    fs::create_dir_all(root.join("js"))?;
    fs::write(root.join("js/script.js"), SCRIPT)?;
    Ok(())
}

pub const STYLE: &str = "\
:root{color-scheme:light dark;--bg:#f4f5f7;--card:#fff;--out:#e3f0ff;--text:#1c1e21;--muted:#65676b;--accent:#2f6fed;--line:rgba(127,127,127,.2);--mark:rgba(47,111,237,.18)}\n\
@media (prefers-color-scheme:dark){:root{--bg:#17212b;--card:#182533;--out:#2b5278;--text:#e8eaed;--muted:#8fa0b3;--accent:#6ab3f3;--mark:rgba(106,179,243,.22)}}\n\
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.45 system-ui,sans-serif}\n\
.page{max-width:760px;margin:0 auto;padding:24px 16px}\n\
header{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-bottom:8px}\n\
h1{font-size:20px;margin:0;flex:1}\n\
header .pagination{flex-basis:100%}\n\
a{color:var(--accent)}\n\
.back{text-decoration:none;font-size:24px;line-height:1;padding:0 6px}\n\
.pagination{display:block;text-align:center;padding:12px 0}\n\
.message{margin:6px 0;border-radius:12px;transition:background .3s}\n\
.message.selected{background:var(--mark)}\n\
.message.service{text-align:center}\n\
.details{color:var(--muted);font-size:13px}\n\
.bold{font-weight:600}\n\
.message.default{display:flex;gap:10px;align-items:flex-start}\n\
.message.default.joined{margin-top:2px;padding-left:52px}\n\
.message.default.out{flex-direction:row-reverse}\n\
.message.default.out.joined{padding-left:0;padding-right:52px}\n\
.message.default>.body{background:var(--card);border-radius:10px;padding:8px 12px;max-width:85%;min-width:0;overflow-wrap:anywhere}\n\
.message.default.out>.body{background:var(--out)}\n\
.userpic_wrap{flex:none}\n\
.userpic{display:block;border-radius:50%;object-fit:cover;color:#fff;font-weight:600;text-align:center;overflow:hidden}\n\
.userpic .initials{font-size:15px}\n\
.userpic1{background:#e17076}.userpic2{background:#7bc862}.userpic3{background:#e5ca77}.userpic4{background:#65aadd}\n\
.userpic5{background:#a695e7}.userpic6{background:#ee7aae}.userpic7{background:#6ec9cb}.userpic8{background:#faa774}\n\
.pull_right{float:right;margin-left:12px}\n\
.pull_left{float:left;margin-right:12px}\n\
.from_name{font-weight:600;font-size:13px;color:var(--accent)}\n\
.forwarded{border-left:2px solid var(--accent);padding-left:8px;margin:4px 0}\n\
.reply_to{margin-bottom:4px}\n\
.media_wrap{margin:4px 0}\n\
.media_wrap .title{font-weight:600}\n\
.media_wrap img,.media_wrap video{max-width:100%;max-height:520px;height:auto;border-radius:6px;display:block}\n\
.media_wrap audio{width:100%}\n\
.media_wrap .thumb{width:48px;height:48px;object-fit:cover;border-radius:6px;float:left;margin-right:10px}\n\
.media.file{display:block;overflow:hidden}\n\
.text blockquote{margin:4px 0;padding-left:8px;border-left:2px solid var(--muted)}\n\
.text pre,.text code{font-family:ui-monospace,monospace;background:rgba(127,127,127,.15);border-radius:4px}\n\
.text pre{padding:6px 8px;white-space:pre-wrap}\n\
.spoiler{background:rgba(127,127,127,.35);color:transparent;border-radius:3px}\n\
.spoiler:hover{color:inherit}\n\
.clearfix::after{content:'';display:block;clear:both}\n\
.page_about{color:var(--muted);font-size:13px;margin:8px 0 16px;white-space:pre-line}\n\
.with_divider{border-top:1px solid var(--line);padding-top:16px}\n\
.entry_list{background:var(--card);border-radius:10px;overflow:hidden}\n\
.entry{display:flex;gap:12px;align-items:center;padding:10px 14px;color:inherit;text-decoration:none;border-bottom:1px solid var(--line)}\n\
.entry:last-child{border-bottom:none}\n\
a.entry:hover{background:var(--mark)}\n\
.entry .body{flex:1;min-width:0}\n\
.entry .info{float:right;margin-left:12px}\n\
.entry .story{width:45px;height:80px;object-fit:cover;border-radius:6px;display:block}\n\
.entry .audio_icon{width:48px;height:48px;border-radius:50%;background:var(--accent)}\n\
.personal_info{display:flex;gap:16px;align-items:flex-start;background:var(--card);border-radius:10px;padding:16px;margin-bottom:16px}\n\
.personal_info .rows{flex:1}\n\
.personal_info .row{display:flex;gap:12px;padding:2px 0}\n\
.personal_info .label{width:110px;flex:none}\n\
.sections{display:flex;flex-direction:column;background:var(--card);border-radius:10px;overflow:hidden;margin-bottom:16px}\n\
.section{display:flex;justify-content:space-between;padding:12px 14px;color:inherit;text-decoration:none;border-bottom:1px solid var(--line)}\n\
.section:last-child{border-bottom:none}\n\
.section:hover{background:var(--mark)}\n\
.toast{position:fixed;left:50%;bottom:32px;transform:translateX(-50%);background:rgba(0,0,0,.75);color:#fff;padding:8px 14px;border-radius:8px;font-size:14px}\n";

/// Reply links, the back button and the "message not exported" note.
/// tdesktop's export has the same three behaviors (`CheckLocation`,
/// `GoToMessage`, `GoBack`); this is a small rewrite with the same names
/// so links written as `#go_to_message<id>` work from any page.
pub const SCRIPT: &str = r##""use strict";
var AllowBack = false;
function CheckLocation() {
  var match = /^#go_to_message(\d+)$/.exec(location.hash);
  if (match) {
    GoToMessage(parseInt(match[1], 10));
  } else if (location.hash === "#allow_back") {
    AllowBack = true;
  }
}
function ShowToast(text) {
  var toast = document.createElement("div");
  toast.className = "toast";
  toast.textContent = text;
  document.body.appendChild(toast);
  setTimeout(function () { toast.remove(); }, 3000);
}
function GoToMessage(id) {
  var element = document.getElementById("message" + id);
  if (!element) {
    ShowToast("This message was not exported. Maybe it was deleted.");
    return false;
  }
  if (location.hash !== "#go_to_message" + id) {
    history.replaceState(null, "", "#go_to_message" + id);
  }
  element.scrollIntoView({ block: "center", behavior: "smooth" });
  element.classList.add("selected");
  setTimeout(function () { element.classList.remove("selected"); }, 1000);
  return false;
}
function GoBack(anchor) {
  if (!AllowBack) {
    return true;
  }
  history.back();
  return false;
}
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_land_where_pages_expect_them() {
        let dir = std::env::temp_dir().join(format!("quill-export-assets-{}", std::process::id()));
        write(&dir).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("css/style.css")).unwrap(),
            STYLE
        );
        let script = fs::read_to_string(dir.join("js/script.js")).unwrap();
        for name in [
            "function CheckLocation",
            "function GoToMessage",
            "function GoBack",
        ] {
            assert!(script.contains(name), "{name}");
        }
        let _ = fs::remove_dir_all(&dir);
    }
}

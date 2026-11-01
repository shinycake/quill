//! The full account export as one blocking routine over [`Tdlib`], in
//! tdesktop's step order (`export_controller.cpp`): chats list, personal
//! information, profile pictures, stories, profile music, contacts,
//! sessions, then each chat. Sections are small and written whole; chats
//! go through the chat export's two-pass [`export_chat`], one at a time,
//! so memory stays bounded by one chat's page of messages.
use super::dialogs::{self, Dialog};
use super::html as pages;
use super::json::{self as out_json, ResultJson};
use super::sections::{self, Files};
use super::settings::{AccountSettings, Section};
use crate::export::assets;
use crate::export::json::MessageSink;
use crate::export::loaded_files::LoadedFiles;
use crate::export::settings::ExportSettings;
use crate::export::worker::{
    ChatOut, ExportError, Job, Progress, Tdlib, create_named_folder, export_chat, query,
};
use crate::json_access::JsonExt;
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    pub folder: PathBuf,
    pub chats: usize,
    pub messages: usize,
    pub files: usize,
    pub skipped: usize,
    /// Chosen chats left out because their content is protected.
    pub protected_chats: usize,
    /// Chats Telegram refused to page (for example a channel that became
    /// private); their rows say "No messages".
    pub failed_chats: usize,
    /// Saved phone contacts were not available; Telegram contacts stood in.
    pub contacts_partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountStage {
    Starting,
    ChatsList,
    PersonalInfo,
    Userpics,
    Stories,
    ProfileMusic,
    Contacts,
    Sessions,
    Chats,
    Finished(AccountSummary),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountProgress {
    pub stage: AccountStage,
    /// Chats found so far while listing them.
    pub found: usize,
    /// The chat being exported (0-based) and how many there are.
    pub chat_index: usize,
    pub chat_count: usize,
    pub chat_name: String,
    /// The running chat's own progress; its file counts run across chats.
    pub chat: Progress,
}

impl AccountProgress {
    pub fn starting() -> Self {
        Self {
            stage: AccountStage::Starting,
            found: 0,
            chat_index: 0,
            chat_count: 0,
            chat_name: String::new(),
            chat: Progress::starting(),
        }
    }

    pub fn is_settled(&self) -> bool {
        matches!(
            self.stage,
            AccountStage::Finished(_) | AccountStage::Failed(_) | AccountStage::Cancelled
        )
    }

    /// The main line of the progress box (tdesktop's state names).
    pub fn label(&self) -> String {
        match &self.stage {
            AccountStage::Starting => "Initializing…".into(),
            AccountStage::ChatsList => format!("Processing chats… {}", self.found),
            AccountStage::PersonalInfo => "Account information".into(),
            AccountStage::Userpics => "Profile photos".into(),
            AccountStage::Stories => "Story archive".into(),
            AccountStage::ProfileMusic => "Music on Profiles".into(),
            AccountStage::Contacts => "Contacts list".into(),
            AccountStage::Sessions => "Active sessions".into(),
            AccountStage::Chats => format!(
                "Chats: {} of {} · {}",
                self.chat_index + 1,
                self.chat_count,
                self.chat_name
            ),
            AccountStage::Finished(_) => "Your data was exported.".into(),
            AccountStage::Failed(reason) => format!("Export failed: {reason}"),
            AccountStage::Cancelled => "Export cancelled".into(),
        }
    }

    /// The line under it: the running chat's own step.
    pub fn detail(&self) -> Option<String> {
        match &self.stage {
            AccountStage::Chats => Some(self.chat.label()),
            AccountStage::Finished(summary) => Some(format!(
                "{} chats, {} messages and {} files saved. {} files left out.",
                summary.chats, summary.messages, summary.files, summary.skipped
            )),
            _ => None,
        }
    }

    /// 0.0..=1.0 across the chats, `None` before they start.
    pub fn fraction(&self) -> Option<f32> {
        match self.stage {
            AccountStage::Chats if self.chat_count > 0 => {
                let inner = self.chat.fraction().unwrap_or(0.0);
                Some((self.chat_index as f32 + inner) / self.chat_count as f32)
            }
            AccountStage::Finished(_) => Some(1.0),
            _ => None,
        }
    }
}

/// Everything the account export needs besides TDLib.
pub struct AccountJob<'a> {
    pub settings: &'a AccountSettings,
    pub files_root: &'a Path,
    pub offset_at: &'a dyn Fn(i64) -> i32,
    pub report: &'a dyn Fn(AccountProgress),
}

/// `DataExport_2023-11-14` in `parent` (tdesktop's name), private to the
/// user (mode 0700 on Unix, a protected owner-and-SYSTEM DACL on Windows):
/// it holds the whole account.
pub fn create_folder(parent: &Path, stamp: &str) -> io::Result<PathBuf> {
    let folder = create_named_folder(parent, &format!("DataExport_{stamp}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    }
    // Best effort: volumes without ACLs (FAT, exFAT) refuse a DACL, and the
    // export still has to work there.
    #[cfg(windows)]
    let _ = super::folder_acl::make_private(&folder);
    Ok(folder)
}

fn write_page(root: &Path, relative: &str, html: &str) -> io::Result<()> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, html)
}

/// Run the export into `folder` (made by [`create_folder`]).
pub fn run<T: Tdlib>(
    job: &AccountJob<'_>,
    tdlib: &mut T,
    folder: &Path,
) -> Result<AccountSummary, ExportError> {
    let spool = folder.join(".spool");
    let result = run_inner(job, tdlib, folder, &spool);
    let _ = fs::remove_dir_all(&spool);
    result
}

struct Writer<'a> {
    root: &'a Path,
    html: bool,
    json: Option<ResultJson<BufWriter<fs::File>>>,
    links: Vec<pages::SectionLink>,
}

impl Writer<'_> {
    fn field(&mut self, key: &str, value: &Value) -> io::Result<()> {
        match self.json.as_mut() {
            Some(json) => json.field(key, value),
            None => Ok(()),
        }
    }

    /// Write a list page and link it from the index, when it has rows.
    fn list(
        &mut self,
        count: usize,
        priority: u8,
        label: &'static str,
        path: &'static str,
        html: impl FnOnce() -> String,
    ) -> io::Result<()> {
        if count == 0 || !self.html {
            return Ok(());
        }
        write_page(self.root, path, &html())?;
        self.links.push(pages::SectionLink {
            priority,
            label,
            count,
            path,
        });
        Ok(())
    }
}

fn run_inner<T: Tdlib>(
    job: &AccountJob<'_>,
    tdlib: &mut T,
    root: &Path,
    spool: &Path,
) -> Result<AccountSummary, ExportError> {
    let settings = job.settings;
    let offset_at = job.offset_at;
    let mut progress = AccountProgress::starting();
    let step = |progress: &mut AccountProgress, stage: AccountStage| {
        progress.stage = stage;
        (job.report)(progress.clone());
    };
    step(&mut progress, AccountStage::Starting);
    let me = query(tdlib, json!({"@type": "getMe"}))?;
    let me_id = me.at("id").as_i64().unwrap_or(0);

    let (dialogs, protected_chats) = if settings.sections.any_chats() {
        step(&mut progress, AccountStage::ChatsList);
        let found = std::cell::Cell::new(0);
        let report_found = |n: usize| {
            found.set(n);
            let mut p = progress.clone();
            p.found = n;
            (job.report)(p);
        };
        let listed = dialogs::list(tdlib, settings, me_id, &report_found)?;
        progress.found = found.get();
        listed
    } else {
        (Vec::new(), 0)
    };

    let mut writer = Writer {
        root,
        html: settings.format.wants_html(),
        json: if settings.format.wants_json() {
            Some(ResultJson::start(BufWriter::new(fs::File::create(
                root.join("result.json"),
            )?))?)
        } else {
            None
        },
        links: Vec::new(),
    };
    if writer.html {
        assets::write(root)?;
    }
    let loaded = LoadedFiles::default();
    let files = Files {
        root,
        loaded: &loaded,
        files_root: job.files_root,
        settings,
        offset_at,
    };
    let sections = settings.sections;

    let mut personal = None;
    if sections.contains(Section::PersonalInfo) {
        step(&mut progress, AccountStage::PersonalInfo);
        let info = sections::personal_info(tdlib, &me)?;
        writer.field("personal_information", &out_json::personal_json(&info))?;
        personal = Some(info);
    }
    let mut newest_photo = None;
    let mut has_userpics = false;
    if sections.contains(Section::Userpics) {
        step(&mut progress, AccountStage::Userpics);
        let list = sections::userpics(tdlib, &files, me_id)?;
        newest_photo = list.iter().find_map(|p| match &p.outcome {
            crate::export::html::Outcome::Saved { path, .. } => Some(path.clone()),
            crate::export::html::Outcome::Skipped(_) => None,
        });
        has_userpics = !list.is_empty() && writer.html;
        writer.field(
            "profile_pictures",
            &out_json::userpics_json(&list, offset_at),
        )?;
        writer.list(list.len(), 4, "Profile pictures", pages::USERPICS, || {
            pages::userpics_page(&list, offset_at)
        })?;
    }
    if sections.contains(Section::Stories) {
        step(&mut progress, AccountStage::Stories);
        let own = query(
            tdlib,
            json!({"@type": "createPrivateChat", "user_id": me_id, "force": false}),
        )?;
        let self_chat = own.at("id").as_i64().unwrap_or(me_id);
        let (list, _protected) = sections::stories(tdlib, &files, self_chat)?;
        writer.field("stories", &out_json::stories_json(&list, offset_at))?;
        writer.list(list.len(), 5, "Stories archive", pages::STORIES, || {
            pages::stories_page(&list, offset_at)
        })?;
    }
    if sections.contains(Section::ProfileMusic) {
        step(&mut progress, AccountStage::ProfileMusic);
        let list = sections::profile_music(tdlib, &files, me_id)?;
        writer.field("profile_music", &out_json::music_json(&list))?;
        writer.list(list.len(), 6, "Profile Music", pages::MUSIC, || {
            pages::music_page(&list)
        })?;
    }
    let mut contacts_partial = false;
    if sections.contains(Section::Contacts) {
        step(&mut progress, AccountStage::Contacts);
        let (list, complete) = sections::contacts(tdlib)?;
        contacts_partial = !complete;
        writer.field(
            "contacts",
            &out_json::contacts_json(&list, complete, offset_at),
        )?;
        writer.list(list.len(), 2, "Contacts", pages::CONTACTS, || {
            pages::contacts_page(&list, complete, offset_at)
        })?;
        let top = sections::frequent(tdlib, me_id)?;
        writer.field("frequent_contacts", &out_json::frequent_json(&top))?;
        writer.list(top.len(), 3, "Frequent contacts", pages::FREQUENT, || {
            pages::frequent_page(&top)
        })?;
    }
    if sections.contains(Section::Sessions) {
        step(&mut progress, AccountStage::Sessions);
        let (list, web) = sections::sessions(tdlib)?;
        writer.field("sessions", &out_json::sessions_json(&list, offset_at))?;
        writer.field(
            "web_sessions",
            &out_json::web_sessions_json(&web, offset_at),
        )?;
        writer.list(list.len(), 7, "Sessions", pages::SESSIONS, || {
            pages::sessions_page(&list, offset_at)
        })?;
        writer.list(web.len(), 8, "Web sessions", pages::WEB_SESSIONS, || {
            pages::web_sessions_page(&web, offset_at)
        })?;
    }

    let mut summary = AccountSummary {
        folder: root.to_path_buf(),
        chats: dialogs.len(),
        messages: 0,
        files: 0,
        skipped: 0,
        protected_chats,
        failed_chats: 0,
        contacts_partial,
    };
    if !dialogs.is_empty() {
        let chats = Chats {
            dialogs: &dialogs,
            spool,
            me: me_id,
            loaded: &loaded,
        };
        export_chats(job, tdlib, &mut writer, &chats, &mut progress, &mut summary)?;
    }
    if writer.html {
        write_page(
            root,
            pages::INDEX,
            &pages::index_page(
                personal.as_ref(),
                newest_photo.as_deref(),
                has_userpics,
                &writer.links,
            ),
        )?;
    }
    if let Some(json) = writer.json.take() {
        json.finish()?;
    }
    Ok(summary)
}

/// The chats step's inputs.
struct Chats<'a> {
    dialogs: &'a [Dialog],
    spool: &'a Path,
    me: i64,
    loaded: &'a LoadedFiles,
}

fn export_chats<T: Tdlib>(
    job: &AccountJob<'_>,
    tdlib: &mut T,
    writer: &mut Writer<'_>,
    chats: &Chats<'_>,
    progress: &mut AccountProgress,
    summary: &mut AccountSummary,
) -> Result<(), ExportError> {
    let (dialogs, spool) = (chats.dialogs, chats.spool);
    let settings = job.settings;
    let root = writer.root;
    let mut list_page = if writer.html {
        let mut file = BufWriter::new(fs::File::create({
            fs::create_dir_all(root.join("lists"))?;
            root.join(pages::CHATS)
        })?);
        file.write_all(pages::chats_page_start().as_bytes())?;
        writer.links.push(pages::SectionLink {
            priority: 0,
            label: "Chats",
            count: dialogs.len(),
            path: pages::CHATS,
        });
        Some(file)
    } else {
        None
    };
    if let Some(json) = writer.json.as_mut() {
        json.chats_start()?;
    }
    progress.stage = AccountStage::Chats;
    progress.chat_count = dialogs.len();
    let mut chat_progress = Progress::starting();
    for (index, dialog) in dialogs.iter().enumerate() {
        progress.chat_index = index;
        progress.chat_name = dialog.name.clone();
        (job.report)(progress.clone());
        let chat_settings = ExportSettings {
            chat_id: dialog.chat_id,
            title: dialog.name.clone(),
            dir: root.to_path_buf(),
            format: settings.format,
            media: settings.media,
            size_limit: settings.size_limit,
            from: 0,
            till: 0,
            only_mine: dialog.only_my,
        };
        let base = progress.clone();
        let report = |p: Progress| {
            let mut whole = base.clone();
            whole.chat = p;
            (job.report)(whole);
        };
        let chat_job = Job {
            settings: &chat_settings,
            files_root: job.files_root,
            offset_at: job.offset_at,
            report: &report,
            me: Some(chats.me),
            loaded: chats.loaded,
        };
        let folder = root.join(&dialog.folder);
        let out = ChatOut {
            folder: &folder,
            spool,
            base: "../../",
            back: Some(pages::CHATS),
            json_prefix: &dialog.folder,
            page_when_empty: false,
        };
        if let Some(json) = writer.json.as_mut() {
            json.chat_start(dialog)?;
        }
        let sink = writer.json.as_mut().map(|j| j as &mut dyn MessageSink);
        let messages = match export_chat(&chat_job, tdlib, &out, sink, &mut chat_progress) {
            Ok(chat) => chat.messages,
            // One chat Telegram won't page (it became private, or the
            // account was banned there) does not end the whole export.
            Err(ExportError::Tdlib(_)) => {
                summary.failed_chats += 1;
                0
            }
            Err(other) => return Err(other),
        };
        if let Some(json) = writer.json.as_mut() {
            json.chat_end()?;
        }
        if let Some(file) = list_page.as_mut() {
            file.write_all(pages::chat_row(dialog, messages).as_bytes())?;
        }
        summary.messages += messages;
        progress.chat = chat_progress.clone();
    }
    summary.files = chat_progress.files;
    summary.skipped = chat_progress.skipped;
    if let Some(mut file) = list_page {
        file.write_all(pages::chats_page_end().as_bytes())?;
        file.flush()?;
    }
    if let Some(json) = writer.json.as_mut() {
        json.chats_end()?;
    }
    Ok(())
}

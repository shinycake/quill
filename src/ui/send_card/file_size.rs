//! The bytes of each file in a send tray, for the file rows ("PDF · 1.2 MB").
//! Read off the UI thread, once per file; the answer is kept by path for
//! every tray and window, like the other tray reads.
#![deny(clippy::indexing_slicing)]

use crate::ui::app::QuillApp;
use gpui_kit::*;
use quill::composer::AttachmentKind;
use std::path::PathBuf;

impl QuillApp {
    /// Start reading the size of every tray file that has not been read.
    pub(in crate::ui) fn read_tray_file_sizes(&mut self, cx: &mut Context<Self>) {
        let unread: Vec<PathBuf> = self
            .composer_sends
            .attachments()
            .filter(|attachment| attachment.kind == AttachmentKind::Document)
            .map(|attachment| attachment.path.clone())
            .filter(|path| self.composer_sends.reads.file_sizes.is_unread(path))
            .collect();
        for path in unread {
            let reads = &mut self.composer_sends.reads;
            reads.file_size_jobs += 1;
            let job = reads.file_size_jobs;
            let source = path.clone();
            let worker = cx.background_spawn(async move {
                std::fs::metadata(&source).ok().map(|meta| meta.len())
            });
            let key = path.clone();
            // The read stage owns the task: the file leaving its tray drops it.
            let answer = Self::spawn_app(cx, async move |this, cx| {
                let size = worker.await;
                let _ = this.update(cx, |this, cx| {
                    if this
                        .composer_sends
                        .reads
                        .file_sizes
                        .finish(&path, job, size)
                    {
                        cx.notify();
                    }
                });
            });
            self.composer_sends
                .reads
                .file_sizes
                .start(key, job, vec![answer]);
        }
    }
}

//! On Windows WebView2's browser process and its children outlive the
//! helper and keep the folder's files open until they exit by themselves.
//! A job holds each helper and everything it starts.

use std::process::Child;

#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
#[cfg(windows)]
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};

#[cfg(windows)]
#[derive(Debug)]
pub(super) struct Processes {
    job: Option<OwnedHandle>,
}

#[cfg(windows)]
impl Processes {
    pub(super) fn new() -> Self {
        // SAFETY: both arguments are optional and null asks for an unnamed
        // job with default security.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        // SAFETY: a non-null result is an open handle that only this value
        // owns.
        let job = (!job.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(job) });
        Self { job }
    }

    pub(super) fn add(&self, child: &Child) {
        if let Some(job) = &self.job {
            // SAFETY: both handles stay open for the call.
            let _ = unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) };
        }
    }

    pub(super) fn end_all(&self) {
        if let Some(job) = &self.job {
            // SAFETY: the handle stays open for the call.
            let _ = unsafe { TerminateJobObject(job.as_raw_handle(), 0) };
        }
    }
}

#[cfg(not(windows))]
#[derive(Debug)]
pub(super) struct Processes;

#[cfg(not(windows))]
impl Processes {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn add(&self, _child: &Child) {}

    pub(super) fn end_all(&self) {}
}

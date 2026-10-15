//! FIDO devices through `hidraw` (the way libfido2's `hid_hidraw.c` finds
//! them, without libudev): every `/sys/class/hidraw/*` whose report
//! descriptor has the FIDO usage page, opened as `/dev/hidrawN`. Pure
//! std plus `poll(2)`; access needs the usual udev rule for security keys
//! (distributions ship it with libfido2 / `u2f-udev`).
#![deny(clippy::indexing_slicing)]

use super::{DescriptorSummary, parse_descriptor};
use crate::webauthn::ctap2::ctaphid::{HidDevice, HidError};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

struct Hidraw {
    file: File,
    report_len: usize,
}

impl HidDevice for Hidraw {
    fn report_len(&self) -> usize {
        self.report_len
    }

    fn write_report(&mut self, report: &[u8]) -> Result<(), HidError> {
        // Report id 0 in front: FIDO devices use unnumbered reports.
        let mut out = Vec::with_capacity(report.len().saturating_add(1));
        out.push(0);
        out.extend_from_slice(report);
        self.file
            .write_all(&out)
            .map_err(|e| HidError::Io(e.to_string()))
    }

    fn read_report(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, HidError> {
        let mut poll = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
        // SAFETY: `poll` points at one valid `pollfd` that lives for the
        // call, and the count passed is 1.
        let ready = unsafe { libc::poll(&mut poll, 1, millis) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(HidError::Io(error.to_string()));
        }
        if ready == 0 {
            return Ok(None);
        }
        if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(HidError::Io("device gone".into()));
        }
        let mut buf = vec![0u8; self.report_len];
        let read = self
            .file
            .read(&mut buf)
            .map_err(|e| HidError::Io(e.to_string()))?;
        buf.truncate(read);
        buf.resize(self.report_len, 0);
        Ok(Some(buf))
    }
}

fn summary(sys_entry: &Path) -> Option<DescriptorSummary> {
    let descriptor = std::fs::read(sys_entry.join("device").join("report_descriptor")).ok()?;
    Some(parse_descriptor(&descriptor))
}

pub(super) fn open_all() -> Vec<Box<dyn HidDevice>> {
    let Ok(entries) = std::fs::read_dir("/sys/class/hidraw") else {
        return Vec::new();
    };
    let mut names: Vec<_> = entries.filter_map(Result::ok).collect();
    names.sort_by_key(std::fs::DirEntry::file_name);
    let mut devices: Vec<Box<dyn HidDevice>> = Vec::new();
    for entry in names {
        let Some(summary) = summary(&entry.path()) else {
            continue;
        };
        if !summary.is_fido() {
            continue;
        }
        let Some(report_len) = summary.report_len() else {
            continue;
        };
        let node = Path::new("/dev").join(entry.file_name());
        match OpenOptions::new().read(true).write(true).open(&node) {
            Ok(file) => devices.push(Box::new(Hidraw { file, report_len })),
            Err(error) => {
                crate::webauthn::log(&format!("cannot open {}: {error}", node.display()));
            }
        }
    }
    devices
}

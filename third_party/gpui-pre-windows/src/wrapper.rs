// Modified by the Quill project (2026) from gpui-pre-windows 0.3.10 (Apache-2.0):
// adds `GpuiMessage::post` and `send`. See third_party/gpui-pre-windows/QUILL-CHANGES.md.
use std::ops::Deref;

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{HCURSOR, PostMessageW, SendMessageW},
};

use crate::GpuiMessage;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SafeCursor {
    raw: HCURSOR,
}

unsafe impl Send for SafeCursor {}
unsafe impl Sync for SafeCursor {}

impl From<HCURSOR> for SafeCursor {
    fn from(value: HCURSOR) -> Self {
        SafeCursor { raw: value }
    }
}

impl Deref for SafeCursor {
    type Target = HCURSOR;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SafeHwnd {
    raw: HWND,
}

impl SafeHwnd {
    pub(crate) fn as_raw(&self) -> HWND {
        self.raw
    }
}

unsafe impl Send for SafeHwnd {}
unsafe impl Sync for SafeHwnd {}

impl From<HWND> for SafeHwnd {
    fn from(value: HWND) -> Self {
        SafeHwnd { raw: value }
    }
}

impl Deref for SafeHwnd {
    type Target = HWND;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl GpuiMessage {
    /// # Safety
    /// A pointer in `lparam` must stay valid until the receiving thread has read it.
    pub(crate) unsafe fn post(self, target: HWND) -> windows::core::Result<()> {
        let (id, wparam, lparam) = self.raw();
        // SAFETY: the caller keeps `lparam` valid.
        unsafe { PostMessageW(Some(target), id, WPARAM(wparam), LPARAM(lparam)) }
    }

    /// # Safety
    /// A pointer in `lparam` must stay valid for this call, which waits for the receiver.
    pub(crate) unsafe fn send(self, target: HWND) -> LRESULT {
        let (id, wparam, lparam) = self.raw();
        // SAFETY: the caller keeps `lparam` valid.
        unsafe { SendMessageW(target, id, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }
    }
}

// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

const WM_USER: u32 = 0x0400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiMessageId(u32);

pub(crate) const WM_GPUI_CURSOR_STYLE_CHANGED: GpuiMessageId = GpuiMessageId(WM_USER + 1);
pub(crate) const WM_GPUI_CLOSE_ONE_WINDOW: GpuiMessageId = GpuiMessageId(WM_USER + 2);
pub(crate) const WM_GPUI_TASK_DISPATCHED_ON_MAIN_THREAD: GpuiMessageId = GpuiMessageId(WM_USER + 3);
pub(crate) const WM_GPUI_DOCK_MENU_ACTION: GpuiMessageId = GpuiMessageId(WM_USER + 4);
pub(crate) const WM_GPUI_FORCE_UPDATE_WINDOW: GpuiMessageId = GpuiMessageId(WM_USER + 5);
pub(crate) const WM_GPUI_KEYBOARD_LAYOUT_CHANGED: GpuiMessageId = GpuiMessageId(WM_USER + 6);
pub(crate) const WM_GPUI_GPU_DEVICE_LOST: GpuiMessageId = GpuiMessageId(WM_USER + 7);
pub(crate) const WM_GPUI_DISPLAYS_CHANGED: GpuiMessageId = GpuiMessageId(WM_USER + 10);
/// Quill: destroy this window because its GPU device is gone and could not be recreated.
pub(crate) const WM_GPUI_DESTROY_AFTER_DEVICE_LOST: GpuiMessageId = GpuiMessageId(WM_USER + 11);
/// Quill: tells the platform window that the GPU device is gone for good (see
/// `WindowsPlatformInner::handle_device_gone`).
pub(crate) const WM_GPUI_GPU_DEVICE_GONE: GpuiMessageId = GpuiMessageId(WM_USER + 12);

const ALL_IDS: [GpuiMessageId; 10] = [
    WM_GPUI_CURSOR_STYLE_CHANGED,
    WM_GPUI_CLOSE_ONE_WINDOW,
    WM_GPUI_TASK_DISPATCHED_ON_MAIN_THREAD,
    WM_GPUI_DOCK_MENU_ACTION,
    WM_GPUI_FORCE_UPDATE_WINDOW,
    WM_GPUI_KEYBOARD_LAYOUT_CHANGED,
    WM_GPUI_GPU_DEVICE_LOST,
    WM_GPUI_DISPLAYS_CHANGED,
    WM_GPUI_DESTROY_AFTER_DEVICE_LOST,
    WM_GPUI_GPU_DEVICE_GONE,
];

impl GpuiMessageId {
    pub(crate) fn from_raw(raw: u32) -> Option<Self> {
        ALL_IDS.into_iter().find(|id| id.0 == raw)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ValidationNumber(usize);

impl ValidationNumber {
    pub(crate) fn new(number: usize) -> Self {
        Self(number)
    }

    pub(crate) fn message(self, id: GpuiMessageId, lparam: isize) -> GpuiMessage {
        GpuiMessage {
            id: id.0,
            wparam: self.0,
            lparam,
        }
    }

    pub(crate) fn accepts(self, wparam: usize) -> bool {
        wparam == self.0
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GpuiMessage {
    id: u32,
    wparam: usize,
    lparam: isize,
}

impl GpuiMessage {
    pub(crate) fn raw(self) -> (u32, usize, isize) {
        (self.id, self.wparam, self.lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NUMBER: usize = 0x51a7_c0de;

    #[test]
    fn every_message_arrives_at_a_receiver_with_the_same_number() {
        let number = ValidationNumber::new(NUMBER);

        for id in ALL_IDS {
            let (raw, wparam, lparam) = number.message(id, 7).raw();

            assert_eq!(GpuiMessageId::from_raw(raw), Some(id));
            assert!(number.accepts(wparam));
            assert_eq!(lparam, 7);
        }
    }

    #[test]
    fn a_receiver_drops_a_message_without_its_number() {
        let number = ValidationNumber::new(NUMBER);
        let (_, other_process, _) = ValidationNumber::new(NUMBER + 1)
            .message(WM_GPUI_DESTROY_AFTER_DEVICE_LOST, 0)
            .raw();

        assert!(!number.accepts(other_process));
        assert!(!number.accepts(0));
    }

    #[test]
    fn ids_gpui_does_not_use_are_not_gpui_messages() {
        for raw in [WM_USER, WM_USER + 8, WM_USER + 9, WM_USER + 13] {
            assert_eq!(GpuiMessageId::from_raw(raw), None);
        }
    }

    #[test]
    fn no_two_messages_share_an_id() {
        for (index, id) in ALL_IDS.iter().enumerate() {
            assert!(!ALL_IDS[..index].contains(id));
        }
    }
}

// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

use std::ops::Deref;

/// Never send under the lock: `SendMessageW` waits for the UI thread, which may wait in `write()`.
pub(crate) fn for_each_after_unlock<Guard, Window, Handle>(
    windows: Guard,
    handle: impl Fn(&Window) -> Handle,
    send: impl FnMut(Handle),
) where
    Guard: Deref,
    Guard::Target: AsRef<[Window]>,
{
    let handles: Vec<Handle> = (*windows).as_ref().iter().map(handle).collect();
    drop(windows);
    handles.into_iter().for_each(send);
}

/// A destroyed window stays listed until its posted `WM_GPUI_CLOSE_ONE_WINDOW` arrives, and a
/// message posted to it fails, so `send` gets only the windows still open.
pub(crate) fn for_each_open_after_unlock<Guard, Window, Handle>(
    windows: Guard,
    handle: impl Fn(&Window) -> Handle,
    is_destroyed: impl Fn(&Window) -> bool,
    send: impl FnMut(Handle),
) where
    Guard: Deref,
    Guard::Target: AsRef<[Window]>,
{
    let open: Vec<Handle> = (*windows)
        .as_ref()
        .iter()
        .filter(|window| !is_destroyed(window))
        .map(handle)
        .collect();
    drop(windows);
    open.into_iter().for_each(send);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn a_writer_can_take_the_list_during_every_send() {
        let list = RwLock::new(vec![1, 2, 3]);
        let mut writer_got_in = Vec::new();

        for_each_after_unlock(
            list.read().unwrap(),
            |window: &i32| *window,
            |handle| writer_got_in.push((handle, list.try_write().is_ok())),
        );

        assert_eq!(writer_got_in, [(1, true), (2, true), (3, true)]);
    }

    #[test]
    fn sends_go_to_the_windows_listed_at_the_start() {
        let list = RwLock::new(vec![1, 2, 3]);
        let mut sent = Vec::new();

        for_each_after_unlock(
            list.read().unwrap(),
            |window: &i32| *window,
            |handle| {
                sent.push(handle);
                if handle == 1
                    && let Ok(mut windows) = list.try_write()
                {
                    windows.remove(1);
                    windows.push(4);
                }
            },
        );

        assert_eq!(sent, [1, 2, 3]);
        assert_eq!(*list.read().unwrap(), [1, 3, 4]);
    }

    struct Tracked {
        handle: i32,
        destroyed: AtomicBool,
    }

    fn tracked(handle: i32) -> Tracked {
        Tracked {
            handle,
            destroyed: AtomicBool::new(false),
        }
    }

    fn open_windows(list: &RwLock<Vec<Tracked>>) -> Vec<i32> {
        let mut sent = Vec::new();
        for_each_open_after_unlock(
            list.read().unwrap(),
            |window: &Tracked| window.handle,
            |window| window.destroyed.load(Ordering::Relaxed),
            |handle| sent.push(handle),
        );
        sent
    }

    #[test]
    fn a_destroyed_window_still_listed_gets_no_message() {
        let list = RwLock::new(vec![tracked(1), tracked(2), tracked(3)]);
        list.read().unwrap()[1]
            .destroyed
            .store(true, Ordering::Relaxed);

        assert_eq!(open_windows(&list), [1, 3]);
    }

    #[test]
    fn a_window_opened_after_the_device_failed_is_reached() {
        let list = RwLock::new(vec![tracked(1)]);
        list.write().unwrap().push(tracked(2));

        assert_eq!(open_windows(&list), [1, 2]);
    }

    #[test]
    fn the_list_is_unlocked_during_every_send() {
        let list = RwLock::new(vec![tracked(1), tracked(2)]);
        let mut writer_got_in = Vec::new();

        for_each_open_after_unlock(
            list.read().unwrap(),
            |window: &Tracked| window.handle,
            |_| false,
            |handle| writer_got_in.push((handle, list.try_write().is_ok())),
        );

        assert_eq!(writer_got_in, [(1, true), (2, true)]);
    }
}

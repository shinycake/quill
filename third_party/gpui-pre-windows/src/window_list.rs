// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

use std::ops::Deref;

pub(crate) fn for_each_after_unlock<Guard, Window, Handle>(
    windows: Guard,
    handle: impl Fn(&Window) -> Handle,
    mut send: impl FnMut(Handle),
) where
    Guard: Deref,
    Guard::Target: AsRef<[Window]>,
{
    for window in (*windows).as_ref() {
        send(handle(window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;

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
}

//! Linux backend: the X11 RECORD extension (pure-Rust `x11rb`, already in the
//! lockfile through GPUI). It sees every key press and release on the
//! display without grabbing the key, so the key still reaches its window.
//!
//! Wayland sessions have no equivalent: the compositor does not hand out
//! system-wide key events to clients (and XRecord under XWayland would only
//! see X11 windows), so there push-to-talk stays in-window and Settings says
//! so. The xdg-desktop-portal GlobalShortcuts portal is not used: it binds
//! compositor-chosen triggers rather than an arbitrary held key, and it can't
//! be exercised without a Wayland desktop.

use super::{KeyFilter, LinuxSession, Sink, StartError, linux_session, x11_keysym};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::record::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;
/// `XRecordFromServer`: the reply carries device events.
const FROM_SERVER: u8 = 0;

/// A running recorder; dropping it ends the data stream.
pub struct Handle {
    control: Arc<RustConnection>,
    context: u32,
    active: Arc<AtomicBool>,
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle").finish_non_exhaustive()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
        // Ends the blocking `enable_context` stream on the data thread.
        let _ = self.control.record_disable_context(self.context);
        let _ = self.control.flush();
    }
}

fn session() -> LinuxSession {
    let get = |name: &str| std::env::var(name).ok();
    linux_session(
        get("XDG_SESSION_TYPE").as_deref(),
        get("WAYLAND_DISPLAY").as_deref(),
        get("DISPLAY").as_deref(),
    )
}

fn failed(what: &str) -> StartError {
    StartError::Failed(format!("Couldn't listen for keys ({what})."))
}

/// Keycodes that produce `keysym` in the first (unshifted) keymap column.
fn keycodes_for(control: &RustConnection, keysym: u32) -> Option<Vec<u8>> {
    let setup = control.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = control
        .get_keyboard_mapping(min, max - min + 1)
        .ok()?
        .reply()
        .ok()?;
    let per = usize::from(map.keysyms_per_keycode).max(1);
    let codes: Vec<u8> = map
        .keysyms
        .chunks(per)
        .enumerate()
        .filter(|(_, syms)| syms.first() == Some(&keysym))
        .filter_map(|(i, _)| u8::try_from(i).ok().map(|i| min.saturating_add(i)))
        .collect();
    (!codes.is_empty()).then_some(codes)
}

pub fn start(key: &str, sink: Sink) -> Result<Handle, StartError> {
    if let Some(limit) = static_limit() {
        return Err(StartError::Unsupported(limit));
    }
    let Some(keysym) = x11_keysym(key) else {
        return Err(StartError::Unsupported(
            "This key can't be used system-wide.".into(),
        ));
    };
    let (control, _) = RustConnection::connect(None).map_err(|_| failed("no X display"))?;
    let (data, _) = RustConnection::connect(None).map_err(|_| failed("no X display"))?;
    if control
        .extension_information(record::X11_EXTENSION_NAME)
        .ok()
        .flatten()
        .is_none()
    {
        return Err(StartError::Unsupported(
            "This X server has no RECORD extension.".into(),
        ));
    }
    let Some(codes) = keycodes_for(&control, keysym) else {
        return Err(StartError::Unsupported(
            "This key isn't on the current keyboard layout.".into(),
        ));
    };
    let context = control.generate_id().map_err(|_| failed("id"))?;
    let range = record::Range {
        device_events: record::Range8 {
            first: KEY_PRESS,
            last: KEY_RELEASE,
        },
        ..Default::default()
    };
    control
        .record_create_context(context, 0, &[u32::from(record::CS::ALL_CLIENTS)], &[range])
        .map_err(|_| failed("context"))?
        .check()
        .map_err(|_| failed("context"))?;
    let control = Arc::new(control);
    let active = Arc::new(AtomicBool::new(true));
    let handle = Handle {
        control: control.clone(),
        context,
        active: active.clone(),
    };
    std::thread::Builder::new()
        .name("quill-ptt-record".into())
        .spawn(move || {
            let mut filters: Vec<(u8, KeyFilter)> = codes
                .into_iter()
                .map(|code| (code, KeyFilter::new(u32::from(code))))
                .collect();
            let Ok(stream) = data.record_enable_context(context) else {
                return;
            };
            for reply in stream {
                let Ok(reply) = reply else { break };
                if !active.load(Ordering::SeqCst) {
                    break;
                }
                if reply.category != FROM_SERVER {
                    continue;
                }
                for event in reply.data.as_chunks::<32>().0 {
                    let pressed = match event[0] & 0x7f {
                        KEY_PRESS => true,
                        KEY_RELEASE => false,
                        _ => continue,
                    };
                    let code = event[1];
                    if let Some((_, filter)) = filters.iter_mut().find(|(c, _)| *c == code)
                        && let Some(edge) = filter.feed(u32::from(code), pressed)
                    {
                        sink(edge);
                    }
                }
            }
            let _ = control.record_free_context(context);
            let _ = control.flush();
        })
        .map_err(|_| failed("thread"))?;
    Ok(handle)
}

pub fn static_limit() -> Option<String> {
    match session() {
        LinuxSession::X11 => None,
        LinuxSession::Wayland => {
            Some("Wayland does not allow system-wide keyboard shortcuts for apps.".into())
        }
        LinuxSession::None => Some("No display was found for system-wide shortcuts.".into()),
    }
}

pub fn permission_granted() -> bool {
    true
}

pub fn open_permission_settings() {}

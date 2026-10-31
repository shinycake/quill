//! One blocking watcher per platform. Each calls
//! [`super::network_changed`] when something about the network may have
//! moved, and returns only when it cannot go on.

/// macOS: `SCDynamicStore` on the watcher's own run loop. The store also
/// names the proxy settings, so an event can mean those changed alone.
#[cfg(target_os = "macos")]
mod backend {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFAllocatorRef, CFType, TCFType};
    use core_foundation::runloop::{
        CFRunLoop, CFRunLoopSource, CFRunLoopSourceRef, kCFRunLoopCommonModes,
    };
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation_sys::base::{Boolean, CFIndex};
    use std::ffi::c_void;
    use std::ptr;

    type Store = *const c_void;
    type Callback = unsafe extern "C" fn(Store, CFArrayRef, *mut c_void);

    #[link(name = "SystemConfiguration", kind = "framework")]
    unsafe extern "C" {
        fn SCDynamicStoreCreate(
            allocator: CFAllocatorRef,
            name: CFStringRef,
            callout: Option<Callback>,
            context: *mut c_void,
        ) -> Store;
        fn SCDynamicStoreSetNotificationKeys(
            store: Store,
            keys: CFArrayRef,
            patterns: CFArrayRef,
        ) -> Boolean;
        fn SCDynamicStoreCreateRunLoopSource(
            allocator: CFAllocatorRef,
            store: Store,
            order: CFIndex,
        ) -> CFRunLoopSourceRef;
    }

    pub(in crate::net_watch) const EVENTS_COVER_PROXY: bool = true;

    unsafe extern "C" fn changed(_store: Store, _keys: CFArrayRef, _info: *mut c_void) {
        crate::thread_guard::ffi_boundary((), super::super::network_changed);
    }

    pub(in crate::net_watch) fn watch() {
        let name = CFString::new("quill-net-watch");
        // SAFETY: `name` outlives the call, null is the default allocator,
        // and a store with no context hands the callout a null `info`,
        // which `changed` ignores.
        let store = unsafe {
            SCDynamicStoreCreate(
                ptr::null(),
                name.as_concrete_TypeRef(),
                Some(changed),
                ptr::null_mut(),
            )
        };
        if store.is_null() {
            return;
        }
        // SAFETY: `store` is a non-null object from a Create call, which
        // this wrapper now owns; it lives as long as the run loop below.
        let _store = unsafe { CFType::wrap_under_create_rule(store) };
        let keys = CFArray::from_CFTypes(&[
            CFString::new("State:/Network/Global/IPv4"),
            CFString::new("State:/Network/Global/IPv6"),
            CFString::new("State:/Network/Global/Proxies"),
        ]);
        let patterns = CFArray::from_CFTypes(&[
            CFString::new("State:/Network/Interface/[^/]+/IPv4"),
            CFString::new("State:/Network/Interface/[^/]+/IPv6"),
        ]);
        // SAFETY: both arrays are live CFArrays of CFStrings for the call.
        let watching = unsafe {
            SCDynamicStoreSetNotificationKeys(
                store,
                keys.as_concrete_TypeRef(),
                patterns.as_concrete_TypeRef(),
            )
        };
        if watching == 0 {
            return;
        }
        // SAFETY: `store` is live and has a callout, as this call requires.
        let source = unsafe { SCDynamicStoreCreateRunLoopSource(ptr::null(), store, 0) };
        if source.is_null() {
            return;
        }
        // SAFETY: a non-null source from a Create call, owned from here on.
        let source = unsafe { CFRunLoopSource::wrap_under_create_rule(source) };
        // SAFETY: `kCFRunLoopCommonModes` is an immutable constant.
        let mode = unsafe { kCFRunLoopCommonModes };
        CFRunLoop::get_current().add_source(&source, mode);
        CFRunLoop::run_current();
    }
}

#[cfg(target_os = "linux")]
mod backend {
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    /// Reading the proxy may start `gsettings`, and netlink is chatty.
    pub(in crate::net_watch) const EVENTS_COVER_PROXY: bool = false;

    const GROUPS: libc::c_int = libc::RTMGRP_LINK
        | libc::RTMGRP_IPV4_IFADDR
        | libc::RTMGRP_IPV6_IFADDR
        | libc::RTMGRP_IPV4_ROUTE
        | libc::RTMGRP_IPV6_ROUTE;

    fn subscribe() -> io::Result<OwnedFd> {
        // SAFETY: constant arguments; the result is checked before use.
        let raw = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                libc::NETLINK_ROUTE,
            )
        };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `raw` is a socket this call just opened and nothing else owns.
        let socket = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: all-zero is a valid `sockaddr_nl`.
        let mut address: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        address.nl_family = libc::AF_NETLINK as libc::sa_family_t;
        address.nl_groups = GROUPS as u32;
        // SAFETY: `address` is a live `sockaddr_nl` of the size passed.
        let bound = unsafe {
            libc::bind(
                socket.as_raw_fd(),
                (&raw const address).cast(),
                size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };
        if bound < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(socket)
    }

    pub(in crate::net_watch) fn watch() {
        let Ok(socket) = subscribe() else {
            return;
        };
        let mut buffer = [0u8; 8192];
        loop {
            // SAFETY: `buffer` is writable for the length passed.
            let read = unsafe {
                libc::recv(
                    socket.as_raw_fd(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    0,
                )
            };
            if read == 0 {
                return;
            }
            if read < 0 {
                match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EINTR) => continue,
                    // Events were lost, so something happened.
                    Some(libc::ENOBUFS) => {}
                    _ => return,
                }
            }
            super::super::network_changed();
        }
    }
}

#[cfg(windows)]
mod backend {
    use windows_sys::Win32::NetworkManagement::IpHelper::NotifyAddrChange;

    /// The registry holds the proxy, and nothing here is told when it moves.
    pub(in crate::net_watch) const EVENTS_COVER_PROXY: bool = false;

    pub(in crate::net_watch) fn watch() {
        // SAFETY: two null pointers ask for the blocking form, which writes
        // to neither.
        while unsafe { NotifyAddrChange(std::ptr::null_mut(), std::ptr::null()) } == 0 {
            super::super::network_changed();
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod backend {
    pub(in crate::net_watch) const EVENTS_COVER_PROXY: bool = false;

    pub(in crate::net_watch) fn watch() {}
}

pub(super) use backend::{EVENTS_COVER_PROXY, watch};

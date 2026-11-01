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
        // A service's IPv4 key also changes when its router's link-layer
        // address is resolved (`net_watch::link`).
        let patterns = CFArray::from_CFTypes(&[
            CFString::new("State:/Network/Interface/[^/]+/IPv4"),
            CFString::new("State:/Network/Interface/[^/]+/IPv6"),
            CFString::new("State:/Network/Service/[^/]+/IPv4"),
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
    use std::ops::ControlFlow;
    use std::os::fd::{AsRawFd, OwnedFd};

    pub(in crate::net_watch) const EVENTS_COVER_PROXY: bool = false;

    const GROUPS: libc::c_int = libc::RTMGRP_LINK
        | libc::RTMGRP_NEIGH
        | libc::RTMGRP_IPV4_IFADDR
        | libc::RTMGRP_IPV6_IFADDR
        | libc::RTMGRP_IPV4_ROUTE
        | libc::RTMGRP_IPV6_ROUTE;

    pub(in crate::net_watch) fn subscribe() -> io::Result<OwnedFd> {
        super::super::netlink::open(GROUPS as u32)
    }

    pub(in crate::net_watch) fn watch() {
        if let Ok(socket) = subscribe() {
            listen(&socket, |batch| {
                heard(batch);
                ControlFlow::Continue(())
            });
        }
    }

    /// What the watcher does with one read.
    pub(in crate::net_watch) fn heard(batch: &[u8]) {
        // A gateway's entry tells two routers on one address apart.
        if !super::super::netlink::only_other_neighbours(batch, super::super::is_gateway) {
            super::super::network_changed();
        }
    }

    /// Calls `changed` with every read until it breaks or the socket fails.
    /// Lost events come as an empty read.
    pub(in crate::net_watch) fn listen(
        socket: &OwnedFd,
        mut changed: impl FnMut(&[u8]) -> ControlFlow<()>,
    ) {
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
            let batch = match usize::try_from(read) {
                Ok(read) => buffer.get(..read).unwrap_or_default(),
                Err(_) => match io::Error::last_os_error().raw_os_error() {
                    Some(libc::EINTR) => continue,
                    // Events were lost, so something happened.
                    Some(libc::ENOBUFS) => &[],
                    _ => return,
                },
            };
            if changed(batch).is_break() {
                return;
            }
        }
    }
}

#[cfg(windows)]
mod backend {
    use std::ffi::c_void;
    use std::io;
    use windows_sys::Win32::Foundation::{HANDLE, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        CancelMibChangeNotify2, MIB_IPFORWARD_ROW2, MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE,
        MIB_UNICASTIPADDRESS_ROW, NotifyIpInterfaceChange, NotifyRouteChange2,
        NotifyUnicastIpAddressChange,
    };
    use windows_sys::Win32::Networking::WinSock::AF_UNSPEC;

    pub(in crate::net_watch) const EVENTS_COVER_PROXY: bool = false;

    /// One notification: what kind of row changed, on which IP version
    /// (0 when the OS passes no row), and how.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(in crate::net_watch) struct Change {
        pub(in crate::net_watch) kind: Kind,
        pub(in crate::net_watch) family: u16,
        pub(in crate::net_watch) what: MIB_NOTIFICATION_TYPE,
    }

    /// Who hears the notifications; the OS gets a pointer to it.
    pub(in crate::net_watch) struct Listener(pub(in crate::net_watch) fn(Change));

    static NETWORK: Listener = Listener(|_| super::super::network_changed());

    fn notify(context: *const c_void, change: Change) {
        // SAFETY: every registration below passes a `&'static Listener`.
        let listener = unsafe { &*context.cast::<Listener>() };
        crate::thread_guard::ffi_boundary((), || (listener.0)(change));
    }

    unsafe extern "system" fn interface(
        context: *const c_void,
        row: *const MIB_IPINTERFACE_ROW,
        what: MIB_NOTIFICATION_TYPE,
    ) {
        // SAFETY: null or a row the OS keeps live for the call.
        let family = unsafe { row.as_ref() }.map_or(0, |row| row.Family);
        notify(
            context,
            Change {
                kind: Kind::Interface,
                family,
                what,
            },
        );
    }

    unsafe extern "system" fn address(
        context: *const c_void,
        row: *const MIB_UNICASTIPADDRESS_ROW,
        what: MIB_NOTIFICATION_TYPE,
    ) {
        // SAFETY: null or a row the OS keeps live for the call; every member
        // of the address union starts with the family.
        let family = unsafe { row.as_ref() }.map_or(0, |row| unsafe { row.Address.si_family });
        notify(
            context,
            Change {
                kind: Kind::Address,
                family,
                what,
            },
        );
    }

    unsafe extern "system" fn route(
        context: *const c_void,
        row: *const MIB_IPFORWARD_ROW2,
        what: MIB_NOTIFICATION_TYPE,
    ) {
        // SAFETY: as in `address`.
        let family = unsafe { row.as_ref() }
            .map_or(0, |row| unsafe { row.DestinationPrefix.Prefix.si_family });
        notify(
            context,
            Change {
                kind: Kind::Route,
                family,
                what,
            },
        );
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(in crate::net_watch) enum Kind {
        Interface,
        Address,
        Route,
    }

    /// The registrations of one [`subscribe`] call.
    pub(in crate::net_watch) struct Subscription(Vec<HANDLE>);

    impl Drop for Subscription {
        fn drop(&mut self) {
            for handle in self.0.drain(..) {
                // SAFETY: a handle from a successful Notify call, cancelled once.
                unsafe { CancelMibChangeNotify2(handle) };
            }
        }
    }

    /// Interface, address and route changes on IPv4 and IPv6 alike.
    /// `NotifyAddrChange` heard only IPv4 addresses.
    pub(in crate::net_watch) fn subscribe(listener: &'static Listener) -> Subscription {
        let kinds = [Kind::Interface, Kind::Address, Kind::Route];
        Subscription(super::keep_working(kinds, |kind| register(listener, *kind)))
    }

    fn register(listener: &'static Listener, kind: Kind) -> io::Result<HANDLE> {
        let initial = false;
        let context: *const c_void = std::ptr::from_ref(listener).cast();
        let mut handle: HANDLE = std::ptr::null_mut();
        let out = &raw mut handle;
        // SAFETY: `context` points at a `'static` listener, each callback
        // has the signature its call documents, and `out` is live.
        let status = unsafe {
            match kind {
                Kind::Interface => {
                    NotifyIpInterfaceChange(AF_UNSPEC, Some(interface), context, initial, out)
                }
                Kind::Address => {
                    NotifyUnicastIpAddressChange(AF_UNSPEC, Some(address), context, initial, out)
                }
                Kind::Route => NotifyRouteChange2(AF_UNSPEC, Some(route), context, initial, out),
            }
        };
        if status != NO_ERROR {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(handle)
    }

    /// The callbacks run on system threads for as long as Quill does.
    pub(in crate::net_watch) fn watch() {
        std::mem::forget(subscribe(&NETWORK));
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod backend {
    pub(in crate::net_watch) const EVENTS_COVER_PROXY: bool = false;

    pub(in crate::net_watch) fn watch() {}
}

pub(super) use backend::{EVENTS_COVER_PROXY, watch};

/// What `register` gives for each of `kinds`, leaving out and logging the
/// ones it refuses: one refused notification must not leave Quill deaf to
/// the others.
#[cfg(any(windows, test))]
fn keep_working<K: std::fmt::Debug, T>(
    kinds: impl IntoIterator<Item = K>,
    mut register: impl FnMut(&K) -> std::io::Result<T>,
) -> Vec<T> {
    kinds
        .into_iter()
        .filter_map(|kind| {
            register(&kind)
                .inspect_err(|error| eprintln!("quill: no {kind:?} notifications: {error}"))
                .ok()
        })
        .collect()
}

#[cfg(test)]
#[path = "watcher_tests.rs"]
mod watcher_tests;

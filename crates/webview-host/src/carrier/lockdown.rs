//! What each platform's web view needs beyond wry's options to host an
//! untrusted relay page out of sight.
//!
//! - Native dialogs and printing: WebKitGTK and WebView2 show real
//!   windows for `alert`, `confirm`, `prompt`, `beforeunload` and
//!   `print` even when the view is hidden, so they are turned off here
//!   (the bridge script also replaces the functions). WKWebView shows none
//!   without a UI delegate, and wry sets none.
//! - The path prefix: WebView2 refuses every request outside it in its
//!   request handler and cancels navigations outside it, the page's and
//!   its frames'; WebKit (macOS and Linux) gets content rules that block
//!   every load outside it, compiled before the page loads. The page's
//!   Content-Security-Policy, and the bridge's cancelling of navigations
//!   in the page itself, stay as further layers.
//! - WebRTC: off in WebKitGTK's settings; removed by the bridge script
//!   everywhere.
//! - Throttling of hidden views: off where the engine has a switch (macOS
//!   14+, WebView2 command-line switches), and macOS App Nap is held off
//!   for the helper's life, so its timers and the page's answers keep
//!   their pace.
//!
//! [`lock_down`] reports through [`Wake::LockedDown`] once the scope is
//! enforced (at once on Windows; after compiling the rules on WebKit).

use super::Wake;
use quill_webview_protocol::carrier::CarrierScope;
use std::path::Path;
use tao::event_loop::EventLoopProxy;
use wry::{WebView, WebViewBuilder};

/// Builder options every carrier web view gets on this platform.
pub(super) fn configure(builder: WebViewBuilder<'_>) -> WebViewBuilder<'_> {
    #[cfg(target_os = "macos")]
    let builder = builder.with_background_throttling(wry::BackgroundThrottlingPolicy::Disabled);
    #[cfg(windows)]
    let builder = {
        use wry::WebViewBuilderExtWindows;
        // wry's own defaults, plus: no occlusion tracking and no
        // throttling of a window that is never shown; no WebAssembly and no
        // WebTransport in any frame or worker (the bridge script only
        // reaches documents); WebRTC, should a page get at it, never sends
        // UDP around the proxy.
        builder
            .with_additional_browser_args(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,\
                 CalculateNativeWinOcclusion \
                 --disable-background-timer-throttling \
                 --disable-renderer-backgrounding \
                 --disable-backgrounding-occluded-windows \
                 --js-flags=--noexpose-wasm \
                 --disable-blink-features=WebTransport \
                 --force-webrtc-ip-handling-policy=disable_non_proxied_udp",
            )
            .with_default_context_menus(false)
            .with_browser_accelerator_keys(false)
    };
    builder
}

/// Keeps the helper's timers on time while it runs (macOS App Nap would
/// otherwise stretch them in a process with no visible window).
pub(super) struct Awake {
    #[cfg(target_os = "macos")]
    _activity: Option<
        objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_foundation::NSObjectProtocol>>,
    >,
}

pub(super) fn stay_awake() -> Awake {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
        let reason = NSString::from_str("Quill web proxy relay");
        let activity = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
            &reason,
        );
        Awake {
            _activity: Some(activity),
        }
    }
    #[cfg(not(target_os = "macos"))]
    Awake {}
}

/// Lock `webview` down for `scope`; the answer arrives as
/// [`Wake::LockedDown`].
pub(super) fn lock_down(
    webview: &WebView,
    scope: &CarrierScope,
    data_dir: Option<&Path>,
    events: EventLoopProxy<Wake>,
) {
    let locked = platform::lock_down(webview, scope, data_dir, events.clone());
    if !locked {
        let _ = events.send_event(Wake::LockedDown(false));
    }
}

/// Whether a frame may load `url`: the scope, or a blank document (a
/// frame's start; at the top level the load handler fails the carrier).
/// The bridge script applies the same rule in the page.
pub(super) fn may_load(scope: &CarrierScope, url: &str) -> bool {
    url == "about:blank" || url == "about:srcdoc" || scope.allows_navigation(url)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::super::{Wake, script::content_rules};
    use block2::RcBlock;
    use objc2_foundation::{MainThreadMarker, NSError, NSString, NSURL};
    use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore};
    use quill_webview_protocol::carrier::CarrierScope;
    use std::path::Path;
    use tao::event_loop::EventLoopProxy;
    use wry::{WebView, WebViewExtMacOS};

    /// Compile the content rules and add them to the view's controller.
    /// `false` when that cannot even start.
    pub(super) fn lock_down(
        webview: &WebView,
        scope: &CarrierScope,
        data_dir: Option<&Path>,
        events: EventLoopProxy<Wake>,
    ) -> bool {
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        // A store of Quill's when given one (so nothing lands in the
        // helper's default WebKit folder); the compiled list is removed
        // again once the view holds it.
        let store = match data_dir.and_then(|dir| dir.join("rules").to_str().map(String::from)) {
            Some(dir) => {
                let _ = std::fs::create_dir_all(&dir);
                let url = NSURL::fileURLWithPath(&NSString::from_str(&dir));
                // SAFETY: a file URL to a folder is what the store takes.
                unsafe { WKContentRuleListStore::storeWithURL(Some(&url), mtm) }
            }
            // SAFETY: called on the main thread, as the marker proves.
            None => unsafe { WKContentRuleListStore::defaultStore(mtm) },
        };
        let Some(store) = store else {
            return false;
        };
        let identifier = NSString::from_str(&format!("quill-carrier-{}", std::process::id()));
        let rules = NSString::from_str(&content_rules(scope));
        // SAFETY: the configuration of a live WKWebView is always set.
        let controller = unsafe { webview.webview().configuration().userContentController() };
        let remove_from = store.clone();
        let removed_id = identifier.clone();
        let block = RcBlock::new(move |list: *mut WKContentRuleList, _error: *mut NSError| {
            // SAFETY: WebKit passes a valid list or null for the duration
            // of the completion handler.
            let compiled = unsafe { list.as_ref() };
            let locked = match compiled {
                Some(list) => {
                    // SAFETY: a compiled list may be added to any controller.
                    unsafe { controller.addContentRuleList(list) };
                    true
                }
                None => false,
            };
            let done = RcBlock::new(|_: *mut NSError| {});
            // SAFETY: the identifier is the one just compiled; the empty
            // handler is called once by WebKit.
            unsafe {
                remove_from.removeContentRuleListForIdentifier_completionHandler(
                    Some(&removed_id),
                    Some(&done),
                )
            };
            let _ = events.send_event(Wake::LockedDown(locked));
        });
        // SAFETY: identifier and rules are valid strings; WebKit copies the
        // block and calls it once on the main thread.
        unsafe {
            store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                Some(&identifier),
                Some(&rules),
                Some(&block),
            )
        };
        true
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::super::{Wake, script::content_rules};
    use quill_webview_protocol::carrier::CarrierScope;
    use std::ffi::CString;
    use std::path::Path;
    use tao::event_loop::EventLoopProxy;
    use webkit2gtk::glib::translate::ToGlibPtr;
    use webkit2gtk::{NetworkError, SettingsExt, UserContentManager, WebViewExt, ffi, gio, glib};
    use wry::{WebView, WebViewExtUnix};

    /// What the filter store's callback needs; owned by the callback.
    struct Save {
        store: *mut ffi::WebKitUserContentFilterStore,
        manager: UserContentManager,
        identifier: CString,
        events: EventLoopProxy<Wake>,
    }

    pub(super) fn lock_down(
        webview: &WebView,
        scope: &CarrierScope,
        data_dir: Option<&Path>,
        events: EventLoopProxy<Wake>,
    ) -> bool {
        let view = webview.webview();
        // Handled means no dialog: `confirm` answers false, `prompt` null.
        view.connect_script_dialog(|_, _| true);
        view.connect_print(|_, _| true);
        if let Some(settings) = WebViewExt::settings(&view) {
            settings.set_enable_webrtc(false);
            settings.set_enable_media_stream(false);
            settings.set_javascript_can_open_windows_automatically(false);
            settings.set_enable_developer_extras(false);
        }
        // A network or TLS failure: the page never loads. A cancelled load
        // is a reload or a refused navigation, which is told apart
        // elsewhere. The URL is not in the message.
        let failed = events.clone();
        view.connect_load_failed(move |_, _, _, error| {
            if !error.matches(NetworkError::Cancelled) {
                let _ = failed.send_event(Wake::LoadError);
            }
            false
        });

        let Some(manager) = view.user_content_manager() else {
            return false;
        };
        let Some(dir) = data_dir.map(|dir| dir.join("filters")) else {
            return false;
        };
        let _ = std::fs::create_dir_all(&dir);
        let (Some(path), Ok(identifier)) = (
            dir.to_str().and_then(|dir| CString::new(dir).ok()),
            CString::new(format!("quill-carrier-{}", std::process::id())),
        ) else {
            return false;
        };
        // SAFETY: a NUL-terminated path; the store is a new reference
        // owned by `Save` and released in `saved`.
        let store = unsafe { ffi::webkit_user_content_filter_store_new(path.as_ptr()) };
        if store.is_null() {
            return false;
        }
        let source = glib::Bytes::from_owned(content_rules(scope).into_bytes());
        let save = Box::new(Save {
            store,
            manager,
            identifier,
            events,
        });
        let identifier = save.identifier.as_ptr();
        let source_ptr: *const glib::ffi::GBytes = source.to_glib_none().0;
        // SAFETY: store and identifier stay valid until `saved` runs (the
        // box owns them); WebKit takes its own reference to the bytes; the
        // callback gets the box back exactly once.
        unsafe {
            ffi::webkit_user_content_filter_store_save(
                store,
                identifier,
                source_ptr as *mut glib::ffi::GBytes,
                std::ptr::null_mut(),
                Some(saved),
                Box::into_raw(save).cast(),
            )
        };
        true
    }

    unsafe extern "C" fn saved(
        _source: *mut glib::gobject_ffi::GObject,
        result: *mut gio::ffi::GAsyncResult,
        data: glib::ffi::gpointer,
    ) {
        // SAFETY: `data` is the box handed to `save` above, given back once.
        let save = unsafe { Box::from_raw(data.cast::<Save>()) };
        let mut error = std::ptr::null_mut();
        // SAFETY: the store and result of this save; `error` is an out
        // pointer WebKit may fill.
        let filter = unsafe {
            ffi::webkit_user_content_filter_store_save_finish(save.store, result, &mut error)
        };
        if !error.is_null() {
            // SAFETY: an error WebKit set and handed over to us.
            unsafe { glib::ffi::g_error_free(error) };
        }
        let locked = !filter.is_null();
        if locked {
            let manager: *mut ffi::WebKitUserContentManager = save.manager.to_glib_none().0;
            // SAFETY: a live manager and the filter just compiled; the
            // manager takes its own reference.
            unsafe {
                ffi::webkit_user_content_manager_add_filter(manager, filter);
                ffi::webkit_user_content_filter_unref(filter);
            }
        }
        // SAFETY: the store and identifier are still owned by `save` (WebKit
        // copies the identifier and keeps the store alive for the removal);
        // WebKit requires a callback, which finishes the removal. The store
        // reference is ours.
        unsafe {
            ffi::webkit_user_content_filter_store_remove(
                save.store,
                save.identifier.as_ptr(),
                std::ptr::null_mut(),
                Some(removed),
                std::ptr::null_mut(),
            );
            glib::gobject_ffi::g_object_unref(save.store.cast());
        }
        let _ = save.events.send_event(Wake::LockedDown(locked));
    }

    unsafe extern "C" fn removed(
        source: *mut glib::gobject_ffi::GObject,
        result: *mut gio::ffi::GAsyncResult,
        _data: glib::ffi::gpointer,
    ) {
        let mut error = std::ptr::null_mut();
        // SAFETY: `source` is the store the removal ran on and `result` its
        // result; `error` is an out pointer WebKit may fill.
        unsafe {
            ffi::webkit_user_content_filter_store_remove_finish(source.cast(), result, &mut error);
            if !error.is_null() {
                glib::ffi::g_error_free(error);
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::super::Wake;
    use super::may_load;
    use quill_webview_protocol::carrier::CarrierScope;
    use std::path::Path;
    use tao::event_loop::EventLoopProxy;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL, ICoreWebView2_22,
        ICoreWebView2NavigationStartingEventHandler,
    };
    use webview2_com::{
        NavigationStartingEventHandler, WebResourceRequestedEventHandler, take_pwstr,
    };
    use windows::core::{HSTRING, Interface, PWSTR};
    use wry::{WebView, WebViewExtWindows};

    /// Cancels a frame's navigation out of `scope` (wry's navigation
    /// handler covers the page's). Cancelling does not keep the request
    /// off the network: WebView2 may send a navigation's `GET` while this
    /// runs and use that answer, and the request handler never sees it
    /// (docs/decisions/web-proxies.md). The bridge cancels in the page
    /// first, before the engine starts the navigation.
    fn frame_guard(scope: CarrierScope) -> ICoreWebView2NavigationStartingEventHandler {
        NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            // SAFETY: plain COM calls on the event's arguments, on the web
            // view's thread. A navigation that cannot be read is cancelled.
            let allowed = unsafe {
                (|| -> windows::core::Result<bool> {
                    let mut uri = PWSTR::null();
                    args.Uri(&mut uri)?;
                    Ok(may_load(&scope, &take_pwstr(uri)))
                })()
                .unwrap_or(false)
            };
            if !allowed {
                crate::debug(|| "frame navigation outside the scope cancelled".into());
                // SAFETY: as above.
                unsafe { args.SetCancel(true)? };
            }
            Ok(())
        }))
    }

    pub(super) fn lock_down(
        webview: &WebView,
        scope: &CarrierScope,
        _data_dir: Option<&Path>,
        events: EventLoopProxy<Wake>,
    ) -> bool {
        let core = webview.webview();
        let environment = webview.environment();
        let frames = frame_guard(scope.clone());
        let scope = scope.clone();
        // SAFETY: plain COM calls on the live web view, on its own thread.
        let locked = unsafe {
            (|| -> windows::core::Result<()> {
                let settings = core.Settings()?;
                settings.SetAreDefaultScriptDialogsEnabled(false)?;
                settings.SetAreDefaultContextMenusEnabled(false)?;
                settings.SetAreDevToolsEnabled(false)?;
                settings.SetIsStatusBarEnabled(false)?;
                crate::debug(|| {
                    let mut version = PWSTR::null();
                    let _ = environment.BrowserVersionString(&mut version);
                    format!("WebView2 runtime {}", take_pwstr(version))
                });
                let every = HSTRING::from("*");
                match core.cast::<ICoreWebView2_22>() {
                    Ok(core) => {
                        crate::debug(|| "request filter: every source kind".into());
                        core.AddWebResourceRequestedFilterWithRequestSourceKinds(
                            &every,
                            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                        )?
                    }
                    Err(_) => {
                        crate::debug(|| {
                            "WebView2 before 1.0.2210: worker requests are not filtered".into()
                        });
                        core.AddWebResourceRequestedFilter(
                            &every,
                            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                        )?
                    }
                }
                let mut token = 0i64;
                core.add_WebResourceRequested(
                    &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        // A request whose address cannot be read is refused,
                        // and so is a service worker's script (Chromium
                        // marks it `Service-Worker: script`), wherever it
                        // is: no service worker can start.
                        let allowed = (|| -> windows::core::Result<bool> {
                            let request = args.Request()?;
                            let mut service_worker = windows::core::BOOL::default();
                            request
                                .Headers()?
                                .Contains(&HSTRING::from("Service-Worker"), &mut service_worker)?;
                            let mut uri = PWSTR::null();
                            request.Uri(&mut uri)?;
                            Ok(!service_worker.as_bool() && scope.allows_request(&take_pwstr(uri)))
                        })()
                        .unwrap_or(false);
                        crate::debug(|| {
                            let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                            let _ = args.ResourceContext(&mut context);
                            let verdict = if allowed { "allowed" } else { "refused" };
                            format!("request filter {verdict} (resource context {})", context.0)
                        });
                        if !allowed {
                            let refused = environment.CreateWebResourceResponse(
                                None,
                                403,
                                &HSTRING::from("Forbidden"),
                                &HSTRING::new(),
                            )?;
                            args.SetResponse(&refused)?;
                        }
                        Ok(())
                    })),
                    &mut token,
                )?;
                core.add_FrameNavigationStarting(&frames, &mut token)?;
                Ok(())
            })()
            .is_ok()
        };
        if locked {
            let _ = events.send_event(Wake::LockedDown(true));
        }
        locked
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    use super::super::Wake;
    use quill_webview_protocol::carrier::CarrierScope;
    use std::path::Path;
    use tao::event_loop::EventLoopProxy;
    use wry::WebView;

    pub(super) fn lock_down(
        _webview: &WebView,
        _scope: &CarrierScope,
        _data_dir: Option<&Path>,
        _events: EventLoopProxy<Wake>,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> CarrierScope {
        CarrierScope::new(
            "https://proxy.example.com",
            "/relay/",
            "TelegramWebProxy",
            false,
        )
        .unwrap()
    }

    #[test]
    fn frames_may_load_the_scope_and_blank_documents() {
        for url in [
            "https://proxy.example.com/relay/",
            "https://proxy.example.com/relay/page?x=1",
            "about:blank",
            "about:srcdoc",
        ] {
            assert!(may_load(&scope(), url), "{url}");
        }
    }

    #[test]
    fn nothing_else_may_load() {
        for url in [
            "https://proxy.example.com/other/",
            "https://proxy.example.com/relay",
            "https://evil.example/",
            "http://proxy.example.com/relay/",
            "file:///C:/Windows/win.ini",
            "data:text/html,hi",
            "javascript:alert(1)",
            "about:settings",
            "",
        ] {
            assert!(!may_load(&scope(), url), "{url}");
        }
    }
}

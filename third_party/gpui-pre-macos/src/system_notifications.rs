// Modified by the Quill project (2026) from gpui-pre-macos 0.3.9 (Apache-2.0):
// inline text-input actions, typed-text responses and an image attachment,
// through `notification_extras`, and every notification-center call on a
// worker thread. See third_party/gpui-pre-macos/QUILL-CHANGES.md.
//! System notifications through the `UNUserNotificationCenter` API.
//!
//! Everything here runs lazily: nothing touches the notification center (and
//! so nothing can trigger the authorization prompt or the framework's
//! not-in-a-bundle abort) until the application posts a notification or
//! registers a response callback.
//!
//! The center is driven from one worker thread (Quill): posting copies the
//! picture and makes `usernoted` validate it, and every call is a round trip
//! to that daemon, none of which may hold up the main thread. One thread
//! keeps the calls in order, so a dismiss never overtakes the post it undoes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;

use block2::RcBlock;
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    ForegroundExecutor, SharedString, SystemNotification, SystemNotificationAction,
    SystemNotificationResponse, Task,
};
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{
    NSArray, NSBundle, NSError, NSObject, NSObjectProtocol, NSSet, NSString, NSURL,
};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationAction,
    UNNotificationActionOptions, UNNotificationAttachment, UNNotificationCategory,
    UNNotificationCategoryOptions, UNNotificationDefaultActionIdentifier,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNTextInputNotificationAction, UNTextInputNotificationResponse, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

use crate::notification_extras::{
    NotificationExtras, TextInputAction, store_reply_text, take_notification_extras,
};

/// The actions of one notification category: the buttons, and which of them
/// is a text field (Quill).
type CategoryKey = (Vec<SystemNotificationAction>, Option<TextInputAction>);

type ResponseCallback = Rc<RefCell<Option<Box<dyn FnMut(SystemNotificationResponse)>>>>;

pub(crate) struct SystemNotificationState {
    initialized: bool,
    worker: Option<mpsc::Sender<Job>>,
    callback: ResponseCallback,
    _response_task: Option<Task<()>>,
}

/// What the worker thread does with the notification center, in order.
enum Job {
    Show(SystemNotification, NotificationExtras),
    Dismiss(String),
}

impl SystemNotificationState {
    pub(crate) fn new() -> Self {
        Self {
            initialized: false,
            worker: None,
            callback: Rc::new(RefCell::new(None)),
            _response_task: None,
        }
    }

    pub(crate) fn show(&mut self, executor: &ForegroundExecutor, notification: SystemNotification) {
        self.initialize(executor);
        // Quill: the sender image and the inline reply field registered for
        // this tag, taken now so a later registration for the same tag is
        // not picked up by this post.
        let extras = take_notification_extras(&notification.tag);
        self.send(Job::Show(notification, extras));
    }

    pub(crate) fn dismiss(&mut self, executor: &ForegroundExecutor, tag: &str) {
        self.initialize(executor);
        self.send(Job::Dismiss(tag.to_string()));
    }

    pub(crate) fn on_response(
        &mut self,
        executor: &ForegroundExecutor,
        callback: Box<dyn FnMut(SystemNotificationResponse)>,
    ) {
        self.initialize(executor);
        *self.callback.borrow_mut() = Some(callback);
    }

    /// A job for the worker. Once the worker is gone (not in an app bundle,
    /// or the thread could not start) notifications are dropped.
    fn send(&self, job: Job) {
        if let Some(worker) = &self.worker {
            worker.send(job).ok();
        }
    }

    fn initialize(&mut self, executor: &ForegroundExecutor) {
        if self.initialized {
            return;
        }
        self.initialized = true;

        let (sender, mut receiver) = unbounded();
        self.worker = spawn_worker(sender);
        if self.worker.is_none() {
            return;
        }

        // Responses arrive from the delegate on an arbitrary thread; this task
        // hands them to the registered callback on the main thread.
        let callback = self.callback.clone();
        self._response_task = Some(executor.spawn(async move {
            while let Some(response) = receiver.next().await {
                // Take the callback out for the call: it may re-enter the
                // platform (e.g. to dismiss the notification it was told
                // about) or replace itself.
                let taken = callback.borrow_mut().take();
                if let Some(mut taken) = taken {
                    taken(response);
                    callback.borrow_mut().get_or_insert(taken);
                }
            }
        }));
    }
}

/// Start the thread that owns the notification center and runs every job
/// in the order it was sent. The thread ends at once when the center cannot
/// be used (not running from an app bundle).
fn spawn_worker(responses: UnboundedSender<SystemNotificationResponse>) -> Option<mpsc::Sender<Job>> {
    let (jobs, queue) = mpsc::channel::<Job>();
    let spawned = std::thread::Builder::new()
        .name("gpui-notifications".to_string())
        .spawn(move || {
            let Some(center) = autoreleasepool(|_| NotificationCenter::new(responses)) else {
                return;
            };
            while let Ok(job) = queue.recv() {
                autoreleasepool(|_| match job {
                    Job::Show(notification, extras) => center.show(notification, extras),
                    Job::Dismiss(tag) => center.dismiss(&tag),
                });
            }
        });
    match spawned {
        Ok(_) => Some(jobs),
        Err(error) => {
            log::warn!("system notifications disabled: no worker thread: {error}");
            None
        }
    }
}

struct NotificationCenter {
    center: Retained<UNUserNotificationCenter>,
    /// The center's `delegate` property is weak; keeping the delegate retained
    /// here keeps response handling alive for the app's lifetime.
    _delegate: Retained<NotificationResponseDelegate>,
    /// The union of every action set registered so far. Categories are shared
    /// by notifications with identical actions because macOS replaces the
    /// entire registered set whenever a category is added.
    categories: RefCell<HashMap<CategoryKey, (SharedString, Retained<UNNotificationCategory>)>>,
    authorization_requested: Cell<bool>,
}

impl NotificationCenter {
    fn new(sender: UnboundedSender<SystemNotificationResponse>) -> Option<Self> {
        // `UNUserNotificationCenter` raises `NSInternalInconsistencyException`
        // ("bundleProxyForCurrentProcess is nil"), aborting the process, when
        // the binary isn't part of an app bundle — which is how dev builds run
        // via `cargo run`. A bundle identifier is only present when launched
        // from a real `.app`, so use it as the guard.
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            log::info!("system notifications disabled: not running from an app bundle");
            return None;
        }

        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = NotificationResponseDelegate::new(sender);
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        Some(Self {
            center,
            _delegate: delegate,
            categories: RefCell::new(HashMap::new()),
            authorization_requested: Cell::new(false),
        })
    }

    fn request_authorization(&self) {
        if self.authorization_requested.replace(true) {
            return;
        }
        let completion = RcBlock::new(|granted: Bool, error: *mut NSError| {
            // SAFETY: when non-null, `error` is a valid `NSError` for the
            // duration of the callback.
            if let Some(error) = unsafe { error.as_ref() } {
                log::warn!(
                    "system notification authorization failed: {}",
                    error.localizedDescription()
                );
            } else if !granted.as_bool() {
                log::info!("system notification authorization denied");
            }
        });
        self.center
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                &completion,
            );
    }

    fn show(&self, notification: SystemNotification, extras: NotificationExtras) {
        self.request_authorization();

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notification.title));
        content.setBody(&NSString::from_str(&notification.body));
        if !notification.actions.is_empty() {
            let category_identifier =
                self.register_category(&notification.actions, extras.text_input);
            content.setCategoryIdentifier(&NSString::from_str(&category_identifier));
        }
        if let Some(image) = extras.image
            && let Some(attachment) = image_attachment(&image)
        {
            content.setAttachments(&NSArray::from_retained_slice(&[attachment]));
        }

        // A nil trigger delivers immediately. Reusing the tag as the request
        // identifier makes a newer notification for the same tag replace the
        // older one.
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&notification.tag),
            &content,
            None,
        );
        let completion = RcBlock::new(|error: *mut NSError| {
            // SAFETY: when non-null, `error` is a valid `NSError` for the
            // duration of the callback.
            if let Some(error) = unsafe { error.as_ref() } {
                log::warn!(
                    "failed to deliver system notification: {}",
                    error.localizedDescription()
                );
            }
        });
        self.center
            .addNotificationRequest_withCompletionHandler(&request, Some(&completion));
    }

    fn register_category(
        &self,
        actions: &[SystemNotificationAction],
        text_input: Option<TextInputAction>,
    ) -> SharedString {
        let mut categories = self.categories.borrow_mut();
        let key: CategoryKey = (actions.to_vec(), text_input);
        if let Some((identifier, _category)) = categories.get(&key) {
            return identifier.clone();
        }

        let identifier =
            SharedString::from(format!("gpui-system-notification-{}", categories.len()));
        let platform_actions: Vec<Retained<UNNotificationAction>> = actions
            .iter()
            .map(|action| {
                // Quill: the reply button opens a text field; its response
                // is a `UNTextInputNotificationResponse` with the same id.
                if let Some(input) = key.1.as_ref().filter(|input| input.action_id == action.id) {
                    let text_action = UNTextInputNotificationAction::actionWithIdentifier_title_options_textInputButtonTitle_textInputPlaceholder(
                        &NSString::from_str(&action.id),
                        &NSString::from_str(&action.label),
                        UNNotificationActionOptions::empty(),
                        &NSString::from_str(&input.send_label),
                        &NSString::from_str(&input.placeholder),
                    );
                    return Retained::into_super(text_action);
                }
                UNNotificationAction::actionWithIdentifier_title_options(
                    &NSString::from_str(&action.id),
                    &NSString::from_str(&action.label),
                    UNNotificationActionOptions::empty(),
                )
            })
            .collect();
        let category =
            UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
                &NSString::from_str(&identifier),
                &NSArray::from_retained_slice(&platform_actions),
                &NSArray::new(),
                UNNotificationCategoryOptions::empty(),
            );

        categories.insert(key, (identifier.clone(), category));
        let all: Vec<Retained<UNNotificationCategory>> = categories
            .values()
            .map(|(_identifier, category)| category.clone())
            .collect();
        self.center
            .setNotificationCategories(&NSSet::from_retained_slice(&all));
        identifier
    }

    fn dismiss(&self, tag: &str) {
        let identifiers = NSArray::from_retained_slice(&[NSString::from_str(tag)]);
        self.center
            .removePendingNotificationRequestsWithIdentifiers(&identifiers);
        self.center
            .removeDeliveredNotificationsWithIdentifiers(&identifiers);
    }
}

/// Quill: a notification attachment showing the PNG at `image`. The system
/// moves an attached file into its own store, so the picture is copied to a
/// temporary file first and the application's copy stays where it is. A copy
/// the center refuses is removed again.
fn image_attachment(image: &Path) -> Option<Retained<UNNotificationAttachment>> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let copy = std::env::temp_dir().join(format!(
        "gpui-notification-{}-{nanos}.png",
        std::process::id()
    ));
    if let Err(error) = std::fs::copy(image, &copy) {
        log::warn!("failed to copy the system notification image: {error}");
        return None;
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(&copy.to_string_lossy()));
    // SAFETY: `options` is nil, which the API allows.
    let attachment = unsafe {
        UNNotificationAttachment::attachmentWithIdentifier_URL_options_error(
            &NSString::from_str("image"),
            &url,
            None,
        )
    };
    match attachment {
        Ok(attachment) => Some(attachment),
        Err(error) => {
            log::warn!(
                "system notification image rejected: {}",
                error.localizedDescription()
            );
            let _ = std::fs::remove_file(&copy);
            None
        }
    }
}

struct DelegateIvars {
    sender: UnboundedSender<SystemNotificationResponse>,
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements and
    // `NotificationResponseDelegate` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[ivars = DelegateIvars]
    struct NotificationResponseDelegate;

    unsafe impl NSObjectProtocol for NotificationResponseDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for NotificationResponseDelegate {
        // Called when the user activates a delivered notification, possibly on
        // a non-main thread; the channel hop hands the response to the
        // foreground task holding the receiver.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive_notification_response(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let tag = response.notification().request().identifier().to_string();
            let action = response.actionIdentifier();
            // Quill: a response from the inline reply field carries the typed
            // text; park it under the tag for the application's handler.
            if let Some(text_response) = response.downcast_ref::<UNTextInputNotificationResponse>()
            {
                store_reply_text(&tag, text_response.userText().to_string());
            }
            // The other well-known identifier, `UNNotificationDismissActionIdentifier`,
            // is only delivered for categories opting into dismiss callbacks,
            // which we never request.
            let action_id = if &*action == unsafe { UNNotificationDefaultActionIdentifier } {
                None
            } else {
                Some(SharedString::from(action.to_string()))
            };
            self.ivars()
                .sender
                .unbounded_send(SystemNotificationResponse {
                    tag: SharedString::from(tag),
                    action_id,
                })
                .ok();
            completion_handler.call(());
        }

        // Without this, macOS suppresses banners while the app is frontmost.
        // Posting is the application's decision; whether the app is focused is
        // its criterion to apply, not the platform's.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present_notification(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }
    }
);

impl NotificationResponseDelegate {
    fn new(sender: UnboundedSender<SystemNotificationResponse>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { sender });
        // SAFETY: `NSObject`'s `init` is its designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

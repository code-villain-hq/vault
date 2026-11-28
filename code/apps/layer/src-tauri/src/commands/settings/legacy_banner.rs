//! The unbundled macOS fallback must split submission from activation.
//! Here `notify-rust` only delivers inside `wait_for_action`, which also
//! blocks until dismissal or activation. Use the same center, with a delegate
//! to handle clicks without holding the caller's delivery gate.

#![allow(deprecated)] // UserNotifications requires a bundle; this is the fallback.

use std::sync::Once;

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_foundation::{
    NSDictionary, NSObject, NSObjectProtocol, NSString, NSUserNotification, NSUserNotificationActivationType,
    NSUserNotificationCenter, NSUserNotificationCenterDelegate,
};
use tauri::{AppHandle, Emitter, Runtime};

const TARGET: &str = "layer-opens";
type Open = Box<dyn Fn(String) + Send + Sync>;

define_class!(
    // SAFETY: NSObject does not require any custom behavior from its subclasses.
    #[unsafe(super(NSObject))]
    #[name = "LayerLegacyBannerPresenter"]
    #[ivars = Open]
    struct Presenter;

    // SAFETY: NSObjectProtocol has no additional requirements.
    unsafe impl NSObjectProtocol for Presenter {}

    // SAFETY: The selector and signature match the delegate protocol.
    unsafe impl NSUserNotificationCenterDelegate for Presenter {
        #[unsafe(method(userNotificationCenter:didActivateNotification:))]
        fn activated(&self, center: &NSUserNotificationCenter, notification: &NSUserNotification) {
            if notification.activationType() == NSUserNotificationActivationType::ContentsClicked {
                let target = notification
                    .userInfo()
                    .and_then(|info| info.objectForKey(&NSString::from_str(TARGET)))
                    .and_then(|target| target.downcast::<NSString>().ok());
                if let Some(target) = target {
                    self.ivars()(target.to_string());
                }
            }
            center.removeDeliveredNotification(notification);
        }
    }

    impl Presenter {
        // The legacy center calls this selector for explicit dismissals.
        #[unsafe(method(userNotificationCenter:didDismissAlert:))]
        fn dismissed(&self, center: &NSUserNotificationCenter, notification: &NSUserNotification) {
            center.removeDeliveredNotification(notification);
        }
    }
);

pub(super) fn post<R: Runtime>(app: AppHandle<R>, title: &str, body: &str, target: &str) {
    autoreleasepool(|_| {
        let center = NSUserNotificationCenter::defaultUserNotificationCenter();
        static DELEGATE: Once = Once::new();
        DELEGATE.call_once(|| {
            let this = Presenter::alloc().set_ivars(Box::new(move |target| {
                let _ = app.emit("system-notify-click", target);
            }));
            // SAFETY: NSObject's initializer is valid for a subclass with initialized ivars.
            let presenter: Retained<Presenter> = unsafe { msg_send![super(this), init] };
            // SAFETY: The center borrows the delegate, which lives for the process.
            unsafe { center.setDelegate(Some(ProtocolObject::from_ref(&*presenter))) };
            std::mem::forget(presenter);
        });

        let notification = NSUserNotification::new();
        notification.setIdentifier(Some(&NSString::from_str(&uuid::Uuid::new_v4().to_string())));
        notification.setTitle(Some(&NSString::from_str(title)));
        notification.setInformativeText(Some(&NSString::from_str(body)));
        notification.setHasActionButton(false);
        let info = NSDictionary::<NSString, NSString>::from_slices(
            &[&*NSString::from_str(TARGET)],
            &[&*NSString::from_str(target)],
        );
        // SAFETY: userInfo accepts property lists; both keys and values are strings.
        unsafe { notification.setUserInfo(Some(info.cast_unchecked::<NSString, AnyObject>())) };
        center.deliverNotification(&notification);
    });
}

//! Notifications from a studio desktop app, through UserNotifications.
//! tauri-plugin-notification posts through NSUserNotificationCenter, which
//! macOS no longer shows: an app that uses it is never even asked whether it
//! may notify.

use std::sync::OnceLock;

/// Asks whether the app may notify, if it hasn't been asked. Call it as the
/// app starts, so the question doesn't wait for its first notification.
pub fn ask() {
    #[cfg(target_os = "macos")]
    mac::allowed(|| {});
}

/// Shows `title` over `body`, with the sound, on the Mac it runs on, while
/// the app is in front too. False where it can't: outside an app bundle
/// (`tauri dev`), and on other systems, so the app can show it its own way.
pub fn show(title: &str, body: &str) -> bool {
    shown(title, body, None)
}

/// `show`, and a click on it hands `opens` to the app's `on_open`, in this
/// run or a later one: it travels with the notification, so one still in
/// Notification Center opens it too.
pub fn show_opening(title: &str, body: &str, opens: &str) -> bool {
    shown(title, body, Some(opens))
}

fn shown(title: &str, body: &str, opens: Option<&str>) -> bool {
    #[cfg(target_os = "macos")]
    {
        let (title, body, opens) = (title.to_owned(), body.to_owned(), opens.map(str::to_owned));
        mac::allowed(move || mac::post(&title, &body, opens.as_deref()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (title, body, opens);
        false
    }
}

/// What a click on a notification from `show_opening` does with its `opens`.
/// Set once, before `ask`.
pub fn on_open(opened: impl Fn(String) + Send + Sync + 'static) {
    let _ = OPENED.set(Box::new(opened));
}

static OPENED: OnceLock<Box<dyn Fn(String) + Send + Sync>> = OnceLock::new();

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::Once;

    use block2::{DynBlock, RcBlock};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Bool, ProtocolObject};
    use objc2::{AnyThread, define_class, msg_send};
    use objc2_foundation::{
        NSBundle, NSDictionary, NSError, NSObject, NSObjectProtocol, NSString, NSUUID,
    };
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
        UNNotificationDefaultActionIdentifier, UNNotificationPresentationOptions,
        UNNotificationRequest, UNNotificationResponse, UNNotificationSound,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };

    /// Where `show_opening` keeps what a click opens, in the notification.
    const OPENS: &str = "opens";

    define_class!(
        /// Shows notifications while the app is in front, which macOS
        /// otherwise keeps quiet, and hands a click to `on_open`.
        // SAFETY: NSObject asks nothing of subclasses, and this has no Drop.
        #[unsafe(super(NSObject))]
        #[name = "CodeVillainNotificationPresenter"]
        struct Presenter;

        // SAFETY: NSObjectProtocol asks nothing more.
        unsafe impl NSObjectProtocol for Presenter {}

        // SAFETY: UNUserNotificationCenterDelegate asks nothing more.
        unsafe impl UNUserNotificationCenterDelegate for Presenter {
            // SAFETY: the signature is the protocol's.
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                present: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                present.call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List
                    | UNNotificationPresentationOptions::Sound,));
            }

            // SAFETY: the signature is the protocol's.
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                done: &DynBlock<dyn Fn()>,
            ) {
                // SAFETY: a constant UserNotifications exports.
                let clicked = unsafe { UNNotificationDefaultActionIdentifier };
                if response.actionIdentifier().isEqualToString(clicked)
                    && let Some(opens) = opens(response)
                    && let Some(opened) = super::OPENED.get()
                {
                    opened(opens);
                }
                done.call(());
            }
        }
    );

    fn opens(response: &UNNotificationResponse) -> Option<String> {
        let info = response.notification().request().content().userInfo();
        let opens = info.objectForKey(&NSString::from_str(OPENS))?;
        Some(opens.downcast::<NSString>().ok()?.to_string())
    }

    impl Presenter {
        fn new() -> Retained<Self> {
            let this = Self::alloc().set_ivars(());
            // SAFETY: NSObject's init, which every subclass may call.
            unsafe { msg_send![super(this), init] }
        }
    }

    /// Runs `then` once the app may notify, asking first if it hasn't been
    /// asked. False outside an app bundle.
    pub fn allowed(then: impl Fn() + 'static) -> bool {
        // UserNotifications throws for a process that isn't an app bundle.
        if !NSBundle::mainBundle()
            .bundlePath()
            .to_string()
            .ends_with(".app")
        {
            return false;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        static PRESENTER: Once = Once::new();
        PRESENTER.call_once(|| {
            let presenter = Presenter::new();
            center.setDelegate(Some(ProtocolObject::from_ref(&*presenter)));
            // The center holds it weakly; it lives as long as the app.
            std::mem::forget(presenter);
        });
        // Asks only the first time; after that it answers at once.
        let asked = RcBlock::new(move |allowed: Bool, error: *mut NSError| {
            if allowed.as_bool() {
                then();
            } else {
                tracing::warn!("notifications aren't allowed: {}", described(error));
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &asked,
        );
        true
    }

    pub fn post(title: &str, body: &str, opens: Option<&str>) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        if let Some(opens) = opens {
            let info = NSDictionary::<NSString, NSString>::from_slices(
                &[&*NSString::from_str(OPENS)],
                &[&*NSString::from_str(opens)],
            );
            // SAFETY: strings over strings, which userInfo may hold.
            unsafe { content.setUserInfo(info.cast_unchecked::<AnyObject, AnyObject>()) };
        }
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSUUID::new().UUIDString(),
            &content,
            None,
        );
        let added = RcBlock::new(|error: *mut NSError| {
            if !error.is_null() {
                tracing::warn!("couldn't show a notification: {}", described(error));
            }
        });
        UNUserNotificationCenter::currentNotificationCenter()
            .addNotificationRequest_withCompletionHandler(&request, Some(&added));
    }

    fn described(error: *mut NSError) -> String {
        // SAFETY: UserNotifications passes null or an error that lives for the call.
        match unsafe { error.as_ref() } {
            Some(error) => error.localizedDescription().to_string(),
            None => "turned off in System Settings → Notifications".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn outside_an_app_it_leaves_it_to_the_app() {
        assert!(!super::show("Almanac", "Water the monstera"));
        assert!(!super::show_opening(
            "Layer",
            "Claude is waiting",
            "reviews"
        ));
    }
}

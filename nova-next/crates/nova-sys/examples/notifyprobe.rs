// Spike: may this (unpackaged) app read other apps' notifications? Asks Windows, reads nothing else.
use windows::UI::Notifications::{Management::UserNotificationListener, NotificationKinds};

fn main() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
    }
    let l = match UserNotificationListener::Current() {
        Ok(l) => l,
        Err(e) => return println!("UserNotificationListener::Current failed: {e:?}"),
    };
    println!("access now: {:?}", l.GetAccessStatus());
    println!("request: {:?}", nova_sys::media::wait(l.RequestAccessAsync(), 5000));
    match l.GetNotificationsAsync(NotificationKinds::Toast) {
        Err(e) => println!("GetNotificationsAsync call failed: {e:?}"),
        Ok(op) => {
            for _ in 0..150 { if op.Status().map(|s| s.0 != 0).unwrap_or(true) { break } std::thread::sleep(std::time::Duration::from_millis(100)); }
            println!("status {:?}, error {:?}", op.Status(), windows::core::Interface::cast::<windows_future::IAsyncInfo>(&op).and_then(|i| i.ErrorCode()));
            if let Ok(list) = op.GetResults() {
                println!("toasts visible to us: {}", list.Size().unwrap_or(0));
                // only which apps sent them, never what they say
                let mut apps = std::collections::BTreeMap::new();
                for n in list {
                    let app = n.AppInfo().and_then(|a| a.DisplayInfo()).and_then(|d| d.DisplayName()).map(|h| h.to_string()).unwrap_or_else(|_| "?".into());
                    *apps.entry(app).or_insert(0) += 1;
                }
                println!("from: {apps:?}");
            }
            // how long a second read takes (the island would ask every couple of seconds)
            let t = std::time::Instant::now();
            let again = nova_sys::media::wait(l.GetNotificationsAsync(NotificationKinds::Toast), 10000);
            println!("second read: {:?} in {:?}", again.map(|x| x.Size().unwrap_or(0)), t.elapsed());
        }
    }
}

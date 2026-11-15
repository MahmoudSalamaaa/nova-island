//! Other apps' notifications (new in 1.8, off unless the user turns it on): what is in Windows'
//! notification center, through the official listener. An app outside the Store can't get the
//! "new notification" event, so the list is read again and again; one read takes about 5 s.
//! The text is only handed to the island to show; it is never written anywhere.
use windows::UI::Notifications::{KnownNotificationBindings, Management::UserNotificationListener, NotificationKinds, UserNotification};

pub struct Note {
    pub id: u32,
    pub app: String,
    /// for icon(): the notification itself
    pub raw: UserNotification,
    pub title: String,
    pub body: String,
}

/// Every notification in the notification center, or None when Windows refuses.
pub fn read() -> Option<Vec<Note>> {
    let l = UserNotificationListener::Current().ok()?;
    let list = crate::media::wait(l.GetNotificationsAsync(NotificationKinds::Toast), 15000)?;
    Some(list.into_iter().filter_map(|n| note(&n)).collect())
}

fn note(n: &UserNotification) -> Option<Note> {
    let app = n.AppInfo().and_then(|a| a.DisplayInfo()).and_then(|d| d.DisplayName()).map(|h| h.to_string()).unwrap_or_default();
    let texts: Vec<String> = n
        .Notification()
        .and_then(|t| t.Visual())
        .and_then(|v| v.GetBinding(&KnownNotificationBindings::ToastGeneric()?))
        .and_then(|b| b.GetTextElements())
        .map(|els| els.into_iter().filter_map(|e| e.Text().ok().map(|h| h.to_string())).filter(|s| !s.trim().is_empty()).collect())
        .unwrap_or_default();
    let mut it = texts.into_iter();
    Some(Note { id: n.Id().ok()?, app, raw: n.clone(), title: it.next().unwrap_or_default(), body: it.collect::<Vec<_>>().join(" ") })
}

/// The sending app's AppUserModelId (what opens it: shell:AppsFolder\<id>).
pub fn aumid(n: &Note) -> String {
    n.raw.AppInfo().and_then(|a| a.AppUserModelId()).map(|h| h.to_string()).unwrap_or_default()
}

/// Asks Windows for access (the Store version: Windows asks the user once; outside the Store it is
/// already allowed). Called from the window thread, where Windows can show its question.
pub fn request() {
    if let Ok(l) = UserNotificationListener::Current() {
        let _ = l.RequestAccessAsync();
    }
}

/// Whether Windows lets this app read them.
pub fn allowed() -> bool {
    UserNotificationListener::Current().and_then(|l| l.GetAccessStatus()).map(|s| s.0 == 1).unwrap_or(false)
}

/// The sending app's logo (a PNG data URL) and its colour, from Windows.
pub fn icon(n: &Note) -> Option<(String, [u8; 3])> {
    logo(n).or_else(|| {
        // an app outside the Store: its icon from the Start menu
        let id = n.raw.AppInfo().ok()?.AppUserModelId().ok()?.to_string();
        crate::icons::start_icon(&id, 44)
    })
}

fn logo(n: &Note) -> Option<(String, [u8; 3])> {
    use windows::{Foundation::Size, Storage::Streams::DataReader};
    let logo = n.raw.AppInfo().ok()?.DisplayInfo().ok()?.GetLogo(Size { Width: 64.0, Height: 64.0 }).ok()?;
    let stream = crate::media::wait(logo.OpenReadAsync(), 3000)?;
    let len = stream.Size().ok()? as u32;
    if len == 0 || len > 2_000_000 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    crate::media::wait(reader.LoadAsync(len), 3000)?;
    let mut bytes = vec![0u8; len as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    crate::icons::png_and_color(&bytes, 44)
}

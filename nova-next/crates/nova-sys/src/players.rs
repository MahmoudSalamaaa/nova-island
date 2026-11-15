//! Players that publish no Windows media session (1.8.3), read and driven directly, like VLC
//! (vlc.rs):
//!   AIMP     its remote API: the "AIMP2_RemoteInfo" window answers state, position and length
//!            (WM_AIMP_PROPERTY) and takes play/pause, next, previous and seek (WM_AIMP_COMMAND);
//!            the shared memory of the same name holds the track's title and artist
//!   Winamp   its IPC: the "Winamp v1.7" window answers state, position and length (WM_USER) and
//!            takes its menu commands (WM_COMMAND); the title is in its window title
//!            ("12. Artist - Title - Winamp [Paused]")
//!   MPC-HC   the "MediaPlayerClassicW" window: the file name in its title, playing while its
//!            audio is heard (as VLC), media keys (WM_APPCOMMAND) to control it
//! Each is asked with a short timeout, so a player that hangs never holds the island up.
use std::time::{Duration, Instant};

use windows::{
    core::{w, Interface, PCWSTR},
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, WPARAM},
        Media::Audio::{eMultimedia, eRender, AudioSessionStateActive, Endpoints::IAudioMeterInformation, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator},
        System::{
            Com::{CoCreateInstance, CLSCTX_ALL},
            Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ},
        },
        UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, GetWindowTextW, GetWindowThreadProcessId, PostMessageW, SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_APPCOMMAND, WM_COMMAND, WM_USER},
    },
};

/// What a player shows on the island.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Now {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// seconds; 0 when the player doesn't tell
    pub pos: f64,
    pub dur: f64,
    /// the "app" the island shows (the player's exe)
    pub app: &'static str,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Aimp,
    Winamp,
    Mpc,
}

pub struct Players {
    shown: Option<(Kind, HWND)>,
    mpc_heard: Heard,
    /// MPC-HC doesn't say if it plays: after a play/pause from the island, the state it should be
    /// in now, until its sound agrees (at most 10 s)
    mpc_expect: Option<(bool, Instant)>,
    mpc_last: bool,
}

impl Players {
    pub fn new() -> Players {
        Players { shown: None, mpc_heard: Heard::default(), mpc_expect: None, mpc_last: false }
    }

    /// The first of AIMP, Winamp and MPC-HC that is playing, else the first that has a track
    /// open (paused). None when none of them is running with a track.
    pub fn find(&mut self) -> Option<Now> {
        let mut paused = None;
        for kind in [Kind::Aimp, Kind::Winamp, Kind::Mpc] {
            let found = match kind {
                Kind::Aimp => aimp(),
                Kind::Winamp => winamp(),
                Kind::Mpc => mpc(&mut self.mpc_heard).map(|(h, mut now)| {
                    if let Some((want, at)) = self.mpc_expect {
                        if now.playing == want || at.elapsed() > Duration::from_secs(10) {
                            self.mpc_expect = None;
                        } else {
                            now.playing = want;
                        }
                    }
                    self.mpc_last = now.playing;
                    (h, now)
                }),
            };
            if let Some((h, now)) = found {
                if now.playing {
                    self.shown = Some((kind, h));
                    return Some(now);
                }
                if paused.is_none() {
                    paused = Some((kind, h, now));
                }
            }
        }
        let (kind, h, now) = paused?;
        self.shown = Some((kind, h));
        Some(now)
    }

    /// Forget the player the island showed (it shows something else now).
    pub fn clear(&mut self) {
        self.shown = None;
    }

    /// toggle | next | prev, to the player the island shows
    pub fn command(&mut self, cmd: &str) {
        let Some((kind, h)) = self.shown else { return };
        unsafe {
            match kind {
                Kind::Aimp => {
                    // AIMP_RA_CMD_PLAYPAUSE, _NEXT, _PREV
                    let c = match cmd {
                        "toggle" => 14,
                        "next" => 17,
                        "prev" => 18,
                        _ => return,
                    };
                    let _ = PostMessageW(Some(h), WM_AIMP_COMMAND, WPARAM(c), LPARAM(0));
                }
                Kind::Winamp => {
                    // pause (toggles), play when stopped, next, previous
                    let c = match cmd {
                        "toggle" if ask(h, WM_USER, 0, IPC_ISPLAYING) == Some(0) => 40045,
                        "toggle" => 40046,
                        "next" => 40048,
                        "prev" => 40044,
                        _ => return,
                    };
                    let _ = PostMessageW(Some(h), WM_COMMAND, WPARAM(c), LPARAM(0));
                }
                Kind::Mpc => {
                    if cmd == "toggle" {
                        self.mpc_expect = Some((!self.mpc_last, Instant::now()));
                    }
                    let app: isize = match cmd {
                        "toggle" => 14,
                        "next" => 11,
                        "prev" => 12,
                        _ => return,
                    };
                    let _ = PostMessageW(Some(h), WM_APPCOMMAND, WPARAM(h.0 as usize), LPARAM(app << 16));
                }
            }
        }
    }

    /// Seeks to `sec` where the player allows it (AIMP, Winamp). True when it was sent.
    pub fn seek(&self, sec: f64) -> bool {
        let Some((kind, h)) = self.shown else { return false };
        let ms = (sec.max(0.0) * 1000.0).round() as usize;
        match kind {
            Kind::Aimp => ask(h, WM_AIMP_PROPERTY, AIMP_POSITION | AIMP_SET, ms as isize).is_some(),
            Kind::Winamp => ask(h, WM_USER, ms, IPC_JUMPTOTIME).is_some(),
            Kind::Mpc => false,
        }
    }
}

const WM_AIMP_COMMAND: u32 = WM_USER + 0x75;
const WM_AIMP_PROPERTY: u32 = WM_USER + 0x77;
const AIMP_SET: usize = 1;
const AIMP_POSITION: usize = 0x20;
const AIMP_DURATION: usize = 0x30;
const AIMP_STATE: usize = 0x40; // 0 stopped, 1 paused, 2 playing
const IPC_ISPLAYING: isize = 104; // 1 playing, 3 paused, 0 stopped
const IPC_GETOUTPUTTIME: isize = 105; // wParam 0: position in ms, 1: length in seconds
const IPC_JUMPTOTIME: isize = 106;

/// SendMessage with a 300 ms limit: None when the player didn't answer.
fn ask(h: HWND, msg: u32, w: usize, l: isize) -> Option<isize> {
    let mut out = 0usize;
    let ok = unsafe { SendMessageTimeoutW(h, msg, WPARAM(w), LPARAM(l), SMTO_ABORTIFHUNG, 300, Some(&mut out)) };
    (ok.0 != 0).then_some(out as isize)
}

fn window(class: PCWSTR) -> Option<HWND> {
    unsafe { FindWindowW(class, PCWSTR::null()).ok().filter(|h| !h.is_invalid()) }
}

fn title_of(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

/// A file's name without its folder and extension.
fn stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    match file.rfind('.') {
        Some(i) if i > 0 && file.len() - i <= 5 => file[..i].to_string(),
        _ => file.to_string(),
    }
}

// ---- AIMP ----
fn aimp() -> Option<(HWND, Now)> {
    let h = window(w!("AIMP2_RemoteInfo"))?;
    let state = ask(h, WM_AIMP_PROPERTY, AIMP_STATE, 0)?;
    if state == 0 {
        return None; // stopped: nothing to show
    }
    let pos = ask(h, WM_AIMP_PROPERTY, AIMP_POSITION, 0).unwrap_or(0).max(0) as f64 / 1000.0;
    let dur = ask(h, WM_AIMP_PROPERTY, AIMP_DURATION, 0).unwrap_or(0).max(0) as f64 / 1000.0;
    let (title, artist) = aimp_track().unwrap_or_default();
    Some((h, Now { title, artist, playing: state == 2, pos, dur, app: "AIMP.exe" }))
}

/// Title and artist from AIMP's shared memory: a header (its size first, the six string lengths
/// last), then album, artist, date, file name, genre and title, UTF-16 without terminators.
fn aimp_track() -> Option<(String, String)> {
    const SIZE: usize = 2048;
    unsafe {
        let map = OpenFileMappingW(FILE_MAP_READ.0, false, w!("AIMP2_RemoteInfo")).ok()?;
        let view = MapViewOfFile(map, FILE_MAP_READ, 0, 0, SIZE);
        let out = (|| {
            if view.Value.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(view.Value as *const u8, SIZE);
            let u32_at = |i: usize| bytes.get(i * 4..i * 4 + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
            let header = u32_at(0)?;
            let lens: Vec<usize> = (10..16).map(|i| u32_at(i).unwrap_or(0)).collect();
            if header < 64 || header > 256 || lens.iter().sum::<usize>() * 2 + header > SIZE {
                return None;
            }
            let mut at = header;
            let mut texts = Vec::new();
            for n in lens {
                let u: Vec<u16> = bytes[at..at + n * 2].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                texts.push(String::from_utf16_lossy(&u));
                at += n * 2;
            }
            // [album, artist, date, file name, genre, title]
            let title = if texts[5].trim().is_empty() { stem(&texts[3]) } else { texts[5].trim().to_string() };
            Some((title, texts[1].trim().to_string()))
        })();
        if !view.Value.is_null() {
            let _ = UnmapViewOfFile(view);
        }
        let _ = CloseHandle(map);
        out
    }
}

// ---- Winamp ----
fn winamp() -> Option<(HWND, Now)> {
    let h = winamp_window()?;
    let state = ask(h, WM_USER, 0, IPC_ISPLAYING)?;
    if state == 0 {
        return None;
    }
    let pos = ask(h, WM_USER, 0, IPC_GETOUTPUTTIME).filter(|v| *v >= 0 && *v < u32::MAX as isize).unwrap_or(0) as f64 / 1000.0;
    let dur = ask(h, WM_USER, 1, IPC_GETOUTPUTTIME).filter(|v| *v > 0 && *v < u32::MAX as isize).unwrap_or(0) as f64;
    let (title, artist) = winamp_title(&title_of(h));
    if title.is_empty() {
        return None;
    }
    Some((h, Now { title, artist, playing: state == 1, pos, dur, app: "winamp.exe" }))
}

/// Winamp's own window. AIMP makes a "Winamp v1.7" window too (for Winamp plugins), so only the
/// one that belongs to winamp.exe counts.
fn winamp_window() -> Option<HWND> {
    let mut after = None;
    for _ in 0..8 {
        let h = unsafe { FindWindowExW(None, after, w!("Winamp v1.7"), PCWSTR::null()) }.ok().filter(|h| !h.is_invalid())?;
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
        if crate::win::process_name(pid).eq_ignore_ascii_case("winamp") {
            return Some(h);
        }
        after = Some(h);
    }
    None
}

/// "12. Artist - Title - Winamp [Paused]" -> ("Title", "Artist")
pub fn winamp_title(t: &str) -> (String, String) {
    let mut t = t.trim();
    for tail in [" [Paused]", " [Stopped]"] {
        t = t.strip_suffix(tail).unwrap_or(t);
    }
    t = t.strip_suffix(" - Winamp").unwrap_or(t).trim();
    if let Some((n, rest)) = t.split_once(". ") {
        if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
            t = rest;
        }
    }
    match t.split_once(" - ") {
        Some((artist, title)) if !artist.trim().is_empty() && !title.trim().is_empty() => (title.trim().to_string(), artist.trim().to_string()),
        _ => (t.to_string(), String::new()),
    }
}

// ---- MPC-HC ----
fn mpc(heard: &mut Heard) -> Option<(HWND, Now)> {
    let h = window(w!("MediaPlayerClassicW"))?;
    let t = title_of(h);
    let t = t.strip_suffix(" - MPC-HC").unwrap_or(&t).trim().to_string();
    if t.is_empty() || t == "Media Player Classic Home Cinema" || t == "MPC-HC" {
        return None; // nothing open
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    let playing = heard.playing(pid);
    Some((h, Now { title: stem(&t), artist: String::new(), playing, pos: 0.0, dur: 0.0, app: "mpc-hc64.exe" }))
}

/// "Playing" for a player that doesn't say: its own audio session is active and made a sound in
/// the last 10 s (quiet passages stay "playing"), as for VLC.
#[derive(Default)]
pub struct Heard {
    pid: u32,
    at: Option<Instant>,
}

impl Heard {
    pub fn playing(&mut self, pid: u32) -> bool {
        if pid != self.pid {
            self.pid = pid;
            self.at = None;
        }
        let look = || -> windows::core::Result<(bool, f32)> {
            let (mut active, mut peak) = (false, 0f32);
            unsafe {
                let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
                let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
                let list = mgr.GetSessionEnumerator()?;
                for i in 0..list.GetCount()? {
                    let Ok(s) = list.GetSession(i) else { continue };
                    let Ok(c) = s.cast::<IAudioSessionControl2>() else { continue };
                    if c.GetProcessId().ok() != Some(pid) {
                        continue;
                    }
                    if c.GetState().ok() == Some(AudioSessionStateActive) {
                        active = true;
                    }
                    if let Ok(m) = s.cast::<IAudioMeterInformation>() {
                        if let Ok(v) = m.GetPeakValue() {
                            peak = peak.max(v);
                        }
                    }
                }
            }
            Ok((active, peak))
        };
        let (active, peak) = look().unwrap_or((false, 0.0));
        if active && peak > 0.0005 {
            self.at = Some(Instant::now());
        }
        active && self.at.is_some_and(|t| t.elapsed() < Duration::from_secs(10))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winamp_titles() {
        assert_eq!(winamp_title("1. Nova Test - Track One - Winamp"), ("Track One".into(), "Nova Test".into()));
        assert_eq!(winamp_title("12. DJ Mike Llama - Llama Whippin' Intro - Winamp [Stopped]"), ("Llama Whippin' Intro".into(), "DJ Mike Llama".into()));
        assert_eq!(winamp_title("3. Just a title - Winamp [Paused]"), ("Just a title".into(), String::new()));
        assert_eq!(winamp_title("Winamp"), ("Winamp".into(), String::new()));
    }

    #[test]
    fn stems() {
        assert_eq!(stem(r"C:\Music\Nova Test - Track Two.wav"), "Nova Test - Track Two");
        assert_eq!(stem("Nova Test - Track One.flac"), "Nova Test - Track One");
        assert_eq!(stem("No extension here"), "No extension here");
    }
}

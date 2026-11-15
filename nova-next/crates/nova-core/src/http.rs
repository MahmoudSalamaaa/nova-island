//! HTTP through Windows' own WinHTTP (nothing bundled). Like Node's fetch in 1.7: no system proxy,
//! a timeout per request, redirects followed. Blocking: call it from a worker thread.
use windows::{
    core::{HSTRING, PCWSTR},
    Win32::Networking::WinHttp::*,
};

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: String,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

struct Handle(*mut core::ffi::c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// GET (or POST with `body`) `url`; `headers` as "Name: value\r\n" lines. Err("timeout") on a timeout.
pub fn request(method: &str, url: &str, headers: &str, body: Option<&[u8]>, timeout_ms: i32) -> Result<Response, String> {
    request_progress(method, url, headers, body, timeout_ms, &mut |_, _| true)
}

/// request() reporting progress: `on_data(received, content_length or 0)`; return false to stop
/// (the request then fails with "too big").
pub fn request_progress(method: &str, url: &str, headers: &str, body: Option<&[u8]>, timeout_ms: i32, on_data: &mut dyn FnMut(usize, usize) -> bool) -> Result<Response, String> {
    let wurl: Vec<u16> = url.encode_utf16().chain([0]).collect();
    let mut host = vec![0u16; 256];
    let mut path = vec![0u16; 4096];
    let mut parts = URL_COMPONENTS {
        dwStructSize: std::mem::size_of::<URL_COMPONENTS>() as u32,
        lpszHostName: windows::core::PWSTR(host.as_mut_ptr()),
        dwHostNameLength: host.len() as u32,
        lpszUrlPath: windows::core::PWSTR(path.as_mut_ptr()),
        dwUrlPathLength: path.len() as u32,
        ..Default::default()
    };
    let mut extra = vec![0u16; 4096];
    parts.lpszExtraInfo = windows::core::PWSTR(extra.as_mut_ptr());
    parts.dwExtraInfoLength = extra.len() as u32;
    unsafe {
        WinHttpCrackUrl(&wurl[..wurl.len() - 1], 0, &mut parts).map_err(|_| "bad URL".to_string())?;
        let secure = parts.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS;
        let host_s = String::from_utf16_lossy(&host[..parts.dwHostNameLength as usize]);
        let target = format!("{}{}", String::from_utf16_lossy(&path[..parts.dwUrlPathLength as usize]), String::from_utf16_lossy(&extra[..parts.dwExtraInfoLength as usize]));
        let session = Handle(WinHttpOpen(&HSTRING::from("Nova Island"), WINHTTP_ACCESS_TYPE_NO_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err("network unavailable".into());
        }
        let _ = WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms);
        let conn = Handle(WinHttpConnect(session.0, &HSTRING::from(host_s), parts.nPort, 0));
        if conn.0.is_null() {
            return Err("fetch failed".into());
        }
        let req = Handle(WinHttpOpenRequest(conn.0, &HSTRING::from(method), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), if secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) }));
        if req.0.is_null() {
            return Err("fetch failed".into());
        }
        let h: Vec<u16> = headers.encode_utf16().collect();
        let sent = WinHttpSendRequest(req.0, if h.is_empty() { None } else { Some(&h) }, body.map(|b| b.as_ptr() as *const _), body.map_or(0, |b| b.len() as u32), body.map_or(0, |b| b.len() as u32), 0);
        let timeout = |e: windows::core::Error| if e.code().0 as u32 & 0xffff == ERROR_WINHTTP_TIMEOUT { "timeout".to_string() } else { "fetch failed".to_string() };
        sent.map_err(timeout)?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(timeout)?;
        let mut status = 0u32;
        let mut size = 4u32;
        WinHttpQueryHeaders(req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut status as *mut u32 as *mut _), &mut size, std::ptr::null_mut()).map_err(|_| "fetch failed".to_string())?;
        let mut ct = [0u16; 256];
        let mut ct_size = (ct.len() * 2) as u32;
        let content_type = if WinHttpQueryHeaders(req.0, WINHTTP_QUERY_CONTENT_TYPE, PCWSTR::null(), Some(ct.as_mut_ptr() as *mut _), &mut ct_size, std::ptr::null_mut()).is_ok() {
            String::from_utf16_lossy(&ct[..ct_size as usize / 2])
        } else {
            String::new()
        };
        let mut len = 0u32;
        let mut len_size = 4u32;
        let total = if WinHttpQueryHeaders(req.0, WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut len as *mut u32 as *mut _), &mut len_size, std::ptr::null_mut()).is_ok() { len as usize } else { 0 };
        let mut body = Vec::new();
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(req.0, &mut avail).map_err(timeout)?;
            if avail == 0 {
                break;
            }
            let start = body.len();
            body.resize(start + avail as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(req.0, body[start..].as_mut_ptr() as *mut _, avail, &mut read).map_err(timeout)?;
            body.truncate(start + read as usize);
            if read == 0 {
                break;
            }
            if !on_data(body.len(), total) {
                return Err("too big".into());
            }
        }
        Ok(Response { status: status as u16, body, content_type })
    }
}

pub fn get(url: &str, timeout_ms: i32) -> Result<Response, String> {
    request("GET", url, "", None, timeout_ms)
}

/// getJson() in system.js: JSON from a URL, or an error for a non-OK answer or a page that isn't JSON.
pub fn get_json(url: &str, timeout_ms: i32) -> Result<serde_json::Value, String> {
    let r = get(url, timeout_ms)?;
    if !r.ok() {
        return Err(format!("HTTP {}", r.status));
    }
    serde_json::from_slice(&r.body).map_err(|_| "not JSON".to_string())
}

/// encodeURIComponent
pub fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

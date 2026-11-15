//! Icons as PNG data URLs, like Electron's nativeImage / app.getFileIcon (apps.js, shelf.js):
//!   png_scaled   a Store app's logo file resized to 64x64 (high-quality scaler)
//!   file_icon    the shell's large icon of a file, folder, exe or shortcut (32x32)
//! Images go through Windows Imaging Component (part of Windows), nothing bundled.
use windows::{
    core::{HSTRING, PCWSTR},
    Win32::{
        Graphics::Imaging::*,
        Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED},
        UI::{
            Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON},
            WindowsAndMessaging::{DestroyIcon, HICON},
        },
    },
};

fn factory() -> Option<IWICImagingFactory> {
    // callers include fresh threads (the screenshot thumbnail) where COM isn't started yet: without
    // it there was no factory and the toast showed no picture. A thread that already has COM
    // (the UI thread) keeps its own mode; the call only answers that it's already set.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()
    }
}

/// A WIC bitmap source to "data:image/png;base64,..."
fn to_png(f: &IWICImagingFactory, src: &IWICBitmapSource) -> Option<String> {
    unsafe {
        let stream = f.CreateStream().ok()?;
        // an in-memory IStream to write into
        let mem = windows::Win32::UI::Shell::SHCreateMemStream(None)?;
        stream.InitializeFromIStream(&mem).ok()?;
        let enc = f.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).ok()?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame = None;
        enc.CreateNewFrame(&mut frame, std::ptr::null_mut()).ok()?;
        let frame = frame?;
        frame.Initialize(None).ok()?;
        let (mut w, mut h) = (0, 0);
        src.GetSize(&mut w, &mut h).ok()?;
        frame.SetSize(w, h).ok()?;
        let mut fmt = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut fmt).ok()?;
        frame.WriteSource(src, std::ptr::null()).ok()?;
        frame.Commit().ok()?;
        enc.Commit().ok()?;
        // read the stream back
        let mut stat = Default::default();
        mem.Stat(&mut stat, windows::Win32::System::Com::STATFLAG_NONAME).ok()?;
        let n = stat.cbSize as usize;
        mem.Seek(0, windows::Win32::System::Com::STREAM_SEEK_SET, None).ok()?;
        let mut buf = vec![0u8; n];
        let mut read = 0u32;
        mem.Read(buf.as_mut_ptr() as *mut _, n as u32, Some(&mut read)).ok().ok()?;
        buf.truncate(read as usize);
        Some(format!("data:image/png;base64,{}", crate::media::base64(&buf)))
    }
}

/// A logo file resized to `size`x`size`.
pub fn png_scaled(path: &str, size: u32) -> String {
    let go = || -> Option<String> {
        let f = factory()?;
        unsafe {
            let dec = f.CreateDecoderFromFilename(&HSTRING::from(path), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
            let frame = dec.GetFrame(0).ok()?;
            let conv = f.CreateFormatConverter().ok()?;
            conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
            let scaler = f.CreateBitmapScaler().ok()?;
            scaler.Initialize(&conv, size, size, WICBitmapInterpolationModeHighQualityCubic).ok()?;
            let out = f.CreateFormatConverter().ok()?;
            out.Initialize(&scaler, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
            to_png(&f, &out.cast().ok()?)
        }
    };
    go().unwrap_or_default()
}

/// The shell's large (32x32) icon of a path.
pub fn file_icon(path: &str) -> String {
    let go = || -> Option<String> {
        let f = factory()?;
        unsafe {
            let mut info = SHFILEINFOW::default();
            let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
            if SHGetFileInfoW(PCWSTR(w.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(0), Some(&mut info), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON) == 0 || info.hIcon.is_invalid() {
                return None;
            }
            let icon: HICON = info.hIcon;
            let bmp = f.CreateBitmapFromHICON(icon).ok();
            let _ = DestroyIcon(icon);
            to_png(&f, &bmp?.cast().ok()?)
        }
    };
    go().unwrap_or_default()
}

/// A picture's own thumbnail as Explorer shows it (screenshots and photos on the Shelf, 1.8),
/// `size` px on its long side; for any other file, or when there's none, its icon (file_icon).
pub fn file_thumb(path: &str, size: i32) -> String {
    const PICTURES: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "heic", "tif", "tiff"];
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !PICTURES.contains(&ext.as_str()) {
        return file_icon(path);
    }
    let go = || -> Option<String> {
        use windows::Win32::{
            Foundation::SIZE,
            Graphics::Gdi::DeleteObject,
            UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_THUMBNAILONLY},
        };
        let f = factory()?;
        unsafe {
            let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
            let bmp = item.GetImage(SIZE { cx: size, cy: size }, SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK).ok()?;
            let src = f.CreateBitmapFromHBITMAP(bmp, windows::Win32::Graphics::Gdi::HPALETTE::default(), WICBitmapUsePremultipliedAlpha);
            let _ = DeleteObject(bmp.into());
            to_png(&f, &src.ok()?.cast().ok()?)
        }
    };
    go().filter(|s| !s.is_empty()).unwrap_or_else(|| file_icon(path))
}

use windows::core::Interface;

/// Top-down BGRA to PNG bytes, scaled to `height` (keeping the shape) when given.
pub fn bgra_png(w: u32, h: u32, bgra: &[u8], height: Option<u32>) -> Option<Vec<u8>> {
    let f = factory()?;
    unsafe {
        let bmp = f.CreateBitmapFromMemory(w, h, &GUID_WICPixelFormat32bppBGRA, w * 4, bgra).ok()?;
        let src: IWICBitmapSource = match height {
            Some(th) if th != h => {
                let tw = ((w as f64 * th as f64 / h as f64).round() as u32).max(1);
                let scaler = f.CreateBitmapScaler().ok()?;
                scaler.Initialize(&bmp, tw, th, WICBitmapInterpolationModeFant).ok()?;
                scaler.cast().ok()?
            }
            _ => bmp.cast().ok()?,
        };
        let url = to_png(&f, &src)?;
        let b64 = url.strip_prefix("data:image/png;base64,")?;
        Some(decode_b64(b64))
    }
}

fn decode_b64(s: &str) -> Vec<u8> {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        bits = (bits << 6) | v as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    out
}

/// An image file's bytes (an app's logo) resized to `size`x`size`, as a PNG data URL, with the
/// colour that stands out in it: the saturated pixels count most, so a grey edge doesn't wash out
/// the brand colour. None when it can't be read.
pub fn png_and_color(bytes: &[u8], size: u32) -> Option<(String, [u8; 3])> {
    let f = factory()?;
    unsafe {
        let mem = windows::Win32::UI::Shell::SHCreateMemStream(Some(bytes))?;
        let dec = f.CreateDecoderFromStream(&mem, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let scaler = f.CreateBitmapScaler().ok()?;
        scaler.Initialize(&conv, size, size, WICBitmapInterpolationModeHighQualityCubic).ok()?;
        let out = f.CreateFormatConverter().ok()?;
        out.Initialize(&scaler, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let mut px = vec![0u8; (size * size * 4) as usize];
        out.CopyPixels(std::ptr::null(), size * 4, &mut px).ok()?;
        Some((to_png(&f, &out.cast().ok()?)?, color_of(&px)))
    }
}

/// The colour that stands out in BGRA pixels: saturated ones count most.
fn color_of(bgra: &[u8]) -> [u8; 3] {
    let (mut sum, mut weight) = ([0f64; 3], 0f64);
    for p in bgra.chunks_exact(4) {
        let (b, g, r, a) = (p[0] as f64, p[1] as f64, p[2] as f64, p[3] as f64);
        if a < 128.0 {
            continue;
        }
        let sat = r.max(g).max(b) - r.min(g).min(b);
        let w = 1.0 + sat * sat;
        sum[0] += r * w;
        sum[1] += g * w;
        sum[2] += b * w;
        weight += w;
    }
    if weight > 0.0 { [(sum[0] / weight) as u8, (sum[1] / weight) as u8, (sum[2] / weight) as u8] } else { [120, 140, 255] }
}

/// An app's icon as the Start menu shows it (shell:AppsFolder\<AppUserModelId>), for apps
/// outside the Store whose notifications carry no logo; with its colour.
pub fn start_icon(aumid: &str, size: i32) -> Option<(String, [u8; 3])> {
    use windows::Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS},
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY},
    };
    if aumid.is_empty() {
        return None;
    }
    unsafe {
        let f: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(format!("shell:AppsFolder\\{aumid}")), None).ok()?;
        let bmp = f.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY).ok()?;
        let mut info = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: size, biHeight: -size, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() }, ..Default::default() };
        let mut px = vec![0u8; (size * size * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(dc, bmp, 0, size as u32, Some(px.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(bmp.into());
        if lines == 0 {
            return None;
        }
        let png = bgra_png(size as u32, size as u32, &px, None)?;
        Some((format!("data:image/png;base64,{}", crate::media::base64(&png)), color_of(&px)))
    }
}

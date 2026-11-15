//! A widget as a picture, for Settings › Widgets' preview: the face drawn by the same code as on
//! the desktop, into a bitmap instead of a window, on Nova's glass tone (a picture has no
//! wallpaper behind it), saved as PNG.
use windows::{
    core::{Interface, Result},
    Win32::{
        Graphics::{
            Direct2D::{Common::*, *},
            Dxgi::Common::*,
            Imaging::*,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, IStream, CLSCTX_ALL, COINIT_APARTMENTTHREADED, STATFLAG_NONAME, STREAM_SEEK_SET},
        UI::Shell::SHCreateMemStream,
    },
};

use crate::{
    font::Fonts,
    paint::{rect, rgba},
    widget::{Ctx, Face, RADIUS},
};

/// The face at `scale` (1 = 96 DPI), as PNG bytes.
pub fn png(face: &mut dyn Face, scale: f32, fonts: &Fonts) -> Result<Vec<u8>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let (w, h) = face.size();
        let (pw, ph) = ((w * scale).round() as u32, (h * scale).round() as u32);
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_ALL)?;
        let bmp = wic.CreateBitmap(pw, ph, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0 * scale,
            dpiY: 96.0 * scale,
            ..Default::default()
        };
        let rt = d2d.CreateWicBitmapRenderTarget(&bmp, &props)?;
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
        // the glass: a deep blue like Nova's frosted card over a dark wallpaper
        let stops = [D2D1_GRADIENT_STOP { position: 0.0, color: rgba(36, 46, 92, 1.0) }, D2D1_GRADIENT_STOP { position: 1.0, color: rgba(18, 22, 44, 1.0) }];
        let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
        let glass = rt.CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: crate::paint::pt(0.0, 0.0), endPoint: crate::paint::pt(w, h) }, None, &coll)?;
        rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect(0.0, 0.0, w, h), radiusX: RADIUS, radiusY: RADIUS }, &glass);
        let ctx = Ctx { rt: &rt, d2d: &d2d, fonts };
        let drawn = face.draw(&ctx);
        rt.EndDraw(None, None)?;
        drawn?;

        // to PNG, in memory
        let stream: IStream = SHCreateMemStream(None).ok_or_else(windows::core::Error::from_thread)?;
        let enc = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        enc.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        enc.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.ok_or_else(windows::core::Error::empty)?;
        frame.Initialize(None)?;
        frame.SetSize(pw, ph)?;
        let mut fmt = GUID_WICPixelFormat32bppPBGRA;
        frame.SetPixelFormat(&mut fmt)?;
        frame.WriteSource(&bmp.cast::<IWICBitmapSource>()?, std::ptr::null())?;
        frame.Commit()?;
        enc.Commit()?;
        let mut st = Default::default();
        stream.Stat(&mut st, STATFLAG_NONAME)?;
        let mut out = vec![0u8; st.cbSize as usize];
        stream.Seek(0, STREAM_SEEK_SET, None)?;
        let mut read = 0u32;
        stream.Read(out.as_mut_ptr() as _, out.len() as u32, Some(&mut read)).ok()?;
        out.truncate(read as usize);
        Ok(out)
    }
}

/// Bytes as base64 (for a data: address).
pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_matches() {
        assert_eq!(base64(b"Nova"), "Tm92YQ==");
        assert_eq!(base64(b"Nov"), "Tm92");
        assert_eq!(base64(b"No"), "Tm8=");
    }
    #[test]
    fn a_widget_as_png() {
        let fonts = Fonts::new().unwrap();
        let mut f = crate::date_faces::DayName;
        let p = png(&mut f, 1.0, &fonts).unwrap();
        assert_eq!(&p[1..4], b"PNG");
        assert!(p.len() > 1000);
    }
}

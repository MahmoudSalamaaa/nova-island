//! Drawing helpers for the faces: the glass card, rings and arcs, gradients and text placed in a
//! box, measured or not. Every size is in DIPs (the render target carries the screen's scale).
use windows::{
    core::Result,
    Win32::Graphics::{
        Direct2D::{Common::*, *},
        DirectWrite::*,
    },
};
use windows_numerics::Vector2;

use crate::widget::Ctx;

/// A colour from 0..255 channels and an alpha 0..1.
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}
pub fn white(a: f32) -> D2D1_COLOR_F {
    rgba(255, 255, 255, a)
}
/// Nova's gradient ends
pub const BLUE: (u8, u8, u8) = (111, 140, 255);
pub const VIOLET: (u8, u8, u8) = (178, 107, 255);
pub fn c3(c: (u8, u8, u8), a: f32) -> D2D1_COLOR_F {
    rgba(c.0, c.1, c.2, a)
}

pub fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}
pub fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

/// Where text sits in its box, left to right as seen.
#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// The font of a line of text.
#[derive(Clone, Copy)]
pub struct Font {
    pub family: &'static str,
    pub size: f32,
    pub weight: DWRITE_FONT_WEIGHT,
}
pub const fn ui(size: f32, weight: i32) -> Font {
    Font { family: crate::font::UI, size, weight: DWRITE_FONT_WEIGHT(weight) }
}
pub const fn ruqaa(size: f32) -> Font {
    Font { family: crate::font::CALLIGRAPHY, size, weight: DWRITE_FONT_WEIGHT_BOLD }
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
/// Arabic letters in it (read right to left)
fn rtl(s: &str) -> bool {
    s.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c))
}

pub struct P<'a> {
    pub ctx: &'a Ctx<'a>,
}

impl<'a> P<'a> {
    pub fn new(ctx: &'a Ctx<'a>) -> P<'a> {
        P { ctx }
    }
    fn rt(&self) -> &ID2D1RenderTarget {
        self.ctx.rt
    }

    pub fn solid(&self, c: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {
        unsafe { self.rt().CreateSolidColorBrush(&c, None) }
    }
    /// A straight gradient from `a` (at p0) to `b` (at p1).
    pub fn linear(&self, p0: (f32, f32), p1: (f32, f32), a: D2D1_COLOR_F, b: D2D1_COLOR_F) -> Result<ID2D1LinearGradientBrush> {
        self.linear_stops(p0, p1, &[(0.0, a), (1.0, b)])
    }
    pub fn linear_stops(&self, p0: (f32, f32), p1: (f32, f32), stops: &[(f32, D2D1_COLOR_F)]) -> Result<ID2D1LinearGradientBrush> {
        unsafe {
            let s: Vec<D2D1_GRADIENT_STOP> = stops.iter().map(|(p, c)| D2D1_GRADIENT_STOP { position: *p, color: *c }).collect();
            let coll = self.rt().CreateGradientStopCollection(&s, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            self.rt().CreateLinearGradientBrush(&D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(p0.0, p0.1), endPoint: pt(p1.0, p1.1) }, None, &coll)
        }
    }
    /// Nova's blue-to-violet gradient across a box, top-left to bottom-right.
    pub fn nova(&self, r: D2D_RECT_F) -> Result<ID2D1LinearGradientBrush> {
        self.linear((r.left, r.top), (r.right, r.bottom), c3(BLUE, 1.0), c3(VIOLET, 1.0))
    }
    pub fn radial(&self, c: (f32, f32), rx: f32, ry: f32, stops: &[(f32, D2D1_COLOR_F)]) -> Result<ID2D1RadialGradientBrush> {
        unsafe {
            let s: Vec<D2D1_GRADIENT_STOP> = stops.iter().map(|(p, c)| D2D1_GRADIENT_STOP { position: *p, color: *c }).collect();
            let coll = self.rt().CreateGradientStopCollection(&s, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            self.rt().CreateRadialGradientBrush(
                &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES { center: pt(c.0, c.1), gradientOriginOffset: pt(0.0, 0.0), radiusX: rx, radiusY: ry },
                None,
                &coll,
            )
        }
    }

    /// The widget's glass: Nova's tint over the frosted backdrop and a hairline edge.
    pub fn card(&self, w: f32, h: f32) -> Result<()> {
        unsafe {
            let card = D2D1_ROUNDED_RECT { rect: rect(0.5, 0.5, w - 0.5, h - 0.5), radiusX: crate::widget::RADIUS, radiusY: crate::widget::RADIUS };
            self.rt().FillRoundedRectangle(&card, &self.solid(rgba(22, 26, 46, 0.42))?);
            self.rt().DrawRoundedRectangle(&card, &self.solid(white(0.10))?, 1.0, None);
        }
        Ok(())
    }

    pub fn fill_round(&self, r: D2D_RECT_F, radius: f32, b: &ID2D1Brush) {
        unsafe { self.rt().FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius }, b) }
    }
    pub fn stroke_round(&self, r: D2D_RECT_F, radius: f32, b: &ID2D1Brush, width: f32) {
        unsafe { self.rt().DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r, radiusX: radius, radiusY: radius }, b, width, None) }
    }
    pub fn fill_circle(&self, cx: f32, cy: f32, r: f32, b: &ID2D1Brush) {
        unsafe { self.rt().FillEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r }, b) }
    }
    pub fn stroke_circle(&self, cx: f32, cy: f32, r: f32, b: &ID2D1Brush, width: f32) {
        unsafe { self.rt().DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r }, b, width, None) }
    }
    pub fn line(&self, a: (f32, f32), b: (f32, f32), br: &ID2D1Brush, width: f32, round: bool) -> Result<()> {
        unsafe {
            let st = if round { Some(self.caps(None)?) } else { None };
            self.rt().DrawLine(pt(a.0, a.1), pt(b.0, b.1), br, width, st.as_ref());
        }
        Ok(())
    }
    /// Round ends, and dashes (in stroke widths) if any.
    pub fn caps(&self, dashes: Option<&[f32]>) -> Result<ID2D1StrokeStyle> {
        unsafe {
            let p = D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                dashStyle: if dashes.is_some() { D2D1_DASH_STYLE_CUSTOM } else { D2D1_DASH_STYLE_SOLID },
                ..Default::default()
            };
            self.ctx.d2d.CreateStrokeStyle(&p, dashes)
        }
    }

    /// A ring's arc: from 12 o'clock, `share` (0..1) of the way round clockwise, round ends.
    pub fn ring_arc(&self, cx: f32, cy: f32, r: f32, share: f32, b: &ID2D1Brush, width: f32) -> Result<()> {
        let sweep = (share.clamp(0.0, 1.0) * 359.9).max(0.5);
        self.arc_deg(cx, cy, r, r, -90.0, -90.0 + sweep, b, width, None)
    }
    /// An elliptic arc between two angles (degrees, 0 = 3 o'clock, growing clockwise on screen),
    /// drawn from `a0` towards `a1`.
    #[allow(clippy::too_many_arguments)]
    pub fn arc_deg(&self, cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, b: &ID2D1Brush, width: f32, dashes: Option<&[f32]>) -> Result<()> {
        unsafe {
            let (s, e) = (a0.to_radians(), a1.to_radians());
            let geo = self.ctx.d2d.CreatePathGeometry()?;
            let sink = geo.Open()?;
            sink.BeginFigure(pt(cx + rx * s.cos(), cy + ry * s.sin()), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: pt(cx + rx * e.cos(), cy + ry * e.sin()),
                size: D2D_SIZE_F { width: rx, height: ry },
                rotationAngle: 0.0,
                sweepDirection: if a1 >= a0 { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                arcSize: if (a1 - a0).abs() > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            sink.Close()?;
            self.rt().DrawGeometry(&geo, b, width, &self.caps(dashes)?);
        }
        Ok(())
    }
    /// A closed or open path through points (a sparkline, a filled area).
    pub fn poly(&self, pts: &[(f32, f32)], b: &ID2D1Brush, fill: bool, width: f32) -> Result<()> {
        if pts.len() < 2 {
            return Ok(());
        }
        unsafe {
            let geo = self.ctx.d2d.CreatePathGeometry()?;
            let sink = geo.Open()?;
            sink.BeginFigure(pt(pts[0].0, pts[0].1), if fill { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW });
            for p in &pts[1..] {
                sink.AddLine(pt(p.0, p.1));
            }
            sink.EndFigure(if fill { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
            sink.Close()?;
            if fill {
                self.rt().FillGeometry(&geo, b, None);
            } else {
                self.rt().DrawGeometry(&geo, b, width, &self.caps(None)?);
            }
        }
        Ok(())
    }

    fn layout(&self, s: &str, f: Font, w: f32, h: f32, align: Align) -> Result<IDWriteTextLayout> {
        unsafe {
            let r = rtl(s);
            let fmt = self.ctx.fonts.format(f.family, f.size, f.weight, r)?;
            fmt.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            // the box's sides as seen: in right-to-left text "leading" is the right side
            let a = match (align, r) {
                (Align::Center, _) => DWRITE_TEXT_ALIGNMENT_CENTER,
                (Align::Right, true) | (Align::Left, false) => DWRITE_TEXT_ALIGNMENT_LEADING,
                _ => DWRITE_TEXT_ALIGNMENT_TRAILING,
            };
            fmt.SetTextAlignment(a)?;
            let t = utf16(s);
            self.ctx.fonts.factory.CreateTextLayout(&t, &fmt, w.max(1.0), h.max(1.0))
        }
    }
    /// One line of text, placed in a box.
    pub fn text(&self, s: &str, f: Font, r: D2D_RECT_F, align: Align, b: &ID2D1Brush) -> Result<()> {
        self.text_x(s, f, r, align, b, false)
    }
    /// One line of text, struck through or not.
    pub fn text_x(&self, s: &str, f: Font, r: D2D_RECT_F, align: Align, b: &ID2D1Brush, strike: bool) -> Result<()> {
        unsafe {
            let l = self.layout(s, f, r.right - r.left, r.bottom - r.top, align)?;
            if strike {
                l.SetStrikethrough(true, DWRITE_TEXT_RANGE { startPosition: 0, length: s.encode_utf16().count() as u32 })?;
            }
            // too long for the box: ends with "…"
            let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
            let sign = self.ctx.fonts.factory.CreateEllipsisTrimmingSign(&l)?;
            l.SetTrimming(&trim, &sign)?;
            self.rt().DrawTextLayout(pt(r.left, r.top), &l, b, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
        }
        Ok(())
    }
    /// The width a line of text takes.
    pub fn measure(&self, s: &str, f: Font) -> Result<f32> {
        unsafe {
            let l = self.layout(s, f, 4000.0, 400.0, Align::Left)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            l.GetMetrics(&mut m)?;
            Ok(m.widthIncludingTrailingWhitespace)
        }
    }

    pub fn geometry(&self) -> Result<ID2D1PathGeometry> {
        unsafe { self.ctx.d2d.CreatePathGeometry() }
    }
    pub fn fill_geo(&self, g: &ID2D1PathGeometry, b: &ID2D1Brush) {
        unsafe { self.rt().FillGeometry(g, b, None) }
    }
    pub fn stroke_geo(&self, g: &ID2D1PathGeometry, b: &ID2D1Brush, width: f32) {
        unsafe { self.rt().DrawGeometry(g, b, width, None) }
    }
}

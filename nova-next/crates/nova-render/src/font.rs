//! Text with DirectWrite: Windows' own fonts, plus Aref Ruqaa (the calligraphy for the day and
//! month names; SIL Open Font License, assets/fonts/OFL.txt) loaded from inside the app, so
//! nothing is installed on the PC.
use windows::{
    core::{Interface, Result, HSTRING},
    Win32::Graphics::DirectWrite::*,
};

static RUQAA: &[u8] = include_bytes!("../../../assets/fonts/ArefRuqaa-Regular.ttf");
static RUQAA_BOLD: &[u8] = include_bytes!("../../../assets/fonts/ArefRuqaa-Bold.ttf");

pub const CALLIGRAPHY: &str = "Aref Ruqaa";
pub const UI: &str = "Segoe UI Variable Display";

pub struct Fonts {
    pub factory: IDWriteFactory5,
    /// Aref Ruqaa, from memory
    own: IDWriteFontCollection1,
}

impl Fonts {
    pub fn new() -> Result<Fonts> {
        unsafe {
            let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loader = factory.CreateInMemoryFontFileLoader()?;
            factory.RegisterFontFileLoader(&loader)?;
            let builder: IDWriteFontSetBuilder1 = factory.CreateFontSetBuilder()?.cast()?;
            for data in [RUQAA, RUQAA_BOLD] {
                // no owner: DirectWrite keeps its own copy of the bytes
                let file = loader.CreateInMemoryFontFileReference(&factory, data.as_ptr() as _, data.len() as u32, None)?;
                builder.AddFontFile(&file)?;
            }
            let set = builder.CreateFontSet()?;
            let own = factory.CreateFontCollectionFromFontSet(&set)?;
            Ok(Fonts { factory, own })
        }
    }

    /// A text format: `family` from the app's own fonts (Aref Ruqaa) or Windows'.
    pub fn format(&self, family: &str, size: f32, weight: DWRITE_FONT_WEIGHT, rtl: bool) -> Result<IDWriteTextFormat> {
        unsafe {
            let own = family == CALLIGRAPHY;
            let collection: Option<IDWriteFontCollection> = if own { Some(self.own.cast()?) } else { None };
            let f = self.factory.CreateTextFormat(
                &HSTRING::from(family),
                collection.as_ref(),
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                &HSTRING::from(if rtl { "ar" } else { "en-us" }),
            )?;
            f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            if rtl {
                f.SetReadingDirection(DWRITE_READING_DIRECTION_RIGHT_TO_LEFT)?;
            }
            Ok(f)
        }
    }
}

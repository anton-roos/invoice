//! Fonts for the printed invoice. The same font files are used for the
//! on-screen preview and the PDF, so both measure and look the same.

use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Regular,
    Bold,
    Italic,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Regular, Style::Bold, Style::Italic];

    /// Family name used when registering the font with egui.
    pub fn family_name(self) -> &'static str {
        match self {
            Style::Regular => "invoice-regular",
            Style::Bold => "invoice-bold",
            Style::Italic => "invoice-italic",
        }
    }
}

pub struct InvoiceFonts {
    regular: Arc<Vec<u8>>,
    bold: Arc<Vec<u8>>,
    italic: Arc<Vec<u8>>,
}

impl InvoiceFonts {
    /// Arial from the Windows fonts folder (the original invoice uses Arial),
    /// falling back to egui's built-in font if it can't be read.
    pub fn load() -> Self {
        let fonts_dir = std::env::var_os("WINDIR")
            .map(|w| PathBuf::from(w).join("Fonts"))
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows\Fonts"));
        let load = |file: &str| {
            let data = std::fs::read(fonts_dir.join(file))
                .ok()
                .filter(|d| ttf_parser::Face::parse(d, 0).is_ok())
                .unwrap_or_else(|| epaint_default_fonts::UBUNTU_LIGHT.to_vec());
            Arc::new(data)
        };
        InvoiceFonts { regular: load("arial.ttf"), bold: load("arialbd.ttf"), italic: load("ariali.ttf") }
    }

    pub fn data(&self, style: Style) -> &Arc<Vec<u8>> {
        match style {
            Style::Regular => &self.regular,
            Style::Bold => &self.bold,
            Style::Italic => &self.italic,
        }
    }

    fn face(&self, style: Style) -> ttf_parser::Face<'_> {
        ttf_parser::Face::parse(self.data(style), 0).expect("font validated on load")
    }

    /// Advance width of `text` in points.
    pub fn width(&self, text: &str, style: Style, size: f32) -> f32 {
        let face = self.face(style);
        let units: u32 = text
            .chars()
            .map(|c| {
                face.glyph_index(c).and_then(|g| face.glyph_hor_advance(g)).unwrap_or(0) as u32
            })
            .sum();
        units as f32 * size / face.units_per_em() as f32
    }

    /// Distance from the top of a line to its baseline, in points.
    pub fn ascent(&self, style: Style, size: f32) -> f32 {
        let face = self.face(style);
        face.ascender() as f32 * size / face.units_per_em() as f32
    }
}

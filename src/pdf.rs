//! Render laid-out invoice pages to a PDF with krilla.

use std::sync::Arc;

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};

use crate::fonts::{InvoiceFonts, Style};
use crate::layout::{Item, PAGE_H, PAGE_W, Page, Rgb};
use crate::logo::LogoImage;

pub fn render(pages: &[Page], fonts: &InvoiceFonts, logo: Option<&LogoImage>) -> Result<Vec<u8>, String> {
    let font = |style: Style| {
        let data: Arc<Vec<u8>> = fonts.data(style).clone();
        Font::new(data.into(), 0).ok_or("could not load the invoice font")
    };
    let regular = font(Style::Regular)?;
    let bold = font(Style::Bold)?;
    let italic = font(Style::Italic)?;
    let logo = logo.map(embed_logo);

    let mut document = Document::new();
    for items in pages {
        let size = PageSettings::from_wh(PAGE_W, PAGE_H).ok_or("invalid page size")?;
        let mut page = document.start_page_with(size);
        let mut surface = page.surface();
        for item in items {
            match item {
                Item::Text { x, y, size, style, color, text } => {
                    let font = match style {
                        Style::Regular => &regular,
                        Style::Bold => &bold,
                        Style::Italic => &italic,
                    };
                    set_fill(&mut surface, *color);
                    let baseline = y + fonts.ascent(*style, *size);
                    surface.draw_text(
                        Point::from_xy(*x, baseline),
                        font.clone(),
                        *size,
                        text,
                        false,
                        TextDirection::Auto,
                    );
                }
                Item::Rect { x, y, w, h, color } => {
                    let Some(rect) = Rect::from_xywh(*x, *y, *w, *h) else { continue };
                    let mut path = PathBuilder::new();
                    path.push_rect(rect);
                    if let Some(path) = path.finish() {
                        set_fill(&mut surface, *color);
                        surface.draw_path(&path);
                    }
                }
                Item::Logo { x, y, w, h } => {
                    if let (Some(image), Some(size)) = (&logo, Size::from_wh(*w, *h)) {
                        surface.push_transform(&Transform::from_translate(*x, *y));
                        surface.draw_image(image.clone(), size);
                        surface.pop();
                    }
                }
            }
        }
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|e| format!("could not create the PDF: {e:?}"))
}

/// Embed JPEGs and PNGs unchanged (much smaller); anything else as raw pixels.
fn embed_logo(logo: &LogoImage) -> Image {
    let source = || Arc::new(logo.source.clone()).into();
    let embedded = match image::guess_format(&logo.source) {
        Ok(image::ImageFormat::Jpeg) => Image::from_jpeg(source(), true).ok(),
        Ok(image::ImageFormat::Png) => Image::from_png(source(), true).ok(),
        _ => None,
    };
    embedded.unwrap_or_else(|| Image::from_rgba8(logo.rgba.clone(), logo.width, logo.height))
}

fn set_fill(surface: &mut Surface, Rgb(r, g, b): Rgb) {
    surface.set_fill(Some(Fill { paint: rgb::Color::new(r, g, b).into(), ..Default::default() }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{InvoiceDoc, layout_invoice};
    use crate::model::Data;

    #[test]
    fn renders_seed_invoice() {
        let data = Data::from_json(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample-data.json"))).unwrap();
        let fonts = InvoiceFonts::load();
        let logo = crate::logo::load(&crate::logo::test_logo_url()).unwrap();
        let doc = InvoiceDoc {
            company: &data.company,
            client: data.clients.first(),
            settings: &data.settings,
            invoice: &data.invoices[0],
            logo_size: logo.as_ref().map(|l| (l.width, l.height)),
        };
        let pdf = render(&layout_invoice(&doc, &fonts), &fonts, logo.as_ref()).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }
}

//! Paint laid-out invoice pages on screen, using the same layout and fonts
//! as the PDF export.

use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Shadow, Stroke, Vec2};

use invoice::fonts::Style;
use invoice::layout::{Item, PAGE_H, PAGE_W, Page, Rgb};

pub fn show_pages(ui: &mut egui::Ui, pages: &[Page], logo: Option<&egui::TextureHandle>) {
    // Fit the page width to the panel, within reason.
    let scale = ((ui.available_width() - 24.0) / PAGE_W).clamp(0.35, 1.6);
    let size = Vec2::new(PAGE_W, PAGE_H) * scale;
    for page in pages {
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            if !ui.is_rect_visible(rect) {
                return;
            }
            let painter = ui.painter_at(rect.expand(12.0));
            let shadow = Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(40) };
            painter.add(shadow.as_shape(rect, 0.0));
            painter.rect_filled(rect, 0.0, Color32::WHITE);
            let at = |x: f32, y: f32| rect.min + Vec2::new(x, y) * scale;
            for item in page {
                match item {
                    Item::Text { x, y, size, style, color, text } => {
                        let font = FontId::new(size * scale, FontFamily::Name(style_family(*style).into()));
                        painter.text(at(*x, *y), egui::Align2::LEFT_TOP, text, font, color32(*color));
                    }
                    Item::Rect { x, y, w, h, color } => {
                        let r = Rect::from_min_size(at(*x, *y), Vec2::new(*w, *h) * scale);
                        painter.rect_filled(r, 0.0, color32(*color));
                    }
                    Item::Logo { x, y, w, h } => {
                        if let Some(tex) = logo {
                            let r = Rect::from_min_size(at(*x, *y), Vec2::new(*w, *h) * scale);
                            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                            painter.image(tex.id(), r, uv, Color32::WHITE);
                        }
                    }
                }
            }
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(215)), egui::StrokeKind::Outside);
        });
        ui.add_space(16.0);
    }
}

fn style_family(style: Style) -> &'static str {
    style.family_name()
}

fn color32(Rgb(r, g, b): Rgb) -> Color32 {
    Color32::from_rgb(r, g, b)
}

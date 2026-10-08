//! Company details, banking, numbering, the item catalog, and data files.

use eframe::egui::{self, RichText};

use invoice::logo;
use invoice::model::CatalogItem;

use super::{InvoiceApp, lines_of, open_path};

impl InvoiceApp {
    pub(super) fn settings_view(&mut self, ui: &mut egui::Ui) {
        let now = Self::now(ui.ctx());
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.set_max_width(760.0);

            // ---- Company
            section(ui, "Your company (From)");
            let mut company_changed = false;
            let company = &mut self.data.company;
            egui::Grid::new("company").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Company name");
                company_changed |= text(ui, &mut company.name);
                ui.end_row();
                ui.label("VAT no.");
                company_changed |= text(ui, &mut company.vat_no);
                ui.end_row();
                ui.label("Logo");
                ui.horizontal(|ui| {
                    if let Some(tex) = &self.logo.texture {
                        let size = tex.size_vec2();
                        let scale = (160.0 / size.x).min(44.0 / size.y);
                        ui.image((tex.id(), size * scale));
                    } else if let Some(err) = &self.logo.error {
                        ui.label(RichText::new(err).color(egui::Color32::from_rgb(0xb3, 0x26, 0x1e)));
                    } else {
                        ui.label(RichText::new("No logo — the company name is shown instead").weak());
                    }
                    if ui.button("Choose image…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Choose a logo")
                            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                            .pick_file()
                    {
                        match logo::to_data_url(&path) {
                            Ok(url) => {
                                company.logo = url;
                                company_changed = true;
                            }
                            Err(e) => self.toast = Some((e, f64::NAN)),
                        }
                    }
                    if !company.logo.is_empty() && ui.button("No logo").clicked() {
                        company.logo.clear();
                        company_changed = true;
                    }
                });
                ui.end_row();
            });
            ui.columns(2, |cols| {
                cols[0].label("Postal address (one line each)");
                if multiline(&mut cols[0], &mut self.postal_buf) {
                    company.postal_address = lines_of(&self.postal_buf);
                    company_changed = true;
                }
                cols[1].label("Physical address (one line each)");
                if multiline(&mut cols[1], &mut self.physical_buf) {
                    company.physical_address = lines_of(&self.physical_buf);
                    company_changed = true;
                }
            });

            // ---- Banking
            section(ui, "Banking details");
            let bank = &mut company.bank;
            egui::Grid::new("bank").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                for (label, value) in [
                    ("Bank", &mut bank.name),
                    ("Account holder", &mut bank.account_holder),
                    ("Account number", &mut bank.account_number),
                    ("Account type", &mut bank.account_type),
                    ("Branch code", &mut bank.branch_code),
                ] {
                    ui.label(label);
                    company_changed |= text(ui, value);
                    ui.end_row();
                }
            });
            if company_changed {
                self.company_dirty.get_or_insert(now);
            }

            // ---- Numbering
            section(ui, "Invoice numbering");
            let s = &mut self.data.settings;
            let mut settings_changed = false;
            egui::Grid::new("numbering").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Prefix");
                settings_changed |= text(ui, &mut s.number_prefix);
                ui.end_row();
                ui.label("Digits (zero-padded)");
                settings_changed |= ui.add(egui::DragValue::new(&mut s.number_padding).range(0..=12)).changed();
                ui.end_row();
                ui.label("Next number");
                settings_changed |= ui.add(egui::DragValue::new(&mut s.next_invoice_number).range(1..=i64::MAX)).changed();
                ui.end_row();
                ui.label("Currency symbol");
                settings_changed |= text(ui, &mut s.currency_symbol);
                ui.end_row();
            });
            ui.label(RichText::new(format!("Next invoice: {}", s.format_number(s.next_invoice_number))).weak());
            if settings_changed {
                self.settings_dirty.get_or_insert(now);
            }

            // ---- Catalog
            section(ui, "Catalog items");
            ui.label(RichText::new("Quick-fill for invoice line items.").weak());
            let items = &mut self.data.items;
            let mut items_changed = false;
            let mut remove = None;
            egui::Grid::new("catalog").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                for title in ["Code", "Description", "Unit price", ""] {
                    ui.label(RichText::new(title).size(12.0).weak());
                }
                ui.end_row();
                for (i, item) in items.iter_mut().enumerate() {
                    items_changed |=
                        ui.add_sized([90.0, 22.0], egui::TextEdit::singleline(&mut item.code)).changed();
                    items_changed |=
                        ui.add_sized([300.0, 22.0], egui::TextEdit::singleline(&mut item.description)).changed();
                    items_changed |= ui
                        .add(egui::DragValue::new(&mut item.unit_price).range(0.0..=f64::MAX).max_decimals(2))
                        .changed();
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                items.remove(i);
                items_changed = true;
            }
            if ui.button("+ Add item").clicked() {
                items.push(CatalogItem::default());
                items_changed = true;
            }
            if items_changed {
                self.items_dirty.get_or_insert(now);
            }

            // ---- Data
            section(ui, "Data");
            ui.label("Everything is saved automatically to a SQLite database on this PC:");
            ui.horizontal(|ui| {
                ui.monospace(self.db_path.display().to_string());
                if ui.button("Open folder").clicked()
                    && let Some(dir) = self.db_path.parent()
                {
                    open_path(dir);
                }
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Back up database…").on_hover_text("Save a complete copy of the database").clicked() {
                    self.backup_db();
                }
                if ui.button("Export data.json…").on_hover_text("Everything as JSON, readable by Import").clicked() {
                    self.export_json();
                }
                if ui.button("Import data.json…").on_hover_text("Replace all data with a data.json file").clicked() {
                    self.pick_import();
                }
            });
            ui.add_space(20.0);
        });
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(14.0);
    ui.heading(title);
    ui.separator();
}

fn text(ui: &mut egui::Ui, value: &mut String) -> bool {
    ui.add(egui::TextEdit::singleline(value).desired_width(320.0)).changed()
}

fn multiline(ui: &mut egui::Ui, value: &mut String) -> bool {
    ui.add(egui::TextEdit::multiline(value).desired_rows(5).desired_width(f32::INFINITY)).changed()
}

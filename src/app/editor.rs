//! Editing one invoice: the form on the left, the live printable preview on
//! the right.

use eframe::egui::{self, RichText};
use egui_extras::DatePickerButton;

use invoice::model::{Invoice, LineItem};

use super::{Confirm, InvoiceApp, View, blank_line, preview};

impl InvoiceApp {
    pub(super) fn editor_view(&mut self, ui: &mut egui::Ui) {
        let now = Self::now(ui.ctx());
        let Some(ed) = &mut self.editor else {
            self.set_view(View::Invoices);
            return;
        };

        let mut changed = false;
        let mut action = EditorAction::None;
        egui::Panel::left("editor-form")
            .resizable(true)
            .default_size(470.0)
            .size_range(400.0..=700.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let title = if ed.is_new { "New invoice".to_string() } else { format!("Invoice {}", ed.invoice.number) };
                        ui.heading(title);
                    });
                    ui.add_space(8.0);
                    changed |= invoice_form(ui, &mut ed.invoice, &self.data.clients);
                    ui.add_space(10.0);
                    changed |= line_items_form(ui, &mut ed.invoice.line_items, &self.data);
                    ui.add_space(10.0);
                    ui.label(RichText::new("Changes save automatically.").weak().size(12.0));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui.button("Back to list").clicked() {
                            action = EditorAction::Back;
                        }
                        if ui.button(RichText::new("Export PDF…").strong()).clicked() {
                            action = EditorAction::ExportPdf;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let delete = egui::Button::new(
                                RichText::new("Delete").color(egui::Color32::from_rgb(0xb3, 0x26, 0x1e)),
                            );
                            if ui.add(delete).clicked() {
                                action = EditorAction::Delete;
                            }
                        });
                    });
                });
            });
        if changed {
            ed.dirty.get_or_insert(now);
        }

        let invoice = ed.invoice.clone();
        egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(0xe9, 0xea, 0xed)).inner_margin(12))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    let pages = self.layout(&invoice);
                    preview::show_pages(ui, &pages, self.logo.texture.as_ref());
                });
            });

        match action {
            EditorAction::None => {}
            EditorAction::Back => self.set_view(View::Invoices),
            EditorAction::ExportPdf => {
                self.flush_all();
                self.export_pdf();
            }
            EditorAction::Delete => {
                let Some(ed) = &self.editor else { return };
                if ed.is_new && ed.dirty.is_none() {
                    // Never saved: just discard it.
                    self.editor = None;
                    self.set_view(View::Invoices);
                } else {
                    self.flush_all();
                    let ed = self.editor.as_ref().expect("still open");
                    self.confirm = Some(Confirm::DeleteInvoice {
                        id: ed.invoice.id.clone(),
                        number: ed.invoice.number.clone(),
                    });
                }
            }
        }
    }
}

enum EditorAction {
    None,
    Back,
    ExportPdf,
    Delete,
}

/// Header fields. Returns true if anything changed.
fn invoice_form(ui: &mut egui::Ui, inv: &mut Invoice, clients: &[invoice::model::Client]) -> bool {
    let mut changed = false;
    egui::Grid::new("invoice-fields").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Invoice number");
        changed |= ui.text_edit_singleline(&mut inv.number).changed();
        ui.end_row();

        ui.label("Reference");
        changed |= ui.text_edit_singleline(&mut inv.reference).changed();
        ui.end_row();

        ui.label("Date");
        changed |= date_field(ui, "invoice-date", &mut inv.date);
        ui.end_row();

        ui.label("Due date");
        changed |= date_field(ui, "invoice-due", &mut inv.due_date);
        ui.end_row();

        ui.label("Bill to");
        let selected = clients
            .iter()
            .find(|c| c.id == inv.client_id)
            .map(|c| c.name.as_str())
            .unwrap_or("— No client —");
        let before = inv.client_id.clone();
        egui::ComboBox::from_id_salt("bill-to")
            .selected_text(selected)
            .width(260.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut inv.client_id, String::new(), "— No client —");
                for c in clients {
                    ui.selectable_value(&mut inv.client_id, c.id.clone(), &c.name);
                }
            });
        changed |= inv.client_id != before;
        ui.end_row();
    });
    changed
}

/// A date stored as `YYYY-MM-DD`, edited with a calendar picker.
fn date_field(ui: &mut egui::Ui, id: &str, value: &mut String) -> bool {
    let mut date = value.parse::<jiff::civil::Date>().unwrap_or_else(|_| jiff::Zoned::now().date());
    let before = date;
    ui.add(DatePickerButton::new(&mut date).id_salt(id).format("%d/%m/%Y").calendar_week(false));
    if date != before {
        *value = date.to_string();
        return true;
    }
    false
}

/// The line item rows. Returns true if anything changed.
fn line_items_form(ui: &mut egui::Ui, lines: &mut Vec<LineItem>, data: &invoice::model::Data) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Line items").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("+ Add line").clicked() {
                lines.push(blank_line());
                changed = true;
            }
            let mut pick: Option<usize> = None;
            egui::ComboBox::from_id_salt("catalog-insert")
                .selected_text("Insert from catalog…")
                .width(170.0)
                .show_ui(ui, |ui| {
                    if data.items.is_empty() {
                        ui.label(RichText::new("No catalog items yet (Settings).").weak());
                    }
                    for (i, item) in data.items.iter().enumerate() {
                        if ui.selectable_label(false, format!("{} — {}", item.code, item.description)).clicked() {
                            pick = Some(i);
                        }
                    }
                });
            if let Some(item) = pick.and_then(|i| data.items.get(i)) {
                lines.push(LineItem {
                    code: item.code.clone(),
                    description: format!("{} - {}", item.code, item.description),
                    quantity: 1.0,
                    unit_price: item.unit_price,
                    ..Default::default()
                });
                changed = true;
            }
        });
    });
    ui.add_space(4.0);

    let mut remove = None;
    egui::Grid::new("line-items").num_columns(6).spacing([6.0, 6.0]).show(ui, |ui| {
        for title in ["Description", "Qty", "Unit price", "VAT %", "Total", ""] {
            ui.label(RichText::new(title).size(12.0).weak());
        }
        ui.end_row();
        for (i, li) in lines.iter_mut().enumerate() {
            changed |= ui
                .add_sized([170.0, 22.0], egui::TextEdit::singleline(&mut li.description).hint_text("Description"))
                .changed();
            changed |= ui.add(egui::DragValue::new(&mut li.quantity).range(0.0..=f64::MAX).speed(0.1)).changed();
            changed |= ui
                .add(egui::DragValue::new(&mut li.unit_price).range(0.0..=f64::MAX).speed(1.0).max_decimals(2))
                .changed();
            changed |= ui.add(egui::DragValue::new(&mut li.vat_pct).range(0.0..=100.0).speed(0.1)).changed();
            ui.label(data.settings.money(li.incl()));
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        lines.remove(i);
        if lines.is_empty() {
            lines.push(blank_line());
        }
        changed = true;
    }
    changed
}

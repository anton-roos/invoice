//! The invoice list.

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use invoice::model::display_date;

use super::{Confirm, InvoiceApp};

enum Action {
    Edit(String),
    Duplicate(String),
    Delete { id: String, number: String },
}

impl InvoiceApp {
    pub(super) fn invoices_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Invoices");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+ New invoice").clicked() {
                    self.new_invoice();
                }
            });
        });
        ui.add_space(8.0);

        if self.summaries.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No invoices yet — create your first one.").weak());
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Have data from the old version? Settings > Import data.json.").weak(),
                );
            });
            return;
        }

        let mut action = None;
        TableBuilder::new(ui)
            .id_salt("invoices-table")
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().at_least(110.0))
            .column(Column::remainder().at_least(180.0).clip(true))
            .column(Column::auto().at_least(90.0))
            .column(Column::auto().at_least(90.0))
            .column(Column::auto().at_least(110.0))
            .column(Column::auto().at_least(230.0))
            .header(26.0, |mut header| {
                for title in ["Number", "Client", "Date", "Due date", "Total", ""] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(32.0, self.summaries.len(), |mut row| {
                    let inv = &self.summaries[row.index()];
                    row.col(|ui| {
                        if ui.link(&inv.number).clicked() {
                            action = Some(Action::Edit(inv.id.clone()));
                        }
                    });
                    row.col(|ui| {
                        ui.label(&inv.client_name);
                    });
                    row.col(|ui| {
                        ui.label(display_date(&inv.date));
                    });
                    row.col(|ui| {
                        ui.label(display_date(&inv.due_date));
                    });
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(self.data.settings.money(inv.grand_total));
                        });
                    });
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Delete").clicked() {
                                action = Some(Action::Delete { id: inv.id.clone(), number: inv.number.clone() });
                            }
                            if ui.button("Duplicate").clicked() {
                                action = Some(Action::Duplicate(inv.id.clone()));
                            }
                            if ui.button("Edit").clicked() {
                                action = Some(Action::Edit(inv.id.clone()));
                            }
                        });
                    });
                });
            });

        match action {
            Some(Action::Edit(id)) => self.open_invoice(&id),
            Some(Action::Duplicate(id)) => self.duplicate_invoice(&id),
            Some(Action::Delete { id, number }) => self.confirm = Some(Confirm::DeleteInvoice { id, number }),
            None => {}
        }
    }
}

//! The client list and the add/edit client dialog.

use eframe::egui::{self, RichText};
use egui_extras::{Column, TableBuilder};

use invoice::db;
use invoice::model::Client;

use super::{ClientDraft, Confirm, InvoiceApp, lines_of};

impl InvoiceApp {
    pub(super) fn clients_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Clients");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+ New client").clicked() {
                    self.client_draft = Some(ClientDraft {
                        client: Client::default(),
                        is_new: true,
                        postal: String::new(),
                        physical: String::new(),
                    });
                }
            });
        });
        ui.add_space(8.0);

        if self.data.clients.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| ui.label(RichText::new("No clients yet.").weak()));
            return;
        }

        let mut edit = None;
        let mut delete = None;
        TableBuilder::new(ui)
            .id_salt("clients-table")
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto().at_least(220.0))
            .column(Column::auto().at_least(110.0))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::auto().at_least(140.0))
            .header(26.0, |mut header| {
                for title in ["Name", "VAT no.", "Physical address", ""] {
                    header.col(|ui| {
                        ui.label(RichText::new(title).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(32.0, self.data.clients.len(), |mut row| {
                    let c = &self.data.clients[row.index()];
                    row.col(|ui| {
                        ui.label(&c.name);
                    });
                    row.col(|ui| {
                        ui.label(&c.vat_no);
                    });
                    row.col(|ui| {
                        ui.label(c.physical_address.join(", "));
                    });
                    row.col(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Delete").clicked() {
                                delete = Some((c.id.clone(), c.name.clone()));
                            }
                            if ui.button("Edit").clicked() {
                                edit = Some(c.clone());
                            }
                        });
                    });
                });
            });

        if let Some(client) = edit {
            self.client_draft = Some(ClientDraft {
                postal: client.postal_address.join("\n"),
                physical: client.physical_address.join("\n"),
                client,
                is_new: false,
            });
        }
        if let Some((id, name)) = delete {
            self.confirm = Some(Confirm::DeleteClient { id, name });
        }
    }

    pub(super) fn client_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.client_draft else { return };
        let mut save = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("client-dialog")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading(if draft.is_new { "New client" } else { "Edit client" });
            ui.add_space(8.0);
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut draft.client.name).desired_width(f32::INFINITY));
            ui.label("VAT no.");
            ui.add(egui::TextEdit::singleline(&mut draft.client.vat_no).desired_width(f32::INFINITY));
            ui.label("Postal address (one line each)");
            ui.add(egui::TextEdit::multiline(&mut draft.postal).desired_rows(4).desired_width(f32::INFINITY));
            ui.label("Physical address (one line each)");
            ui.add(egui::TextEdit::multiline(&mut draft.physical).desired_rows(4).desired_width(f32::INFINITY));
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui.button("Save").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel || (modal.should_close() && !save) {
            self.client_draft = None;
            return;
        }
        if !save {
            return;
        }

        let mut client = draft.client.clone();
        client.name = client.name.trim().to_string();
        if client.name.is_empty() {
            client.name = "Untitled client".into();
        }
        client.vat_no = client.vat_no.trim().to_string();
        client.postal_address = lines_of(&draft.postal);
        client.physical_address = lines_of(&draft.physical);

        let result = if draft.is_new {
            client.id = db::new_id("c");
            db::insert_client(&self.conn, &client).map(|()| self.data.clients.push(client))
        } else {
            db::update_client(&self.conn, &client).map(|_| {
                if let Some(slot) = self.data.clients.iter_mut().find(|c| c.id == client.id) {
                    *slot = client;
                }
            })
        };
        match result {
            Ok(()) => {
                self.client_draft = None;
                self.notify("Client saved");
            }
            // Keep the dialog open so nothing typed is lost.
            Err(e) => self.report(format!("Could not save the client: {e}")),
        }
    }

    pub(super) fn delete_client(&mut self, id: &str) {
        match db::delete_client(&self.conn, id) {
            Ok(_) => {
                self.data.clients.retain(|c| c.id != id);
                // Invoices billed to this client now have none.
                if let Some(ed) = &mut self.editor
                    && ed.invoice.client_id == id
                {
                    ed.invoice.client_id.clear();
                }
                self.notify("Client deleted");
            }
            Err(e) => self.report(format!("Could not delete the client: {e}")),
        }
    }
}

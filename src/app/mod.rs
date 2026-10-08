//! The desktop UI. State lives here; each screen is in its own module.
//!
//! Edits are written to SQLite shortly after you stop typing (and straight
//! away when you switch screens or close the window), so there is no Save
//! button.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use rusqlite::Connection;

use invoice::fonts::{InvoiceFonts, Style};
use invoice::layout::{InvoiceDoc, Page, layout_invoice};
use invoice::logo::LogoImage;
use invoice::model::*;
use invoice::{db, logo, pdf};

mod clients;
mod editor;
mod invoices;
mod preview;
mod settings;
#[cfg(test)]
mod tests;

/// Seconds after the last keystroke before an edit is written.
const SAVE_DELAY: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Invoices,
    Editor,
    Clients,
    Settings,
}

/// The invoice open in the editor.
pub struct Editor {
    pub invoice: Invoice,
    /// Not in the database yet: it is created on the first edit.
    pub is_new: bool,
    /// Time of the first unsaved edit.
    pub dirty: Option<f64>,
}

/// The client being added or edited in the client dialog.
pub struct ClientDraft {
    pub client: Client,
    pub is_new: bool,
    pub postal: String,
    pub physical: String,
}

/// An action waiting for the user to confirm it.
pub enum Confirm {
    DeleteInvoice { id: String, number: String },
    DeleteClient { id: String, name: String },
    Import(PathBuf),
}

#[derive(Default)]
struct LogoCache {
    /// The `Company::logo` value this cache was built from.
    key: Option<String>,
    image: Option<Arc<LogoImage>>,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
}

pub struct InvoiceApp {
    conn: Connection,
    db_path: PathBuf,
    fonts: Arc<InvoiceFonts>,
    /// Company, settings, clients and catalog. Invoices are loaded on demand.
    data: Data,
    summaries: Vec<InvoiceSummary>,
    view: View,
    editor: Option<Editor>,
    company_dirty: Option<f64>,
    settings_dirty: Option<f64>,
    items_dirty: Option<f64>,
    /// Text being typed into the company address boxes.
    postal_buf: String,
    physical_buf: String,
    client_draft: Option<ClientDraft>,
    confirm: Option<Confirm>,
    toast: Option<(String, f64)>,
    save_error: Option<String>,
    logo: LogoCache,
}

impl InvoiceApp {
    pub fn new(cc: &eframe::CreationContext, conn: Connection, db_path: PathBuf) -> Self {
        let fonts = Arc::new(InvoiceFonts::load());
        let ctx = &cc.egui_ctx;
        // Register the invoice fonts so the preview matches the PDF.
        for style in Style::ALL {
            ctx.add_font(egui::epaint::text::FontInsert::new(
                style.family_name(),
                egui::FontData::from_owned(fonts.data(style).to_vec()),
                vec![egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Name(style.family_name().into()),
                    priority: egui::epaint::text::FontPriority::Highest,
                }],
            ));
        }
        ctx.set_theme(egui::Theme::Light);
        ctx.all_styles_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 6.0);
            s.spacing.button_padding = egui::vec2(10.0, 4.0);
        });

        let mut app = InvoiceApp {
            conn,
            db_path,
            fonts,
            data: Data::default(),
            summaries: Vec::new(),
            view: View::Invoices,
            editor: None,
            company_dirty: None,
            settings_dirty: None,
            items_dirty: None,
            postal_buf: String::new(),
            physical_buf: String::new(),
            client_draft: None,
            confirm: None,
            toast: None,
            save_error: None,
            logo: LogoCache::default(),
        };
        if let Err(e) = app.reload() {
            app.save_error = Some(format!("Could not read the database: {e}"));
        }
        app
    }

    /// Re-read everything from the database.
    fn reload(&mut self) -> rusqlite::Result<()> {
        let mut data = db::load_data(&self.conn)?;
        data.invoices.clear();
        self.data = data;
        self.summaries = db::invoice_summaries(&self.conn)?;
        self.postal_buf = self.data.company.postal_address.join("\n");
        self.physical_buf = self.data.company.physical_address.join("\n");
        Ok(())
    }

    fn refresh_summaries(&mut self) {
        match db::invoice_summaries(&self.conn) {
            Ok(s) => self.summaries = s,
            Err(e) => self.report(format!("Could not load invoices: {e}")),
        }
    }

    // ------------------------------------------------------------ saving

    fn now(ctx: &egui::Context) -> f64 {
        ctx.input(|i| i.time)
    }

    fn has_unsaved(&self) -> bool {
        self.company_dirty.is_some()
            || self.settings_dirty.is_some()
            || self.items_dirty.is_some()
            || self.editor.as_ref().is_some_and(|e| e.dirty.is_some())
    }

    /// Write edits that have settled for [`SAVE_DELAY`], or all of them if `force`.
    fn flush(&mut self, now: f64, force: bool) {
        let mut saved_any = false;
        let mut due = |t: &mut Option<f64>| {
            let ready = t.is_some_and(|t| force || now - t >= SAVE_DELAY);
            if ready {
                *t = None;
                saved_any = true;
            }
            ready
        };
        let mut errors = Vec::new();
        if due(&mut self.company_dirty)
            && let Err(e) = db::save_company(&self.conn, &self.data.company)
        {
            errors.push(format!("Could not save company details: {e}"));
        }
        if due(&mut self.settings_dirty)
            && let Err(e) = db::save_settings(&self.conn, &self.data.settings)
        {
            errors.push(format!("Could not save settings: {e}"));
        }
        if due(&mut self.items_dirty)
            && let Err(e) = db::replace_items(&mut self.conn, &self.data.items)
        {
            errors.push(format!("Could not save the catalog: {e}"));
        }
        if let Some(ed) = &mut self.editor
            && due(&mut ed.dirty)
        {
            if ed.is_new {
                match db::create_invoice(&mut self.conn, ed.invoice.clone()) {
                    Ok(saved) => {
                        ed.invoice.id = saved.id;
                        ed.invoice.number = saved.number;
                        ed.is_new = false;
                        // Creating an invoice advances the number counter.
                        match db::get_settings(&self.conn) {
                            Ok(s) => self.data.settings = s,
                            Err(e) => errors.push(format!("Could not read settings: {e}")),
                        }
                    }
                    Err(e) => errors.push(format!("Could not create the invoice: {e}")),
                }
            } else if let Err(e) = db::update_invoice(&mut self.conn, &ed.invoice) {
                errors.push(format!("Could not save the invoice: {e}"));
            }
        }
        if let Some(e) = errors.pop() {
            self.report(e);
        } else if saved_any {
            // A successful save clears the warning from an earlier failure.
            self.save_error = None;
        }
    }

    fn flush_all(&mut self) {
        self.flush(f64::MAX, true);
    }

    fn report(&mut self, message: String) {
        self.toast = Some((message.clone(), f64::NAN));
        self.save_error = Some(message);
    }

    fn notify(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), f64::NAN));
    }

    // ------------------------------------------------------------ navigation

    fn set_view(&mut self, view: View) {
        self.flush_all();
        match view {
            View::Invoices => self.refresh_summaries(),
            View::Settings => {
                self.postal_buf = self.data.company.postal_address.join("\n");
                self.physical_buf = self.data.company.physical_address.join("\n");
            }
            View::Editor | View::Clients => {}
        }
        self.view = view;
    }

    fn open_invoice(&mut self, id: &str) {
        self.flush_all();
        match db::get_invoice(&self.conn, id) {
            Ok(Some(invoice)) => {
                self.editor = Some(Editor { invoice, is_new: false, dirty: None });
                self.set_view(View::Editor);
            }
            Ok(None) => self.report("That invoice no longer exists.".into()),
            Err(e) => self.report(format!("Could not open the invoice: {e}")),
        }
    }

    fn new_invoice(&mut self) {
        self.flush_all();
        let today = today();
        let settings = &self.data.settings;
        let invoice = Invoice {
            number: settings.format_number(settings.next_invoice_number),
            date: today.clone(),
            due_date: today,
            client_id: self.data.clients.first().map(|c| c.id.clone()).unwrap_or_default(),
            line_items: vec![blank_line()],
            ..Default::default()
        };
        self.editor = Some(Editor { invoice, is_new: true, dirty: None });
        self.set_view(View::Editor);
    }

    fn duplicate_invoice(&mut self, id: &str) {
        self.flush_all();
        match db::duplicate_invoice(&mut self.conn, id, &today()) {
            Ok(Some(copy)) => {
                if let Ok(s) = db::get_settings(&self.conn) {
                    self.data.settings = s;
                }
                self.editor = Some(Editor { invoice: copy, is_new: false, dirty: None });
                self.set_view(View::Editor);
            }
            Ok(None) => self.report("That invoice no longer exists.".into()),
            Err(e) => self.report(format!("Could not duplicate the invoice: {e}")),
        }
    }

    fn delete_invoice(&mut self, id: &str) {
        if self.editor.as_ref().is_some_and(|e| e.invoice.id == id) {
            self.editor = None;
        }
        match db::delete_invoice(&self.conn, id) {
            Ok(_) => self.notify("Invoice deleted"),
            Err(e) => self.report(format!("Could not delete the invoice: {e}")),
        }
        self.set_view(View::Invoices);
    }

    // ------------------------------------------------------------ logo & invoice layout

    fn ensure_logo(&mut self, ctx: &egui::Context) {
        let key = &self.data.company.logo;
        if self.logo.key.as_ref() == Some(key) {
            return;
        }
        self.logo = LogoCache { key: Some(key.clone()), ..Default::default() };
        match logo::load(key) {
            Ok(Some(image)) => {
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.rgba,
                );
                self.logo.texture = Some(ctx.load_texture("company-logo", color, egui::TextureOptions::LINEAR));
                self.logo.image = Some(Arc::new(image));
            }
            Ok(None) => {}
            Err(e) => self.logo.error = Some(e),
        }
    }

    fn layout(&self, invoice: &Invoice) -> Vec<Page> {
        let doc = InvoiceDoc {
            company: &self.data.company,
            client: self.data.clients.iter().find(|c| c.id == invoice.client_id),
            settings: &self.data.settings,
            invoice,
            logo_size: self.logo.image.as_ref().map(|l| (l.width, l.height)),
        };
        layout_invoice(&doc, &self.fonts)
    }

    fn export_pdf(&mut self) {
        let Some(ed) = &self.editor else { return };
        let invoice = ed.invoice.clone();
        let file_name = format!("{}.pdf", safe_file_name(&invoice.number, "invoice"));
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export invoice as PDF")
            .set_file_name(file_name)
            .add_filter("PDF", &["pdf"])
            .save_file()
        else {
            return;
        };
        let pages = self.layout(&invoice);
        let result = pdf::render(&pages, &self.fonts, self.logo.image.as_deref())
            .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
        match result {
            Ok(()) => {
                self.notify(format!("Saved {}", path.display()));
                open_path(&path);
            }
            Err(e) => self.report(format!("Could not export the PDF: {e}")),
        }
    }

    // ------------------------------------------------------------ data files

    fn export_json(&mut self) {
        self.flush_all();
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export data")
            .set_file_name("data.json")
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };
        let result = db::load_data(&self.conn)
            .map_err(|e| e.to_string())
            .and_then(|d| serde_json::to_string_pretty(&d).map_err(|e| e.to_string()))
            .and_then(|json| std::fs::write(&path, json).map_err(|e| e.to_string()));
        match result {
            Ok(()) => self.notify(format!("Exported to {}", path.display())),
            Err(e) => self.report(format!("Could not export: {e}")),
        }
    }

    fn pick_import(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Import data.json")
            .add_filter("JSON", &["json"])
            .pick_file()
        {
            self.confirm = Some(Confirm::Import(path));
        }
    }

    fn import_json(&mut self, path: &Path) {
        self.flush_all();
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| Data::from_json(&text).map_err(|e| format!("not valid invoice data: {e}")))
            .and_then(|data| db::replace_all(&mut self.conn, &data).map_err(|e| e.to_string()))
            .and_then(|()| self.reload().map_err(|e| e.to_string()));
        match result {
            Ok(()) => {
                self.editor = None;
                self.notify(format!("Imported {}", path.display()));
                self.set_view(View::Invoices);
            }
            Err(e) => self.report(format!("Could not import {}: {e}", path.display())),
        }
    }

    fn backup_db(&mut self) {
        self.flush_all();
        let name = format!("invoices-backup-{}.db", today());
        let Some(path) = rfd::FileDialog::new()
            .set_title("Back up database")
            .set_file_name(name)
            .add_filter("SQLite database", &["db"])
            .save_file()
        else {
            return;
        };
        // VACUUM INTO refuses to overwrite; the save dialog already asked.
        let _ = std::fs::remove_file(&path);
        match db::backup_to(&self.conn, &path) {
            Ok(()) => self.notify(format!("Backed up to {}", path.display())),
            Err(e) => self.report(format!("Could not back up: {e}")),
        }
    }

    // ------------------------------------------------------------ chrome

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top-bar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Invoices").strong().size(17.0));
                ui.add_space(16.0);
                let has_editor = self.editor.is_some();
                let mut tab = |ui: &mut egui::Ui, view: View, label: &str, enabled: bool| {
                    let selected = self.view == view;
                    let response = ui.add_enabled(enabled, egui::Button::selectable(selected, label));
                    if response.clicked() && !selected {
                        self.set_view(view);
                    }
                };
                tab(ui, View::Invoices, "Invoices", true);
                tab(ui, View::Editor, "Editor", has_editor);
                tab(ui, View::Clients, "Clients", true);
                tab(ui, View::Settings, "Settings", true);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (text, color) = if let Some(err) = &self.save_error {
                        (format!("⚠ {err}"), Color32::from_rgb(0xb3, 0x26, 0x1e))
                    } else if self.has_unsaved() {
                        ("Saving…".to_string(), ui.visuals().weak_text_color())
                    } else {
                        ("All changes saved".to_string(), ui.visuals().weak_text_color())
                    };
                    ui.label(RichText::new(text).color(color).size(12.0))
                        .on_hover_text(format!("Database: {}", self.db_path.display()));
                });
            });
            ui.add_space(4.0);
        });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(confirm) = &self.confirm else { return };
        let (title, body, action) = match confirm {
            Confirm::DeleteInvoice { number, .. } => (
                "Delete invoice?",
                format!("Delete invoice {number}? This cannot be undone."),
                "Delete",
            ),
            Confirm::DeleteClient { name, .. } => (
                "Delete client?",
                format!("Delete client \"{name}\"? Their invoices are kept, without a client."),
                "Delete",
            ),
            Confirm::Import(path) => (
                "Replace all data?",
                format!(
                    "Importing {} replaces ALL current invoices, clients and settings.\n\n\
                     Tip: use \"Back up database…\" first if you might want them back.",
                    path.display()
                ),
                "Replace everything",
            ),
        };
        let mut decision = None;
        let modal = egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(title);
            ui.add_space(6.0);
            ui.label(body);
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let danger = egui::Button::new(RichText::new(action).color(Color32::WHITE))
                    .fill(Color32::from_rgb(0xb3, 0x26, 0x1e));
                if ui.add(danger).clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
            });
        });
        if modal.should_close() && decision.is_none() {
            decision = Some(false);
        }
        if let Some(go) = decision {
            let confirm = self.confirm.take().expect("checked above");
            if go {
                match confirm {
                    Confirm::DeleteInvoice { id, .. } => self.delete_invoice(&id),
                    Confirm::DeleteClient { id, .. } => self.delete_client(&id),
                    Confirm::Import(path) => self.import_json(&path),
                }
            }
        }
    }

    fn show_toast(&mut self, ctx: &egui::Context) {
        let now = Self::now(ctx);
        let Some((message, shown_at)) = &mut self.toast else { return };
        if shown_at.is_nan() {
            *shown_at = now;
        }
        if now - *shown_at > 3.5 {
            self.toast = None;
            return;
        }
        egui::Area::new(egui::Id::new("toast"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -20.0))
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x1a, 0x1a, 0x1a))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(14, 8))
                    .show(ui, |ui| ui.label(RichText::new(message.as_str()).color(Color32::WHITE)));
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

impl eframe::App for InvoiceApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.flush(Self::now(&ctx), false);
        if self.has_unsaved() {
            ctx.request_repaint_after(Duration::from_secs_f64(SAVE_DELAY));
        }
        self.ensure_logo(&ctx);

        self.top_bar(ui);
        egui::CentralPanel::default_margins().show(ui, |ui| match self.view {
            View::Invoices => self.invoices_view(ui),
            View::Editor => self.editor_view(ui),
            View::Clients => self.clients_view(ui),
            View::Settings => self.settings_view(ui),
        });
        self.client_dialog(&ctx);
        self.confirm_dialog(&ctx);
        self.show_toast(&ctx);
    }

    fn on_exit(&mut self) {
        self.flush_all();
    }
}

// ------------------------------------------------------------ helpers

pub fn today() -> String {
    jiff::Zoned::now().date().to_string()
}

pub fn blank_line() -> LineItem {
    LineItem { quantity: 1.0, ..Default::default() }
}

/// Lines from a multi-line text box, trimmed, without blanks.
pub fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// Keep a file name free of characters Windows doesn't allow.
fn safe_file_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() { fallback.to_string() } else { cleaned }
}

/// Open a file or folder with its default app.
pub fn open_path(path: &Path) {
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = result {
        eprintln!("could not open {}: {e}", path.display());
    }
}

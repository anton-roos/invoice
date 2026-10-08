//! Invoices: a native desktop app (egui) that stores its data in SQLite.

// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;

fn main() {
    let db_path = match invoice::paths::db_path() {
        Ok(path) => path,
        Err(e) => fatal(&format!("Could not create the data folder: {e}")),
    };
    let conn = match invoice::db::open(&db_path) {
        Ok(conn) => conn,
        Err(e) => fatal(&format!("Could not open the database at {}:\n\n{e}", db_path.display())),
    };

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("bundled icon is a valid PNG");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Invoices")
            .with_app_id("invoices")
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(icon),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Invoices",
        options,
        Box::new(move |cc| Ok(Box::new(app::InvoiceApp::new(cc, conn, db_path)))),
    );
    if let Err(e) = result {
        fatal(&format!("Could not start Invoices: {e}"));
    }
}

/// Show an error box (there may be no console to print to) and exit.
fn fatal(message: &str) -> ! {
    rfd::MessageDialog::new()
        .set_title("Invoices")
        .set_level(rfd::MessageLevel::Error)
        .set_description(message)
        .show();
    std::process::exit(1);
}

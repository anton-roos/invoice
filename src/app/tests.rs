//! Drive the real app offscreen (egui_kittest + wgpu): check that edits reach
//! SQLite, and save a screenshot of each screen to `target/ui-screens/`.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;

use invoice::db;
use invoice::model::Data;

use super::{InvoiceApp, View};

fn temp_db() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("invoices-ui-{}", db::new_id("")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("invoices.db");
    let mut conn = db::open(&path).unwrap();
    db::replace_all(&mut conn, &Data::from_json(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample-data.json"))).unwrap()).unwrap();
    path
}

fn harness(db_path: &std::path::Path) -> Harness<'static, InvoiceApp> {
    let conn = db::open(db_path).unwrap();
    let path = db_path.to_path_buf();
    Harness::builder()
        .with_size([1280.0, 840.0])
        .with_theme(egui::Theme::Light) // the app is light; kittest defaults to dark
        .wgpu()
        .build_eframe(move |cc| InvoiceApp::new(cc, conn, path))
}

fn screenshot(h: &mut Harness<InvoiceApp>, name: &str) {
    h.run_steps(3); // let tables finish their first-frame sizing pass
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-screens");
    std::fs::create_dir_all(&dir).unwrap();
    // Rendering needs a GPU adapter; skip the picture (not the test) without one.
    match h.render() {
        Ok(image) => image.save(dir.join(format!("{name}.png"))).unwrap(),
        Err(e) => eprintln!("no screenshot for {name}: {e}"),
    }
}

#[test]
fn screens_render_and_edits_are_saved() {
    let db_path = temp_db();
    let mut h = harness(&db_path);
    h.run();
    h.get_by_label("INV0000001");
    screenshot(&mut h, "1-invoices");

    // Open the invoice and edit it through the UI state.
    h.state_mut().open_invoice("inv1");
    h.run();
    screenshot(&mut h, "2-editor");
    {
        let app = h.state_mut();
        let ed = app.editor.as_mut().unwrap();
        ed.invoice.reference = "EDITED".into();
        ed.invoice.line_items[0].quantity = 10.0;
        ed.dirty = Some(0.0);
    }
    h.run();
    h.state_mut().set_view(View::Invoices); // switching screens saves
    h.run();
    h.get_by_label("R4,500.00");

    // A new invoice gets the next number and is created on first edit.
    h.state_mut().new_invoice();
    h.run();
    {
        let app = h.state_mut();
        let ed = app.editor.as_mut().unwrap();
        ed.invoice.line_items[0].description = "New work".into();
        ed.invoice.line_items[0].unit_price = 200.0;
        ed.invoice.line_items[0].vat_pct = 15.0;
        ed.dirty = Some(0.0);
    }
    h.state_mut().set_view(View::Invoices);
    h.run();
    h.get_by_label("INV0000002");
    h.get_by_label("R230.00");

    h.state_mut().set_view(View::Clients);
    h.run();
    screenshot(&mut h, "3-clients");

    h.state_mut().set_view(View::Settings);
    h.run();
    h.get_by_label("Back up database…");
    screenshot(&mut h, "4-settings");

    // Everything above is in the database file.
    drop(h);
    let conn = db::open(&db_path).unwrap();
    let inv = db::get_invoice(&conn, "inv1").unwrap().unwrap();
    assert_eq!(inv.reference, "EDITED");
    assert_eq!(inv.line_items[0].quantity, 10.0);
    assert_eq!(db::get_settings(&conn).unwrap().next_invoice_number, 3);
    assert_eq!(db::list_invoices(&conn).unwrap().len(), 2);
    drop(conn);
    let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
}

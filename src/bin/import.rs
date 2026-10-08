//! Load a `data.json` from the old browser version of the app (Settings →
//! Export data.json, or a connected data.json file) into the SQLite database.
//! The desktop app can do the same from Settings → Import data.json.
//!
//! Double-click it (no arguments) to pick the file in a dialog, or run:
//!
//!     invoices-import data.json [--db <file>] [--force]
//!
//! Before replacing a database that already has data, a backup copy is saved
//! next to it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

use invoice::db;
use invoice::model::Data;

const USAGE: &str = "usage: invoices-import [<data.json>] [--db <file>] [--force]

  <data.json>   JSON exported from the old app or from Invoices.
                Leave it out to choose the file in a dialog.
  --db <file>   SQLite database to write
                (default: $INVOICE_DB, or %APPDATA%\\Invoices\\invoices.db)
  --force       replace the database contents even if it already has data
                (a backup copy is saved first)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return interactive();
    }
    match command_line(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Double-clicked: ask for the file and report back in message boxes, since
/// the console window closes as soon as we exit.
fn interactive() -> ExitCode {
    let Some(json_path) = rfd::FileDialog::new()
        .set_title("Choose the data.json to import into Invoices")
        .add_filter("Invoice data (JSON)", &["json"])
        .pick_file()
    else {
        return ExitCode::SUCCESS;
    };
    let result = invoice::paths::db_path()
        .map_err(|e| format!("could not create the data folder: {e}"))
        .and_then(|db_path| {
            import(&json_path, &db_path, |existing| {
                let answer = MessageDialog::new()
                    .set_title("Replace existing data?")
                    .set_level(MessageLevel::Warning)
                    .set_description(format!(
                        "The Invoices database already contains {existing}.\n\n\
                         Replace ALL of it with the contents of {}?\n\n\
                         A backup copy of the current database is saved first. \
                         Close the Invoices app before continuing.",
                        json_path.display()
                    ))
                    .set_buttons(MessageButtons::YesNo)
                    .show();
                answer == MessageDialogResult::Yes
            })
        });
    let (level, text, code) = match result {
        Ok(Some(summary)) => (MessageLevel::Info, summary, ExitCode::SUCCESS),
        Ok(None) => return ExitCode::SUCCESS, // declined
        Err(e) => (MessageLevel::Error, format!("The import failed:\n\n{e}"), ExitCode::FAILURE),
    };
    MessageDialog::new().set_title("Invoices import").set_level(level).set_description(text).show();
    code
}

fn command_line(args: Vec<String>) -> Result<(), String> {
    let mut json_path = None;
    let mut db_path = None;
    let mut force = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db_path = Some(PathBuf::from(args.next().ok_or("--db needs a file name")?)),
            "--force" => force = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if json_path.is_none() && !arg.starts_with('-') => json_path = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument '{arg}'\n\n{USAGE}")),
        }
    }
    let json_path = json_path.ok_or(USAGE)?;
    let db_path = match db_path {
        Some(path) => path,
        None => invoice::paths::db_path().map_err(|e| format!("could not create the data folder: {e}"))?,
    };
    let summary = import(&json_path, &db_path, |existing| {
        if !force {
            eprintln!(
                "{} already contains {existing}. Re-run with --force to replace it with {} \
                 (a backup copy is saved first).",
                db_path.display(),
                json_path.display()
            );
        }
        force
    })?;
    if let Some(summary) = summary {
        println!("{summary}");
        Ok(())
    } else {
        Err("nothing imported".into())
    }
}

/// Import `json_path` into `db_path`. If the database already has data,
/// `confirm_replace` is asked (with a description of what's there); on yes a
/// backup is made first. Returns a summary, or `None` if the user declined.
fn import(
    json_path: &Path,
    db_path: &Path,
    confirm_replace: impl FnOnce(&str) -> bool,
) -> Result<Option<String>, String> {
    let text = std::fs::read_to_string(json_path)
        .map_err(|e| format!("could not read {}: {e}", json_path.display()))?;
    let data = Data::from_json(&text)
        .map_err(|e| format!("{} is not invoice data from the old app: {e}", json_path.display()))?;

    let mut conn = db::open(db_path).map_err(|e| format!("could not open {}: {e}", db_path.display()))?;
    let mut backup_note = String::new();
    if !db::is_empty(&conn).map_err(|e| e.to_string())? {
        let current = db::load_data(&conn).map_err(|e| e.to_string())?;
        let existing = format!(
            "{} invoice(s), {} client(s) and {} catalog item(s)",
            current.invoices.len(),
            current.clients.len(),
            current.items.len()
        );
        if !confirm_replace(&existing) {
            return Ok(None);
        }
        let stamp = jiff::Zoned::now().strftime("%Y-%m-%d-%H%M%S").to_string();
        let backup = db_path.with_file_name(format!("invoices-before-import-{stamp}.db"));
        db::backup_to(&conn, &backup).map_err(|e| format!("could not back up the database first: {e}"))?;
        backup_note = format!("\n\nThe previous data was backed up to:\n{}", backup.display());
    }
    db::replace_all(&mut conn, &data).map_err(|e| format!("could not write the database: {e}"))?;

    let settings = db::get_settings(&conn).map_err(|e| e.to_string())?;
    let line_count: usize = data.invoices.iter().map(|i| i.line_items.len()).sum();
    let mut summary = format!(
        "Imported {} into {}:\n{} client(s), {} catalog item(s), {} invoice(s) ({line_count} line items).\n",
        json_path.display(),
        db_path.display(),
        data.clients.len(),
        data.items.len(),
        data.invoices.len(),
    );
    for inv in db::list_invoices(&conn).map_err(|e| e.to_string())? {
        summary.push_str(&format!(
            "\n  {:<14} {:<10} {}",
            inv.number,
            inv.date,
            settings.money(inv.totals().grand_total)
        ));
    }
    summary.push_str(&backup_note);
    Ok(Some(summary))
}

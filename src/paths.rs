//! Where the app keeps its data.

use std::path::PathBuf;

/// Per-user data folder, e.g. `C:\Users\<you>\AppData\Roaming\Invoices`.
/// Uninstalling the app leaves it (and your invoices) in place.
pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Invoices")
}

/// The SQLite database: `INVOICE_DB` if set, otherwise `invoices.db` in
/// [`data_dir`]. The folder is created if needed.
pub fn db_path() -> std::io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("INVOICE_DB") {
        return Ok(PathBuf::from(path));
    }
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("invoices.db"))
}

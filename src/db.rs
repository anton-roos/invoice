//! SQLite storage. Addresses are stored one line per `\n`; company and
//! settings are single-row tables.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Result, Row, params};

use crate::model::*;

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS company (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    name                TEXT NOT NULL DEFAULT '',
    logo                TEXT NOT NULL DEFAULT '',
    vat_no              TEXT NOT NULL DEFAULT '',
    postal_address      TEXT NOT NULL DEFAULT '',
    physical_address    TEXT NOT NULL DEFAULT '',
    bank_name           TEXT NOT NULL DEFAULT '',
    bank_account_holder TEXT NOT NULL DEFAULT '',
    bank_account_number TEXT NOT NULL DEFAULT '',
    bank_account_type   TEXT NOT NULL DEFAULT '',
    bank_branch_code    TEXT NOT NULL DEFAULT ''
);
INSERT OR IGNORE INTO company (id) VALUES (1);

CREATE TABLE IF NOT EXISTS settings (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    currency_symbol     TEXT    NOT NULL DEFAULT 'R',
    number_prefix       TEXT    NOT NULL DEFAULT 'INV',
    number_padding      INTEGER NOT NULL DEFAULT 7,
    next_invoice_number INTEGER NOT NULL DEFAULT 1
);
INSERT OR IGNORE INTO settings (id) VALUES (1);

CREATE TABLE IF NOT EXISTS clients (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL DEFAULT '',
    vat_no           TEXT NOT NULL DEFAULT '',
    postal_address   TEXT NOT NULL DEFAULT '',
    physical_address TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS catalog_items (
    id          INTEGER PRIMARY KEY,
    position    INTEGER NOT NULL,
    code        TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    unit_price  REAL NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS invoices (
    id                   TEXT PRIMARY KEY,
    number               TEXT NOT NULL,
    reference            TEXT NOT NULL DEFAULT '',
    date                 TEXT NOT NULL DEFAULT '',
    due_date             TEXT NOT NULL DEFAULT '',
    client_id            TEXT REFERENCES clients(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS line_items (
    id          INTEGER PRIMARY KEY,
    invoice_id  TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    code        TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    quantity    REAL NOT NULL DEFAULT 0,
    unit_price  REAL NOT NULL DEFAULT 0,
    vat_pct     REAL NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS line_items_by_invoice ON line_items (invoice_id, position);
";

pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    // Wait rather than fail if another copy of the app is mid-write.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Write a consistent copy of the whole database to `dest` (which must not
/// exist yet), even while the app has it open.
pub fn backup_to(conn: &Connection, dest: &Path) -> Result<()> {
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])?;
    Ok(())
}

pub fn new_id(prefix: &str) -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}{}", &hex[..12])
}

fn join_lines(lines: &[String]) -> String {
    lines.join("\n")
}

fn split_lines(text: String) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

// ---------------------------------------------------------------- whole db

pub fn load_data(conn: &Connection) -> Result<Data> {
    Ok(Data {
        company: get_company(conn)?,
        settings: get_settings(conn)?,
        clients: list_clients(conn)?,
        items: list_items(conn)?,
        invoices: list_invoices(conn)?,
    })
}

/// True when there are no clients, catalog items or invoices yet.
pub fn is_empty(conn: &Connection) -> Result<bool> {
    conn.query_row(
        "SELECT NOT EXISTS (SELECT 1 FROM clients)
            AND NOT EXISTS (SELECT 1 FROM catalog_items)
            AND NOT EXISTS (SELECT 1 FROM invoices)",
        [],
        |r| r.get(0),
    )
}

/// Replace everything in the database with `data`, in one transaction.
pub fn replace_all(conn: &mut Connection, data: &Data) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch("DELETE FROM line_items; DELETE FROM invoices; DELETE FROM clients;")?;
    save_company(&tx, &data.company)?;
    save_settings(&tx, &data.settings)?;
    for client in &data.clients {
        insert_client(&tx, client)?;
    }
    write_items(&tx, &data.items)?;
    for invoice in &data.invoices {
        insert_invoice(&tx, invoice)?;
    }
    tx.commit()
}

// ---------------------------------------------------------------- company

pub fn get_company(conn: &Connection) -> Result<Company> {
    conn.query_row(
        "SELECT name, logo, vat_no, postal_address, physical_address, bank_name,
                bank_account_holder, bank_account_number, bank_account_type, bank_branch_code
         FROM company WHERE id = 1",
        [],
        |r| {
            Ok(Company {
                name: r.get(0)?,
                logo: r.get(1)?,
                vat_no: r.get(2)?,
                postal_address: split_lines(r.get(3)?),
                physical_address: split_lines(r.get(4)?),
                bank: Bank {
                    name: r.get(5)?,
                    account_holder: r.get(6)?,
                    account_number: r.get(7)?,
                    account_type: r.get(8)?,
                    branch_code: r.get(9)?,
                },
            })
        },
    )
}

pub fn save_company(conn: &Connection, c: &Company) -> Result<()> {
    conn.execute(
        "UPDATE company SET name = ?1, logo = ?2, vat_no = ?3, postal_address = ?4,
                physical_address = ?5, bank_name = ?6, bank_account_holder = ?7,
                bank_account_number = ?8, bank_account_type = ?9, bank_branch_code = ?10
         WHERE id = 1",
        params![
            c.name,
            c.logo,
            c.vat_no,
            join_lines(&c.postal_address),
            join_lines(&c.physical_address),
            c.bank.name,
            c.bank.account_holder,
            c.bank.account_number,
            c.bank.account_type,
            c.bank.branch_code,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- settings

pub fn get_settings(conn: &Connection) -> Result<Settings> {
    conn.query_row(
        "SELECT currency_symbol, number_prefix, number_padding, next_invoice_number
         FROM settings WHERE id = 1",
        [],
        |r| {
            Ok(Settings {
                currency_symbol: r.get(0)?,
                number_prefix: r.get(1)?,
                number_padding: r.get(2)?,
                next_invoice_number: r.get(3)?,
            })
        },
    )
}

pub fn save_settings(conn: &Connection, s: &Settings) -> Result<()> {
    conn.execute(
        "UPDATE settings SET currency_symbol = ?1, number_prefix = ?2, number_padding = ?3,
                next_invoice_number = ?4
         WHERE id = 1",
        params![s.currency_symbol, s.number_prefix, s.number_padding, s.next_invoice_number],
    )?;
    Ok(())
}

/// Format the next invoice number and advance the counter.
fn take_next_number(conn: &Connection) -> Result<String> {
    let settings = get_settings(conn)?;
    conn.execute("UPDATE settings SET next_invoice_number = next_invoice_number + 1 WHERE id = 1", [])?;
    Ok(settings.format_number(settings.next_invoice_number))
}

// ---------------------------------------------------------------- clients

pub fn list_clients(conn: &Connection) -> Result<Vec<Client>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, vat_no, postal_address, physical_address FROM clients ORDER BY rowid",
    )?;
    stmt.query_map([], |r| {
        Ok(Client {
            id: r.get(0)?,
            name: r.get(1)?,
            vat_no: r.get(2)?,
            postal_address: split_lines(r.get(3)?),
            physical_address: split_lines(r.get(4)?),
        })
    })?
    .collect()
}

pub fn insert_client(conn: &Connection, c: &Client) -> Result<()> {
    conn.execute(
        "INSERT INTO clients (id, name, vat_no, postal_address, physical_address)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![c.id, c.name, c.vat_no, join_lines(&c.postal_address), join_lines(&c.physical_address)],
    )?;
    Ok(())
}

/// Returns false if no client has that id.
pub fn update_client(conn: &Connection, c: &Client) -> Result<bool> {
    let n = conn.execute(
        "UPDATE clients SET name = ?2, vat_no = ?3, postal_address = ?4, physical_address = ?5
         WHERE id = ?1",
        params![c.id, c.name, c.vat_no, join_lines(&c.postal_address), join_lines(&c.physical_address)],
    )?;
    Ok(n > 0)
}

/// Invoices billed to the client keep existing, with no client.
pub fn delete_client(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM clients WHERE id = ?1", [id])? > 0)
}

// ---------------------------------------------------------------- catalog

pub fn list_items(conn: &Connection) -> Result<Vec<CatalogItem>> {
    let mut stmt =
        conn.prepare("SELECT code, description, unit_price FROM catalog_items ORDER BY position")?;
    stmt.query_map([], |r| {
        Ok(CatalogItem { code: r.get(0)?, description: r.get(1)?, unit_price: r.get(2)? })
    })?
    .collect()
}

pub fn replace_items(conn: &mut Connection, items: &[CatalogItem]) -> Result<()> {
    let tx = conn.transaction()?;
    write_items(&tx, items)?;
    tx.commit()
}

fn write_items(conn: &Connection, items: &[CatalogItem]) -> Result<()> {
    conn.execute("DELETE FROM catalog_items", [])?;
    let mut stmt = conn.prepare(
        "INSERT INTO catalog_items (position, code, description, unit_price) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (pos, item) in items.iter().enumerate() {
        stmt.execute(params![pos as i64, item.code, item.description, item.unit_price])?;
    }
    Ok(())
}

// ---------------------------------------------------------------- invoices

const INVOICE_COLUMNS: &str = "id, number, reference, date, due_date, COALESCE(client_id, '')";

fn invoice_from_row(r: &Row) -> Result<Invoice> {
    Ok(Invoice {
        id: r.get(0)?,
        number: r.get(1)?,
        reference: r.get(2)?,
        date: r.get(3)?,
        due_date: r.get(4)?,
        client_id: r.get(5)?,
        line_items: Vec::new(),
    })
}

const LINE_ITEM_COLUMNS: &str = "code, description, quantity, unit_price, vat_pct";

fn line_item_from_row(r: &Row, offset: usize) -> Result<LineItem> {
    Ok(LineItem {
        code: r.get(offset)?,
        description: r.get(offset + 1)?,
        quantity: r.get(offset + 2)?,
        unit_price: r.get(offset + 3)?,
        vat_pct: r.get(offset + 4)?,
    })
}

/// All invoices with their line items, newest first.
pub fn list_invoices(conn: &Connection) -> Result<Vec<Invoice>> {
    let mut lines: HashMap<String, Vec<LineItem>> = HashMap::new();
    let mut stmt = conn.prepare(&format!(
        "SELECT invoice_id, {LINE_ITEM_COLUMNS} FROM line_items ORDER BY invoice_id, position"
    ))?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        lines.entry(r.get(0)?).or_default().push(line_item_from_row(r, 1)?);
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT {INVOICE_COLUMNS} FROM invoices ORDER BY date DESC, number DESC"
    ))?;
    stmt.query_map([], invoice_from_row)?
        .map(|inv| {
            let mut inv = inv?;
            inv.line_items = lines.remove(&inv.id).unwrap_or_default();
            Ok(inv)
        })
        .collect()
}

pub fn invoice_summaries(conn: &Connection) -> Result<Vec<InvoiceSummary>> {
    let clients: HashMap<String, String> =
        list_clients(conn)?.into_iter().map(|c| (c.id, c.name)).collect();
    Ok(list_invoices(conn)?
        .into_iter()
        .map(|inv| InvoiceSummary {
            grand_total: inv.totals().grand_total,
            client_name: clients.get(&inv.client_id).cloned().unwrap_or_else(|| "—".into()),
            id: inv.id,
            number: inv.number,
            date: inv.date,
            due_date: inv.due_date,
        })
        .collect())
}

pub fn get_invoice(conn: &Connection, id: &str) -> Result<Option<Invoice>> {
    let Some(mut inv) = conn
        .query_row(&format!("SELECT {INVOICE_COLUMNS} FROM invoices WHERE id = ?1"), [id], invoice_from_row)
        .optional()?
    else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT {LINE_ITEM_COLUMNS} FROM line_items WHERE invoice_id = ?1 ORDER BY position"
    ))?;
    inv.line_items = stmt.query_map([id], |r| line_item_from_row(r, 0))?.collect::<Result<_>>()?;
    Ok(Some(inv))
}

/// `None` for an empty or unknown client id, so the foreign key holds.
fn client_ref(conn: &Connection, client_id: &str) -> Result<Option<String>> {
    if client_id.is_empty() {
        return Ok(None);
    }
    conn.query_row("SELECT id FROM clients WHERE id = ?1", [client_id], |r| r.get(0)).optional()
}

fn insert_invoice(conn: &Connection, inv: &Invoice) -> Result<()> {
    conn.execute(
        "INSERT INTO invoices (id, number, reference, date, due_date, client_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            inv.id,
            inv.number,
            inv.reference,
            inv.date,
            inv.due_date,
            client_ref(conn, &inv.client_id)?,
        ],
    )?;
    write_line_items(conn, &inv.id, &inv.line_items)
}

fn write_line_items(conn: &Connection, invoice_id: &str, items: &[LineItem]) -> Result<()> {
    conn.execute("DELETE FROM line_items WHERE invoice_id = ?1", [invoice_id])?;
    let mut stmt = conn.prepare(
        "INSERT INTO line_items (invoice_id, position, code, description, quantity, unit_price,
                                 vat_pct)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for (pos, li) in items.iter().enumerate() {
        stmt.execute(params![
            invoice_id,
            pos as i64,
            li.code,
            li.description,
            li.quantity,
            li.unit_price,
            li.vat_pct,
        ])?;
    }
    Ok(())
}

/// Store a new invoice under a fresh id, giving it the next number if it has
/// none. The number counter advances either way, as in the original app.
pub fn create_invoice(conn: &mut Connection, mut inv: Invoice) -> Result<Invoice> {
    let tx = conn.transaction()?;
    let next = take_next_number(&tx)?;
    if inv.number.trim().is_empty() {
        inv.number = next;
    }
    inv.id = new_id("inv");
    insert_invoice(&tx, &inv)?;
    tx.commit()?;
    Ok(inv)
}

/// Returns false if no invoice has that id.
pub fn update_invoice(conn: &mut Connection, inv: &Invoice) -> Result<bool> {
    let tx = conn.transaction()?;
    let n = tx.execute(
        "UPDATE invoices SET number = ?2, reference = ?3, date = ?4, due_date = ?5,
                client_id = ?6
         WHERE id = ?1",
        params![
            inv.id,
            inv.number,
            inv.reference,
            inv.date,
            inv.due_date,
            client_ref(&tx, &inv.client_id)?,
        ],
    )?;
    if n == 0 {
        return Ok(false);
    }
    write_line_items(&tx, &inv.id, &inv.line_items)?;
    tx.commit()?;
    Ok(true)
}

pub fn delete_invoice(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM invoices WHERE id = ?1", [id])? > 0)
}

/// Copy an invoice under the next number, dated `today`.
pub fn duplicate_invoice(conn: &mut Connection, id: &str, today: &str) -> Result<Option<Invoice>> {
    let Some(mut inv) = get_invoice(conn, id)? else {
        return Ok(None);
    };
    inv.number.clear();
    inv.date = today.to_string();
    create_invoice(conn, inv).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Data {
        Data::from_json(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample-data.json"))).unwrap()
    }

    #[test]
    fn seed_round_trips() {
        let mut conn = open(":memory:").unwrap();
        assert!(is_empty(&conn).unwrap());
        replace_all(&mut conn, &seed()).unwrap();

        let data = load_data(&conn).unwrap();
        assert_eq!(data.company.name, "Example Co");
        assert_eq!(data.company.postal_address.len(), 5);
        assert_eq!(data.clients[0].name, "Sample Client Ltd");
        assert_eq!(data.items.len(), 2);
        assert_eq!(data.invoices[0].line_items.len(), 2);
        assert_eq!(data.invoices[0].client_id, "c1");
        assert_eq!(invoice_summaries(&conn).unwrap()[0].grand_total, 3300.0);
    }

    #[test]
    fn create_duplicate_and_delete() {
        let mut conn = open(":memory:").unwrap();
        replace_all(&mut conn, &seed()).unwrap();

        let copy = duplicate_invoice(&mut conn, "inv1", "2026-02-01").unwrap().unwrap();
        assert_eq!(copy.number, "INV0000002");
        assert_eq!(copy.line_items.len(), 2);
        assert_eq!(get_settings(&conn).unwrap().next_invoice_number, 3);

        // Deleting a client keeps its invoices.
        assert!(delete_client(&conn, "c1").unwrap());
        assert_eq!(get_invoice(&conn, "inv1").unwrap().unwrap().client_id, "");

        assert!(delete_invoice(&conn, &copy.id).unwrap());
        assert!(get_invoice(&conn, &copy.id).unwrap().is_none());
    }

    #[test]
    fn backup_is_a_full_copy() {
        let dir = std::env::temp_dir().join(format!("invoice-test-{}", new_id("")));
        std::fs::create_dir_all(&dir).unwrap();
        let mut conn = open(dir.join("live.db")).unwrap();
        replace_all(&mut conn, &seed()).unwrap();
        backup_to(&conn, &dir.join("copy.db")).unwrap();
        let copy = open(dir.join("copy.db")).unwrap();
        assert_eq!(load_data(&copy).unwrap().invoices.len(), 1);
        drop((conn, copy));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unknown_client_is_dropped() {
        let mut conn = open(":memory:").unwrap();
        let mut data = seed();
        data.invoices[0].client_id = "gone".into();
        replace_all(&mut conn, &data).unwrap();
        assert_eq!(get_invoice(&conn, "inv1").unwrap().unwrap().client_id, "");
    }
}

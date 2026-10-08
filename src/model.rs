//! Data types. The JSON shape (camelCase) matches the original browser
//! app's `data.json`, so exports from it can be imported as-is.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Data {
    pub company: Company,
    pub settings: Settings,
    pub clients: Vec<Client>,
    pub items: Vec<CatalogItem>,
    pub invoices: Vec<Invoice>,
}

impl Data {
    /// Parse a `data.json` produced by the old app. It is forgiving: missing
    /// fields and `null`s (e.g. `NaN` that JSON.stringify turned into null)
    /// fall back to defaults.
    pub fn from_value(mut value: Value) -> serde_json::Result<Data> {
        strip_nulls(&mut value);
        serde_json::from_value(value)
    }

    pub fn from_json(text: &str) -> serde_json::Result<Data> {
        Self::from_value(serde_json::from_str(text)?)
    }
}

fn strip_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(strip_nulls);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Company {
    pub name: String,
    /// A `data:image/...` URL of the uploaded logo (or a path to an image file).
    pub logo: String,
    pub vat_no: String,
    pub postal_address: Vec<String>,
    pub physical_address: Vec<String>,
    pub bank: Bank,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Bank {
    pub name: String,
    pub account_holder: String,
    pub account_number: String,
    pub account_type: String,
    pub branch_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub currency_symbol: String,
    pub number_prefix: String,
    pub number_padding: i64,
    pub next_invoice_number: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            currency_symbol: "R".into(),
            number_prefix: "INV".into(),
            number_padding: 7,
            next_invoice_number: 1,
        }
    }
}

impl Settings {
    /// `R3,300.00` style amount with thousands separators.
    pub fn money(&self, amount: f64) -> String {
        let cents = (amount.abs() * 100.0).round() as u64;
        let whole = (cents / 100).to_string();
        let mut grouped = String::new();
        for (i, ch) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i).is_multiple_of(3) {
                grouped.push(',');
            }
            grouped.push(ch);
        }
        let sign = if amount < 0.0 && cents > 0 { "-" } else { "" };
        format!("{sign}{}{grouped}.{:02}", self.currency_symbol, cents % 100)
    }

    /// `INV` + zero-padded number, e.g. `INV0000002`.
    pub fn format_number(&self, n: i64) -> String {
        let width = self.number_padding.clamp(0, 20) as usize;
        format!("{}{:0width$}", self.number_prefix, n)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub vat_no: String,
    pub postal_address: Vec<String>,
    pub physical_address: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CatalogItem {
    pub code: String,
    pub description: String,
    pub unit_price: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Invoice {
    pub id: String,
    pub number: String,
    pub reference: String,
    pub date: String,
    pub due_date: String,
    /// Empty when the invoice has no (or a deleted) client.
    pub client_id: String,
    pub line_items: Vec<LineItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LineItem {
    pub code: String,
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub vat_pct: f64,
}

impl LineItem {
    /// `qty × price`
    pub fn excl(&self) -> f64 {
        self.quantity * self.unit_price
    }

    /// `excl × (1 + VAT%)`
    pub fn incl(&self) -> f64 {
        self.excl() * (1.0 + self.vat_pct / 100.0)
    }
}

/// `2026-01-19` → `19/01/2026`; anything else is shown as-is.
pub fn display_date(iso: &str) -> String {
    let parts: Vec<&str> = iso.split('-').collect();
    match parts.as_slice() {
        [y, m, d] if !y.is_empty() && !m.is_empty() && !d.is_empty() => format!("{d}/{m}/{y}"),
        _ => iso.to_string(),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub total_exclusive: f64,
    pub total_vat: f64,
    pub sub_total: f64,
    pub grand_total: f64,
}

impl Invoice {
    /// Per line: `excl = qty × price`, `vat = excl × vat%`.
    pub fn totals(&self) -> Totals {
        let mut total_exclusive = 0.0;
        let mut vat_total = 0.0;
        for li in &self.line_items {
            let excl = li.excl();
            total_exclusive += excl;
            vat_total += excl * li.vat_pct / 100.0;
        }
        let sub_total = total_exclusive + vat_total;
        Totals {
            total_exclusive,
            total_vat: vat_total,
            sub_total,
            grand_total: sub_total,
        }
    }
}

/// A row in the invoice list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceSummary {
    pub id: String,
    pub number: String,
    pub client_name: String,
    pub date: String,
    pub due_date: String,
    pub grand_total: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(quantity: f64, unit_price: f64, vat_pct: f64) -> LineItem {
        LineItem { quantity, unit_price, vat_pct, ..Default::default() }
    }

    #[test]
    fn totals_match_original_invoice() {
        let inv = Invoice {
            line_items: vec![line(6.0, 300.0, 0.0), line(5.0, 300.0, 0.0)],
            ..Default::default()
        };
        assert_eq!(inv.totals().grand_total, 3300.0);
    }

    #[test]
    fn totals_with_vat() {
        let inv = Invoice {
            line_items: vec![line(2.0, 100.0, 15.0), line(1.0, 50.0, 0.0)],
            ..Default::default()
        };
        let t = inv.totals();
        assert_eq!(t.total_exclusive, 250.0);
        assert_eq!(t.total_vat, 30.0); // VAT is per line
        assert_eq!(t.grand_total, 280.0);
    }

    #[test]
    fn number_formatting() {
        let s = Settings::default();
        assert_eq!(s.format_number(2), "INV0000002");
        assert_eq!(s.money(3300.0), "R3,300.00");
        assert_eq!(s.money(1234567.891), "R1,234,567.89");
        assert_eq!(s.money(-5.5), "-R5.50");
        assert_eq!(s.money(0.0), "R0.00");
        assert_eq!(display_date("2026-01-19"), "19/01/2026");
    }

    #[test]
    fn parses_nulls_and_missing_fields() {
        let data = Data::from_json(
            r#"{"invoices":[{"id":"a","reference":null,"lineItems":[{"quantity":2}]}]}"#,
        )
        .unwrap();
        assert_eq!(data.invoices[0].reference, "");
        assert_eq!(data.invoices[0].line_items[0].quantity, 2.0);
        assert_eq!(data.settings.number_prefix, "INV");
    }
}

//! The printed invoice, laid out once as a list of drawing commands (in PDF
//! points, origin top-left) that both the on-screen preview and the PDF
//! export draw. It reproduces the original invoice design: A4, Arial, black
//! ink, grey labels and thin rules.

use crate::fonts::{InvoiceFonts, Style};
use crate::model::*;

pub const PAGE_W: f32 = 595.28; // A4
pub const PAGE_H: f32 = 841.89;
const MARGIN_X: f32 = 28.35; // 10 mm
const MARGIN_Y: f32 = 42.52; // 15 mm
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN_X;
const BOTTOM: f32 = PAGE_H - MARGIN_Y;

/// Body text size (8.5pt) and its line height.
const BODY: f32 = 8.5;
const LINE: f32 = BODY * 1.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

const BLACK: Rgb = Rgb(0, 0, 0);
const INK: Rgb = Rgb(0x33, 0x33, 0x33);
const LABEL: Rgb = Rgb(0x66, 0x66, 0x66);
const MUTED: Rgb = Rgb(0x96, 0x96, 0x96);
const HEADER: Rgb = Rgb(0x9c, 0x9c, 0x9c);
const RULE: Rgb = Rgb(0xde, 0xde, 0xde);
const RULE_LIGHT: Rgb = Rgb(0xee, 0xee, 0xee);
const HEADER_BG: Rgb = Rgb(0xfa, 0xfa, 0xfa);

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `y` is the top of the line; renderers add the font's ascent.
    Text { x: f32, y: f32, size: f32, style: Style, color: Rgb, text: String },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgb },
    Logo { x: f32, y: f32, w: f32, h: f32 },
}

pub type Page = Vec<Item>;

pub struct InvoiceDoc<'a> {
    pub company: &'a Company,
    pub client: Option<&'a Client>,
    pub settings: &'a Settings,
    pub invoice: &'a Invoice,
    /// Pixel size of the decoded logo, if there is one.
    pub logo_size: Option<(u32, u32)>,
}

struct Builder<'f> {
    fonts: &'f InvoiceFonts,
    pages: Vec<Page>,
    y: f32,
}

impl Builder<'_> {
    fn push(&mut self, item: Item) {
        self.pages.last_mut().expect("at least one page").push(item);
    }

    fn text(&mut self, x: f32, y: f32, size: f32, style: Style, color: Rgb, text: &str) {
        if !text.is_empty() {
            let text = text.to_string();
            self.push(Item::Text { x, y, size, style, color, text });
        }
    }

    fn text_right(&mut self, right: f32, y: f32, size: f32, style: Style, color: Rgb, text: &str) {
        let w = self.fonts.width(text, style, size);
        self.text(right - w, y, size, style, color, text);
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgb) {
        self.push(Item::Rect { x, y, w, h, color });
    }

    fn new_page(&mut self) {
        self.pages.push(Vec::new());
        self.y = MARGIN_Y;
    }

    /// Start a new page unless `height` more points fit on this one.
    fn ensure_room(&mut self, height: f32) -> bool {
        if self.y + height > BOTTOM {
            self.new_page();
            true
        } else {
            false
        }
    }

    /// Greedy word wrap; words longer than the line are left to overflow.
    fn wrap(&self, text: &str, style: Style, size: f32, max_w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && self.fonts.width(&candidate, style, size) > max_w {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() || lines.is_empty() {
            lines.push(line);
        }
        lines
    }
}

/// Lay out the invoice on as many A4 pages as it needs.
pub fn layout_invoice(doc: &InvoiceDoc, fonts: &InvoiceFonts) -> Vec<Page> {
    let mut b = Builder { fonts, pages: vec![Vec::new()], y: MARGIN_Y };
    let right = MARGIN_X + CONTENT_W;
    let inv = doc.invoice;
    let s = doc.settings;
    let totals = inv.totals();

    // ---- Logo and title
    let top = b.y;
    let logo_h = match doc.logo_size {
        Some((pw, ph)) if pw > 0 && ph > 0 => {
            let scale = (165.0 / pw as f32).min(52.5 / ph as f32);
            let (w, h) = (pw as f32 * scale, ph as f32 * scale);
            b.push(Item::Logo { x: MARGIN_X, y: top, w, h });
            h
        }
        _ => {
            b.text(MARGIN_X, top, 25.5, Style::Regular, BLACK, &doc.company.name);
            25.5 * 1.15
        }
    };
    b.text_right(right, top, 15.75, Style::Bold, BLACK, "INVOICE");
    b.y = top + logo_h.max(15.75 * 1.15) + 7.5;

    // ---- Number, dates, etc. (right-aligned block)
    let date = display_date(&inv.date);
    let due = display_date(&inv.due_date);
    let meta: [(&str, &str, bool); 5] = [
        ("NUMBER:", &inv.number, false),
        ("REFERENCE:", &inv.reference, false),
        ("DATE:", &date, true),
        ("DUE DATE:", &due, false),
        ("PAGE:", "1/1", false), // patched once the page count is known
    ];
    let value_style = |bold| if bold { Style::Bold } else { Style::Regular };
    let label_w = meta.iter().map(|(l, _, _)| fonts.width(l, Style::Regular, BODY)).fold(0.0, f32::max);
    let value_w = meta.iter().map(|(_, v, bold)| fonts.width(v, value_style(*bold), BODY)).fold(0.0, f32::max);
    let label_x = right - value_w - 13.5 - label_w;
    let mut page_item = 0;
    for (label, value, bold) in meta {
        b.text(label_x, b.y, BODY, Style::Regular, LABEL, label);
        if label == "PAGE:" {
            page_item = b.pages[0].len();
        }
        let color = if bold { BLACK } else { INK };
        b.text_right(right, b.y, BODY, value_style(bold), color, value);
        b.y += LINE + 2.25;
    }
    b.y += 19.5;

    // ---- From / To
    let empty_client = Client::default();
    let client = doc.client.unwrap_or(&empty_client);
    let col_w = (CONTENT_W - 18.0) / 2.0;
    let parties_top = b.y;
    let mut parties_bottom = parties_top;
    let from = ("FROM", &doc.company.name, "VAT NO: ", &doc.company.vat_no,
        &doc.company.postal_address, &doc.company.physical_address);
    let to = ("TO", &client.name, "CUSTOMER VAT NO: ", &client.vat_no,
        &client.postal_address, &client.physical_address);
    for (i, (label, name, vat_label, vat, postal, physical)) in [from, to].into_iter().enumerate() {
        let x = MARGIN_X + i as f32 * (col_w + 18.0);
        let mut y = parties_top;
        b.text(x, y, 9.5, Style::Bold, MUTED, label);
        y += 9.5 * 1.15 + 4.5;
        for line in b.wrap(name, Style::Bold, 13.5, col_w) {
            b.text(x, y, 13.5, Style::Bold, BLACK, &line);
            y += 13.5 * 1.25;
        }
        y += 9.0;
        b.text(x, y, BODY, Style::Bold, BLACK, &format!("{vat_label}{vat}"));
        y += LINE + 6.0;
        let addr_w = (col_w - 7.5) / 2.0;
        let mut addr_bottom = y;
        for (j, (addr_label, lines)) in [("POSTAL ADDRESS:", postal), ("PHYSICAL ADDRESS:", physical)].into_iter().enumerate() {
            let ax = x + j as f32 * (addr_w + 7.5);
            let mut ay = y;
            b.text(ax, ay, BODY, Style::Bold, BLACK, addr_label);
            ay += LINE + 3.0;
            for line in lines.iter().filter(|l| !l.is_empty()) {
                for wrapped in b.wrap(line, Style::Regular, BODY, addr_w) {
                    b.text(ax, ay, BODY, Style::Regular, Rgb(0x11, 0x11, 0x11), &wrapped);
                    ay += BODY * 1.55;
                }
            }
            addr_bottom = addr_bottom.max(ay);
        }
        parties_bottom = parties_bottom.max(addr_bottom);
    }
    b.y = parties_bottom + 16.5;

    // ---- Line items table
    let headers = ["Description", "Quantity", "Unit Price", "VAT %", "Excl. Total", "Incl. Total"];
    let desc_w = CONTENT_W * 0.36;
    let num_w = (CONTENT_W - desc_w) / 5.0;
    let pad_x = 6.0;
    // Right edge of each numeric column.
    let col_right = |c: usize| MARGIN_X + desc_w + num_w * c as f32 - pad_x;
    let header = |b: &mut Builder| {
        let h = LINE + 9.0;
        b.rect(MARGIN_X, b.y, CONTENT_W, h, HEADER_BG);
        b.rect(MARGIN_X, b.y, CONTENT_W, 0.9, RULE);
        b.rect(MARGIN_X, b.y + h - 0.9, CONTENT_W, 0.9, RULE);
        let ty = b.y + 4.5;
        b.text(MARGIN_X + pad_x, ty, BODY, Style::Italic, HEADER, headers[0]);
        for (c, title) in headers.iter().enumerate().skip(1) {
            b.text_right(col_right(c), ty, BODY, Style::Italic, HEADER, title);
        }
        b.y += h;
    };
    header(&mut b);
    let count = inv.line_items.len();
    for (i, li) in inv.line_items.iter().enumerate() {
        let desc = b.wrap(&li.description, Style::Italic, BODY, desc_w - 2.0 * pad_x);
        let row_h = desc.len() as f32 * LINE + 12.0;
        if b.ensure_room(row_h) {
            header(&mut b);
        }
        let ty = b.y + 6.0;
        for (k, line) in desc.iter().enumerate() {
            b.text(MARGIN_X + pad_x, ty + k as f32 * LINE, BODY, Style::Italic, INK, line);
        }
        let cells = [
            (format_qty(li.quantity), Style::Bold, BLACK),
            (s.money(li.unit_price), Style::Regular, INK),
            (format!("{:.2}%", li.vat_pct), Style::Regular, INK),
            (s.money(li.excl()), Style::Regular, INK),
            (s.money(li.incl()), Style::Bold, BLACK),
        ];
        for (c, (text, style, color)) in cells.iter().enumerate() {
            b.text_right(col_right(c + 1), ty, BODY, *style, *color, text);
        }
        b.y += row_h;
        let (thickness, color) = if i + 1 == count { (0.9, RULE) } else { (0.75, RULE_LIGHT) };
        b.rect(MARGIN_X, b.y - thickness, CONTENT_W, thickness, color);
    }

    // ---- Banking details and totals
    let footer_h = 25.5 + 10.5 + 4.0 * (LINE + 4.5) + 9.0 + 16.5 + 9.5 * 1.15 + 3.0 + 15.0 * 1.15;
    b.y += 25.5;
    b.ensure_room(footer_h - 25.5);
    b.rect(MARGIN_X, b.y, CONTENT_W, 0.75, RULE_LIGHT);
    b.y += 10.5;
    let footer_top = b.y;
    let bank = &doc.company.bank;
    let bank_rows = [
        format!("Bank: {}", bank.name),
        format!("Account Holder: {}", bank.account_holder),
        format!("Account Number: {}", bank.account_number),
        format!("Account Type: {}", bank.account_type),
        format!("Branch Code: {}", bank.branch_code),
    ];
    for (i, row) in bank_rows.iter().enumerate() {
        b.text(MARGIN_X, footer_top + i as f32 * (LINE + 1.5), BODY, Style::Regular, BLACK, row);
    }
    let totals_x = MARGIN_X + col_w + 18.0;
    let mut y = footer_top;
    let rows = [
        ("Total Exclusive:", totals.total_exclusive, false),
        ("Total VAT:", totals.total_vat, false),
        ("Sub Total:", totals.sub_total, false),
        ("Grand Total:", totals.grand_total, true),
    ];
    for (label, amount, grand) in rows {
        if grand {
            y += 9.0;
        }
        let style = if grand { Style::Bold } else { Style::Regular };
        b.text(totals_x, y + 2.25, BODY, style, BLACK, label);
        b.text_right(right, y + 2.25, BODY, Style::Bold, BLACK, &s.money(amount));
        y += LINE + 4.5;
    }
    y += 16.5;
    b.text_right(right, y, 9.5, Style::Bold, HEADER, "BALANCE DUE");
    y += 9.5 * 1.15 + 3.0;
    b.text_right(right, y, 15.0, Style::Bold, BLACK, &s.money(totals.grand_total));

    // ---- Page count
    let page_count = b.pages.len();
    if let Some(Item::Text { x, text, .. }) = b.pages[0].get_mut(page_item) {
        *text = format!("1/{page_count}");
        *x = right - fonts.width(text, Style::Regular, BODY);
    }
    b.pages
}

/// `6` rather than `6.00`, but keep real fractions like `1.5`.
fn format_qty(q: f64) -> String {
    if q.fract() == 0.0 { format!("{q:.0}") } else { format!("{q}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(pages: &[Page]) -> Vec<String> {
        pages
            .iter()
            .flatten()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn seed() -> Data {
        Data::from_json(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample-data.json"))).unwrap()
    }

    #[test]
    fn seed_invoice_fits_one_page() {
        let data = seed();
        let fonts = InvoiceFonts::load();
        let doc = InvoiceDoc {
            company: &data.company,
            client: data.clients.first(),
            settings: &data.settings,
            invoice: &data.invoices[0],
            logo_size: Some((400, 120)),
        };
        let pages = layout_invoice(&doc, &fonts);
        assert_eq!(pages.len(), 1);
        let t = texts(&pages);
        for expected in ["INV0000001", "19/01/2026", "Sample Client Ltd", "R3,300.00", "1/1"] {
            assert!(t.iter().any(|s| s == expected), "missing {expected}");
        }
        // Everything stays on the page.
        for item in &pages[0] {
            if let Item::Text { x, y, .. } = item {
                assert!(*x >= 0.0 && *x < PAGE_W && *y < PAGE_H, "{item:?}");
            }
        }
    }

    #[test]
    fn long_invoices_flow_onto_more_pages() {
        let mut data = seed();
        let line = data.invoices[0].line_items[0].clone();
        data.invoices[0].line_items = vec![line; 60];
        let fonts = InvoiceFonts::load();
        let doc = InvoiceDoc {
            company: &data.company,
            client: data.clients.first(),
            settings: &data.settings,
            invoice: &data.invoices[0],
            logo_size: None,
        };
        let pages = layout_invoice(&doc, &fonts);
        assert!(pages.len() >= 2);
        assert!(texts(&pages).contains(&format!("1/{}", pages.len())));
    }
}

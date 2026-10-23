import type { Api } from "@inventory/shared";
import type { Locale } from "@/lib/format";
import { localeOf } from "@/lib/business";
import { label, showDate, showDateTime, showMoney, showQty, showRate } from "@/lib/format";
import { reasonLabel } from "@/lib/till";

export type Paper = "a4" | "thermal_80" | "thermal_58";

function esc(text: string | null | undefined): string {
  return (text ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

const BASE_CSS = `
* { box-sizing: border-box; }
body { margin: 0; font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; color: #000; }
table { width: 100%; border-collapse: collapse; }
.r { text-align: right; } .c { text-align: center; } .b { font-weight: 700; }
.muted { color: #444; } .small { font-size: 0.85em; }
`;

function thermalCss(widthMm: number): string {
  return `${BASE_CSS}
@page { size: ${widthMm}mm auto; margin: 2mm; }
body { width: ${widthMm - 4}mm; font-size: ${widthMm >= 80 ? 12 : 10.5}px; line-height: 1.35; }
hr { border: 0; border-top: 1px dashed #000; margin: 4px 0; }
td { padding: 1px 0; vertical-align: top; }
.total td { font-size: 1.25em; font-weight: 700; }`;
}

const A4_CSS = `${BASE_CSS}
@page { size: A4; margin: 14mm; }
body { font-size: 12px; }
h1 { font-size: 20px; margin: 0; }
.head { display: flex; justify-content: space-between; gap: 16px; margin-bottom: 16px; }
.box { border: 1px solid #999; padding: 8px; border-radius: 4px; }
th { background: #f0f0f0; text-align: left; font-weight: 600; }
th, td { border: 1px solid #bbb; padding: 4px 6px; vertical-align: top; }
.totals td { border: none; padding: 2px 6px; }
.totals .grand td { font-size: 14px; font-weight: 700; border-top: 1px solid #000; }`;

function taxRows(invoice: Api.InvoiceDetail, l: Locale): string {
  return invoice.taxSummary
    .filter((t) => t.taxRate > 0)
    .map((t) => {
      const parts = [
        t.cgst ? `CGST ${showMoney(t.cgst, l)}` : "",
        t.sgst ? `SGST ${showMoney(t.sgst, l)}` : "",
        t.igst ? `IGST ${showMoney(t.igst, l)}` : "",
        t.cess ? `Cess ${showMoney(t.cess, l)}` : "",
        t.otherTax ? `Tax ${showMoney(t.otherTax, l)}` : "",
      ].filter(Boolean);
      return `<tr><td>${showRate(t.taxRate)} on ${showMoney(t.taxable, l)}</td><td class="r">${parts.join(" · ")}</td></tr>`;
    })
    .join("");
}

/** "Batch B7 exp 03/2027": the batch a line was sold from, with its expiry month. */
function batchText(b: Api.LineBatch): string {
  return `Batch ${esc(b.batchNo)}${b.expiryDate ? ` exp ${esc(b.expiryDate.slice(5, 7))}/${esc(b.expiryDate.slice(0, 4))}` : ""}`;
}

/** A thermal (58 or 80 mm) receipt. */
export function receiptHtml(
  invoice: Api.InvoiceDetail,
  settings: Api.PublicSettings | undefined,
  widthMm: 58 | 80,
): string {
  const l = localeOf(settings);
  const b = settings?.business;
  const lines = invoice.lines
    .map(
      (line) => `<tr><td colspan="3">${esc(line.itemName)}</td></tr>
<tr class="small"><td>${showQty(line.qty)} ${esc(line.unitName)} × ${showMoney(line.unitPrice, l)}</td>
<td class="r">${line.lineDiscount ? `−${showMoney(line.lineDiscount, l)}` : ""}</td>
<td class="r">${showMoney(line.lineTotal, l)}</td></tr>${(line.batches ?? [])
        .map((b) => `<tr class="small muted"><td colspan="3">${batchText(b)}</td></tr>`)
        .join("")}`,
    )
    .join("");
  const paid = invoice.payments
    .map(
      (p) =>
        `<tr><td>${esc(label(p.mode))}</td><td class="r">${showMoney(p.tendered ?? p.amount, l)}</td></tr>` +
        (p.changeGiven
          ? `<tr><td>Change</td><td class="r">${showMoney(p.changeGiven, l)}</td></tr>`
          : ""),
    )
    .join("");
  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(invoice.docNo)}</title><style>${thermalCss(widthMm)}</style></head><body>
<div class="c b" style="font-size:1.2em">${esc(b?.name)}</div>
${b?.address ? `<div class="c small">${esc(b.address)}</div>` : ""}
${b?.phone ? `<div class="c small">Ph ${esc(b.phone)}</div>` : ""}
${b?.gstin ? `<div class="c small">GSTIN ${esc(b.gstin)}</div>` : ""}
<hr>
<table><tr><td>${invoice.taxTotal > 0 ? "Tax invoice" : "Bill"} <span class="b">${esc(invoice.docNo)}</span></td><td class="r">${showDate(invoice.businessDate, l.locale)}</td></tr>
<tr><td colspan="2">${esc(invoice.customerName)}${invoice.customerGstin ? ` · ${esc(invoice.customerGstin)}` : ""}</td></tr></table>
<hr><table>${lines}</table><hr>
<table>
<tr><td>Items total</td><td class="r">${showMoney(invoice.gross, l)}</td></tr>
${invoice.lineDiscount + invoice.billDiscount ? `<tr><td>Discount</td><td class="r">−${showMoney(invoice.lineDiscount + invoice.billDiscount, l)}</td></tr>` : ""}
${invoice.taxTotal ? `<tr><td>Tax</td><td class="r">${showMoney(invoice.taxTotal, l)}</td></tr>` : ""}
${invoice.roundOff ? `<tr><td>Round off</td><td class="r">${showMoney(invoice.roundOff, l)}</td></tr>` : ""}
<tr class="total"><td>Total</td><td class="r">${showMoney(invoice.total, l)}</td></tr>
${paid}
${invoice.balanceDue > 0 ? `<tr class="b"><td>Balance due</td><td class="r">${showMoney(invoice.balanceDue, l)}</td></tr>` : ""}
</table>
${invoice.taxTotal ? `<hr><table class="small">${taxRows(invoice, l)}</table>` : ""}
<hr>
${invoice.origin === "offline" ? `<div class="c small">Billed offline · syncs to the hub</div>` : ""}
${settings?.invoice.footerText ? `<div class="c small">${esc(settings.invoice.footerText)}</div>` : `<div class="c small">Thank you!</div>`}
</body></html>`;
}

/** A4 tax invoice (also the PDF: print → "Save as PDF"). */
export function invoiceHtml(
  invoice: Api.InvoiceDetail,
  settings: Api.PublicSettings | undefined,
): string {
  const l = localeOf(settings);
  const b = settings?.business;
  const rows = invoice.lines
    .map(
      (
        line,
        i,
      ) => `<tr><td>${i + 1}</td><td>${esc(line.itemName)}${line.hsnSac ? `<div class="small muted">HSN ${esc(line.hsnSac)}</div>` : ""}${(
        line.batches ?? []
      )
        .map((b) => `<div class="small muted">${batchText(b)}</div>`)
        .join("")}</td>
<td class="r">${showQty(line.qty)} ${esc(line.unitName)}</td><td class="r">${showMoney(line.unitPrice, l)}</td>
<td class="r">${line.lineDiscount + line.billDiscountShare ? showMoney(line.lineDiscount + line.billDiscountShare, l) : ""}</td>
<td class="r">${showMoney(line.taxable, l)}</td><td class="r">${showRate(line.taxRate)}</td>
<td class="r">${showMoney(line.taxTotal, l)}</td><td class="r">${showMoney(line.lineTotal, l)}</td></tr>`,
    )
    .join("");
  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(invoice.docNo)}</title><style>${A4_CSS}</style></head><body>
<div class="head">
  <div><h1>${esc(b?.name)}</h1><div>${esc(b?.address)}</div>
  ${b?.phone ? `<div>Phone ${esc(b.phone)}</div>` : ""}${b?.gstin ? `<div>GSTIN ${esc(b.gstin)}</div>` : ""}</div>
  <div class="r"><div class="b" style="font-size:16px">${invoice.taxTotal > 0 ? "TAX INVOICE" : "INVOICE"}</div>
  <div>No. <span class="b">${esc(invoice.docNo)}</span></div><div>Date ${showDate(invoice.businessDate, l.locale)}</div>
  ${invoice.status === "voided" ? `<div class="b" style="color:#b00">VOIDED</div>` : ""}</div>
</div>
<div class="box" style="margin-bottom:12px"><div class="small muted">Bill to</div>
  <div class="b">${esc(invoice.customerName)}</div>${invoice.billingAddress ? `<div>${esc(invoice.billingAddress)}</div>` : ""}
  ${invoice.customerPhone ? `<div>${esc(invoice.customerPhone)}</div>` : ""}${invoice.customerGstin ? `<div>GSTIN ${esc(invoice.customerGstin)}</div>` : ""}
  ${invoice.placeOfSupply ? `<div class="small">Place of supply: ${esc(invoice.placeOfSupply)}</div>` : ""}</div>
<table><thead><tr><th>#</th><th>Item</th><th class="r">Qty</th><th class="r">Rate</th><th class="r">Disc.</th><th class="r">Taxable</th><th class="r">Tax %</th><th class="r">Tax</th><th class="r">Amount</th></tr></thead>
<tbody>${rows}</tbody></table>
<table class="totals" style="width:45%;margin-left:auto;margin-top:8px">
<tr><td>Taxable value</td><td class="r">${showMoney(invoice.taxable, l)}</td></tr>
${taxRows(invoice, l)}
${invoice.roundOff ? `<tr><td>Round off</td><td class="r">${showMoney(invoice.roundOff, l)}</td></tr>` : ""}
<tr class="grand"><td>Total</td><td class="r">${showMoney(invoice.total, l)}</td></tr>
<tr><td>Paid</td><td class="r">${showMoney(invoice.amountPaid, l)}</td></tr>
${invoice.balanceDue > 0 ? `<tr class="b"><td>Balance due</td><td class="r">${showMoney(invoice.balanceDue, l)}</td></tr>` : ""}
</table>
${settings?.invoice.terms ? `<div style="margin-top:16px" class="small"><div class="b">Terms</div>${esc(settings.invoice.terms)}</div>` : ""}
${settings?.invoice.footerText ? `<div class="c small" style="margin-top:24px">${esc(settings.invoice.footerText)}</div>` : ""}
</body></html>`;
}

export function documentHtml(
  invoice: Api.InvoiceDetail,
  settings: Api.PublicSettings | undefined,
  paper: Paper,
): string {
  if (paper === "a4") return invoiceHtml(invoice, settings);
  return receiptHtml(invoice, settings, paper === "thermal_58" ? 58 : 80);
}

/** A credit note for the customer (thermal), as the receipt printer's return slip. */
export function returnSlipHtml(
  ret: Api.ReturnDetail,
  settings: Api.PublicSettings | undefined,
  widthMm: 58 | 80 = 80,
): string {
  const l = localeOf(settings);
  const b = settings?.business;
  const lines = ret.lines
    .map(
      (line) => `<tr><td colspan="2">${esc(line.itemName)}</td></tr>
<tr class="small"><td>${showQty(line.qty)} × ${showMoney(line.unitPrice, l)}</td><td class="r">${showMoney(line.lineTotal, l)}</td></tr>`,
    )
    .join("");
  const credit = ret.total - ret.amountRefunded;
  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(ret.docNo)}</title><style>${thermalCss(widthMm)}</style></head><body>
<div class="c b" style="font-size:1.2em">${esc(b?.name)}</div>
${b?.address ? `<div class="c small">${esc(b.address)}</div>` : ""}
${b?.gstin ? `<div class="c small">GSTIN ${esc(b.gstin)}</div>` : ""}
<hr><div class="c b">CREDIT NOTE</div>
<table><tr><td class="b">${esc(ret.docNo)}</td><td class="r">${showDate(ret.businessDate, l.locale)}</td></tr>
<tr><td colspan="2">${esc(ret.customerName)}</td></tr>
${ret.reason ? `<tr><td colspan="2" class="small">Reason: ${esc(ret.reason)}</td></tr>` : ""}</table>
<hr><table>${lines}</table><hr>
<table>
<tr><td>Taxable</td><td class="r">${showMoney(ret.taxable, l)}</td></tr>
${ret.taxTotal ? `<tr><td>Tax</td><td class="r">${showMoney(ret.taxTotal, l)}</td></tr>` : ""}
<tr class="total"><td>Total</td><td class="r">${showMoney(ret.total, l)}</td></tr>
${ret.amountRefunded ? `<tr><td>Refunded</td><td class="r">${showMoney(ret.amountRefunded, l)}</td></tr>` : ""}
${credit > 0 ? `<tr><td>Kept as credit</td><td class="r">${showMoney(credit, l)}</td></tr>` : ""}
</table><hr><p>Customer signature ____________</p>
</body></html>`;
}

/** The till's cash-up when it is closed (80 mm), as the thermal printer's version. */
export function tillCloseHtml(
  session: Api.RegisterSession,
  settings: Api.PublicSettings | undefined,
): string {
  const l = localeOf(settings);
  const diff = session.difference ?? 0;
  const diffLabel = diff < 0 ? "Short" : diff > 0 ? "Over" : "Difference";
  const row = (k: string, v: string, cls = "") =>
    `<tr class="${cls}"><td>${esc(k)}</td><td class="r">${v}</td></tr>`;
  return `<!doctype html><html><head><meta charset="utf-8"><title>Till close</title><style>${thermalCss(80)}</style></head><body>
<div class="c b" style="font-size:1.2em">${esc(settings?.business.name)}</div>
<div class="c b">TILL CLOSE</div><hr>
<table>
${row("Till", esc(session.registerName))}
${row("Opened", showDateTime(session.openedAt, l.locale))}
${session.openedByName ? row("Opened by", esc(session.openedByName)) : ""}
${session.closedAt ? row("Closed", showDateTime(session.closedAt, l.locale)) : ""}
</table><hr><table>
${row("Opening float", showMoney(session.openingFloat, l))}
${row("Cash in", showMoney(session.cashIn, l))}
${row("Cash out", `−${showMoney(session.cashOut, l)}`)}
${row("Expected cash", showMoney(session.expectedCash, l), "b")}
${session.countedCash !== null && session.countedCash !== undefined ? row("Counted cash", showMoney(session.countedCash, l)) : ""}
${row(diffLabel, showMoney(Math.abs(diff), l), "total")}
${session.differenceReason ? row("Reason", esc(reasonLabel(session.differenceReason))) : ""}
${session.differenceApprovedBy ? row("Approved by", esc(session.differenceApprovedBy)) : ""}
</table>${session.notes ? `<p>${esc(session.notes)}</p>` : ""}<hr>
<p>Cashier ____________</p><p>Manager ____________</p>
</body></html>`;
}

/** The day-close summary (80 mm), as the thermal printer's version. */
export function dayCloseHtml(
  day: Api.DaySummary,
  settings: Api.PublicSettings | undefined,
): string {
  const l = localeOf(settings);
  const row = (k: string, v: string, cls = "") =>
    `<tr class="${cls}"><td>${esc(k)}</td><td class="r">${v}</td></tr>`;
  const modes = (items: Api.ModeAmount[]) =>
    items.map((m) => row(label(m.mode), showMoney(m.amount, l))).join("");
  const tills = day.tills
    .map(
      (t) =>
        row(t.register, showMoney(t.countedCash, l)) +
        (t.difference !== 0
          ? row(t.difference < 0 ? "  Short" : "  Over", showMoney(Math.abs(t.difference), l)) +
            (t.differenceReason ? row("  Reason", esc(reasonLabel(t.differenceReason))) : "") +
            (t.approvedBy ? row("  Approved by", esc(t.approvedBy)) : "")
          : ""),
    )
    .join("");
  return `<!doctype html><html><head><meta charset="utf-8"><title>Day close</title><style>${thermalCss(80)}</style></head><body>
<div class="c b" style="font-size:1.2em">${esc(settings?.business.name)}</div>
<div class="c b">DAY CLOSE</div><div class="c">${showDate(day.businessDate, l.locale)}</div><hr>
<table>
${row("Bills", String(day.bills))}
${row("Sales", showMoney(day.salesTotal, l))}
${row("Tax in sales", showMoney(day.taxTotal, l))}
${row("Discounts", showMoney(day.discountTotal, l))}
${row("Returns", `−${showMoney(day.returnsTotal, l)}`)}
${row("Net sales", showMoney(day.salesTotal - day.returnsTotal, l), "total")}
${row("Purchases", showMoney(day.purchasesTotal, l))}
${row("Expenses", showMoney(day.expensesTotal, l))}
</table>
${day.received.length ? `<hr><div class="b">Received</div><table>${modes(day.received)}</table>` : ""}
${day.paid.length ? `<div class="b">Paid out</div><table>${modes(day.paid)}</table>` : ""}
${day.tills.length ? `<hr><div class="b">Tills</div><table>${tills}</table>` : ""}
<hr>${day.closedAt ? `<table>${row("Closed", showDateTime(day.closedAt, l.locale))}${row("Closed by", esc(day.closedBy ?? ""))}</table>` : ""}
${day.notes ? `<p>${esc(day.notes)}</p>` : ""}
<p>Signature ____________</p>
</body></html>`;
}

/** Prints HTML through a hidden frame (the OS print dialog; "Save as PDF" makes the PDF). */
export function printHtml(html: string): void {
  const frame = document.createElement("iframe");
  frame.setAttribute("aria-hidden", "true");
  frame.style.position = "fixed";
  frame.style.width = "0";
  frame.style.height = "0";
  frame.style.border = "0";
  document.body.appendChild(frame);
  const cleanup = () => setTimeout(() => frame.remove(), 1000);
  frame.onload = () => {
    frame.contentWindow?.focus();
    frame.contentWindow?.print();
    cleanup();
  };
  frame.srcdoc = html;
}

export function defaultPaper(settings: Api.PublicSettings | undefined): Paper {
  const p = settings?.invoice.paper;
  return p === "a4" || p === "thermal_58" ? p : "thermal_80";
}

const DOC_TITLES: Record<string, string> = {
  quotation: "QUOTATION",
  sales_order: "SALES ORDER",
  delivery_challan: "DELIVERY CHALLAN",
};

/** A4 quotation, sales order or delivery challan. */
export function salesDocHtml(
  doc: Api.SalesDocDetail,
  settings: Api.PublicSettings | undefined,
): string {
  const l = localeOf(settings);
  const b = settings?.business;
  const rows = doc.lines
    .map(
      (
        line,
        i,
      ) => `<tr><td>${i + 1}</td><td>${esc(line.itemName)}</td><td class="r">${showQty(line.qty)}</td>
<td class="r">${showMoney(line.unitPrice, l)}</td><td class="r">${line.discount ? showMoney(line.discount, l) : ""}</td>
<td class="r">${showMoney(line.taxTotal, l)}</td><td class="r">${showMoney(line.lineTotal, l)}</td></tr>`,
    )
    .join("");
  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(doc.docNo)}</title><style>${A4_CSS}</style></head><body>
<div class="head">
  <div><h1>${esc(b?.name)}</h1><div>${esc(b?.address)}</div>${b?.phone ? `<div>Phone ${esc(b.phone)}</div>` : ""}${b?.gstin ? `<div>GSTIN ${esc(b.gstin)}</div>` : ""}</div>
  <div class="r"><div class="b" style="font-size:16px">${DOC_TITLES[doc.docType] ?? "DOCUMENT"}</div>
  <div>No. <span class="b">${esc(doc.docNo)}</span></div><div>Date ${showDate(doc.businessDate, l.locale)}</div>
  ${doc.validUntil ? `<div>Valid until ${showDate(doc.validUntil, l.locale)}</div>` : ""}</div>
</div>
<div class="box" style="margin-bottom:12px"><div class="small muted">For</div><div class="b">${esc(doc.customerName)}</div></div>
<table><thead><tr><th>#</th><th>Item</th><th class="r">Qty</th><th class="r">Rate</th><th class="r">Disc.</th><th class="r">Tax</th><th class="r">Amount</th></tr></thead>
<tbody>${rows}</tbody></table>
<table class="totals" style="width:45%;margin-left:auto;margin-top:8px">
<tr><td>Taxable value</td><td class="r">${showMoney(doc.taxable, l)}</td></tr>
<tr><td>Tax</td><td class="r">${showMoney(doc.taxTotal, l)}</td></tr>
${doc.roundOff ? `<tr><td>Round off</td><td class="r">${showMoney(doc.roundOff, l)}</td></tr>` : ""}
<tr class="grand"><td>Total</td><td class="r">${showMoney(doc.total, l)}</td></tr></table>
${doc.terms ? `<div style="margin-top:16px" class="small"><div class="b">Terms</div>${esc(doc.terms)}</div>` : ""}
${doc.notes ? `<div style="margin-top:8px" class="small">${esc(doc.notes)}</div>` : ""}
</body></html>`;
}

import type { Api } from "@inventory/shared";
import { showDate, showMoney, showQty, showRate } from "./format";
import type { Locale } from "./format";
import { DEFAULT_LOCALE } from "./format";

/** One cell as text, the way the screen shows it. */
export function cellText(
  column: Api.Column,
  value: unknown,
  locale: Locale = DEFAULT_LOCALE,
): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "number") {
    switch (column.kind) {
      case "money":
        return showMoney(value, locale);
      case "qty":
        return showQty(value);
      case "rate":
        return showRate(value);
      case "percent":
        return `${(value / 10_000).toFixed(1)}%`;
      default:
        return String(value);
    }
  }
  if (typeof value === "string") return column.kind === "date" ? showDate(value) : value;
  return JSON.stringify(value);
}

/** A raw (machine) value for spreadsheets: money as a decimal number, quantities as numbers. */
export function rawValue(column: Api.Column, value: unknown, minorDigits: number): string | number {
  if (value === null || value === undefined) return "";
  if (typeof value === "number") {
    switch (column.kind) {
      case "money":
        return value / 10 ** minorDigits;
      case "qty":
        return value / 1000;
      case "rate":
      case "percent":
        return value / 1_000_000;
      default:
        return value;
    }
  }
  return typeof value === "string" ? value : JSON.stringify(value);
}

function csvField(text: string): string {
  // A cell that starts with = + - @ would run as a formula in a spreadsheet: keep it text.
  const safe = /^[=+@]/.test(text) || /^-(?!\d)/.test(text) ? `'${text}` : text;
  return /[",\n\r]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe;
}

/** CSV with exact decimals (no currency symbols), a header row and the totals line. */
export function tableToCsv(table: Api.ReportTable, minorDigits = 2): string {
  const lines = [table.columns.map((c) => csvField(c.label)).join(",")];
  const add = (row: unknown[]) =>
    lines.push(
      table.columns
        .map((c, i) => {
          const v = rawValue(c, row[i], minorDigits);
          return csvField(typeof v === "number" ? String(v) : v);
        })
        .join(","),
    );
  for (const row of table.rows) add(row);
  if (table.totals) add(table.totals);
  const bom = String.fromCharCode(0xfeff); // so Excel reads the file as UTF-8
  const newline = String.fromCharCode(13, 10);
  return bom + lines.join(newline) + newline;
}

function esc(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** A printable page (A4; "Save as PDF" in the print dialog makes the PDF). */
export function tableToHtml(
  table: Api.ReportTable,
  business: string,
  locale: Locale = DEFAULT_LOCALE,
): string {
  const numeric = (c: Api.Column) => ["money", "qty", "rate", "percent", "count"].includes(c.kind);
  const head = table.columns
    .map((c) => `<th class="${numeric(c) ? "r" : ""}">${esc(c.label)}</th>`)
    .join("");
  const body = (row: unknown[], cls = "") =>
    `<tr class="${cls}">${table.columns
      .map(
        (c, i) => `<td class="${numeric(c) ? "r" : ""}">${esc(cellText(c, row[i], locale))}</td>`,
      )
      .join("")}</tr>`;
  const period =
    table.from === table.to ? table.to : `${showDate(table.from)} – ${showDate(table.to)}`;
  return `<!doctype html><html><head><meta charset="utf-8"><title>${esc(table.title)}</title><style>
@page { size: A4 landscape; margin: 12mm; }
body { font-family: system-ui, "Segoe UI", sans-serif; font-size: 11px; color: #000; }
h1 { font-size: 16px; margin: 0 0 2px; } p { margin: 0 0 10px; color: #444; }
table { width: 100%; border-collapse: collapse; } th, td { border-bottom: 1px solid #ccc; padding: 3px 6px; text-align: left; }
th { background: #f0f0f0; } .r { text-align: right; font-variant-numeric: tabular-nums; } .total td { font-weight: 700; border-top: 2px solid #000; }
</style></head><body><h1>${esc(table.title)}</h1><p>${esc(business)} · ${esc(period)}${table.truncated ? " · first rows only" : ""}</p>
<table><thead><tr>${head}</tr></thead><tbody>${table.rows.map((r) => body(r)).join("")}${table.totals ? body(table.totals, "total") : ""}</tbody></table></body></html>`;
}

export function fileName(table: Api.ReportTable, extension: string): string {
  const slug = table.title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  return `${slug}-${table.from}-to-${table.to}.${extension}`;
}

/** Offers a text file for download. */
export function downloadText(name: string, text: string, type: string): void {
  downloadBlob(name, new Blob([text], { type }));
}

export function downloadBlob(name: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}

/** An Excel workbook (the spreadsheet library loads only when this is used). */
export async function downloadXlsx(table: Api.ReportTable, minorDigits = 2): Promise<void> {
  const XLSX = await import("xlsx");
  const header = table.columns.map((c) => c.label);
  const rows = [...table.rows, ...(table.totals ? [table.totals] : [])].map((row) =>
    table.columns.map((c, i) => rawValue(c, row[i], minorDigits)),
  );
  const sheet = XLSX.utils.aoa_to_sheet([header, ...rows]);
  sheet["!cols"] = table.columns.map((c) => ({ wch: Math.max(12, c.label.length + 2) }));
  const book = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(book, sheet, table.title.slice(0, 31).replace(/[\\/?*[\]:]/g, " "));
  const data = XLSX.write(book, { bookType: "xlsx", type: "array" }) as ArrayBuffer;
  downloadBlob(
    fileName(table, "xlsx"),
    new Blob([data], {
      type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    }),
  );
}

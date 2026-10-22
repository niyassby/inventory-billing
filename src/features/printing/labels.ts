import type { Api } from "@inventory/shared";

/** A4 in millimetres, and the sheet margin. */
const A4 = { width: 210, height: 297, margin: 5, gap: 2 } as const;

export const MAX_LABELS = 2000;

/** Labels of a template that fit on one A4 sheet (columns × rows). */
export function sheetGrid(template: Pick<Api.LabelTemplate, "widthMm" | "heightMm">): {
  columns: number;
  rows: number;
} {
  const usableW = A4.width - 2 * A4.margin;
  const usableH = A4.height - 2 * A4.margin;
  const columns = Math.max(1, Math.floor((usableW + A4.gap) / (template.widthMm + A4.gap)));
  const rows = Math.max(1, Math.floor((usableH + A4.gap) / (template.heightMm + A4.gap)));
  return { columns, rows };
}

export function totalLabels(items: readonly Pick<Api.LabelItem, "copies">[]): number {
  return items.reduce((n, i) => n + Math.max(0, i.copies), 0);
}

/** Adds items, merging a product already in the list (its copies add up). */
export function mergeItems(list: Api.LabelItem[], added: Api.LabelItem[]): Api.LabelItem[] {
  const next = [...list];
  for (const item of added) {
    const same = next.findIndex(
      (i) =>
        i.variantId === item.variantId &&
        (i.batchNo ?? null) === (item.batchNo ?? null) &&
        (i.expiryDate ?? null) === (item.expiryDate ?? null),
    );
    if (same >= 0) {
      const current = next[same];
      if (current) next[same] = { ...current, copies: current.copies + item.copies };
    } else {
      next.push(item);
    }
  }
  return next;
}

function svgUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/**
 * A4 sheets of labels for the system print dialog (sticker sheets on an office printer). The
 * labels are the backend's own drawings (labels_preview), repeated for their copies.
 */
export function labelSheetHtml(
  template: Pick<Api.LabelTemplate, "widthMm" | "heightMm" | "name">,
  previews: readonly { svg: string; copies: number }[],
): string {
  const { columns } = sheetGrid(template);
  const cells = previews
    .flatMap((p) => Array.from({ length: Math.max(0, p.copies) }, () => p.svg))
    .map(
      (svg) =>
        `<img alt="" src="${svgUrl(svg)}" style="width:${template.widthMm}mm;height:${template.heightMm}mm">`,
    )
    .join("");
  const title = template.name.replace(/[<>&"]/g, "");
  return `<!doctype html><html><head><meta charset="utf-8"><title>${title}</title><style>
@page { size: A4; margin: ${A4.margin}mm; }
body { margin: 0; }
.sheet { display: grid; grid-template-columns: repeat(${columns}, ${template.widthMm}mm); gap: ${A4.gap}mm; }
img { display: block; break-inside: avoid; }
</style></head><body><div class="sheet">${cells}</div></body></html>`;
}

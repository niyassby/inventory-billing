import type { Api } from "@inventory/shared";

/** A first guess at the columns from the header's words (Indian banks' usual names). */
export function guessMapping(columns: string[]): Api.StatementMapping | null {
  const find = (re: RegExp) => {
    const i = columns.findIndex((c) => re.test(c));
    return i >= 0 ? i : null;
  };
  const date = find(/date/i);
  const description = find(/narration|description|particulars|details|remarks/i);
  if (date === null || description === null) return null;
  const withdrawal = find(/withdraw|debit|^dr\b/i);
  const deposit = find(/deposit|credit|^cr\b/i);
  const amount = find(/amount/i);
  return {
    skipRows: 0,
    hasHeader: true,
    dateColumn: date,
    dateFormat: "DD/MM/YYYY",
    descriptionColumn: description,
    referenceColumn: find(/ref|chq|cheque|utr/i),
    amountColumn: withdrawal !== null || deposit !== null ? null : amount,
    withdrawalColumn: withdrawal,
    depositColumn: deposit,
    balanceColumn: find(/balance/i),
  };
}

/** The mapping names where the money is (otherwise there is nothing to preview yet). */
export function complete(m: Api.StatementMapping): boolean {
  return [m.amountColumn, m.withdrawalColumn, m.depositColumn].some(
    (c) => c !== null && c !== undefined,
  );
}

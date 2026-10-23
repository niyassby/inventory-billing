import { describe, expect, it } from "vitest";
import { complete, guessMapping } from "./statement-mapping";

describe("statement column guess", () => {
  it("finds the usual Indian bank columns", () => {
    const m = guessMapping([
      "Txn Date",
      "Narration",
      "Chq/Ref No",
      "Withdrawal Amt",
      "Deposit Amt",
      "Closing Balance",
    ]);
    expect(m).toMatchObject({
      dateColumn: 0,
      descriptionColumn: 1,
      referenceColumn: 2,
      withdrawalColumn: 3,
      depositColumn: 4,
      amountColumn: null,
      balanceColumn: 5,
    });
    expect(m && complete(m)).toBe(true);
  });

  it("uses one signed amount column when there are no debit and credit columns", () => {
    const m = guessMapping(["Date", "Description", "Amount"]);
    expect(m).toMatchObject({ amountColumn: 2, withdrawalColumn: null, depositColumn: null });
    expect(guessMapping(["Value date", "Particulars", "Dr", "Cr"])).toMatchObject({
      withdrawalColumn: 2,
      depositColumn: 3,
    });
  });

  it("gives up without a date and a description, and waits for the money columns", () => {
    expect(guessMapping(["Column 1", "Column 2"])).toBeNull();
    const m = guessMapping(["Date", "Remarks"]);
    expect(m && complete(m)).toBe(false);
  });
});

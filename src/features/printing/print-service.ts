import { toast } from "sonner";
import type { Api } from "@inventory/shared";
import { call } from "@/api/v2";
import { errorText } from "@/lib/queries";
import { printHtml } from "@/features/sales/print";

export interface PrintRequest {
  document: Api.PrintDocument;
  /** The page for the system print dialog, when no printer is set up for this document. */
  fallbackHtml?: () => string;
  /** Say nothing when it printed (the till prints after every sale). */
  quietSuccess?: boolean;
  /** A printer of this PC instead of the document's route. */
  printerId?: string;
}

/**
 * Prints a document the way this PC is set up: straight to its printer (or the hub's queue), or
 * through the system print dialog when no printer is chosen for it. A failure never undoes
 * anything (the sale is saved); it shows the reason and a Reprint button.
 */
export async function printDocument(request: PrintRequest): Promise<Api.PrintOutcome | null> {
  try {
    const outcome = await call("print_document", {
      document: request.document,
      printerId: request.printerId ?? null,
    });
    switch (outcome.status) {
      case "no_printer":
        if (request.fallbackHtml) printHtml(request.fallbackHtml());
        break;
      case "queued":
        toast.info(`Waiting to print on ${outcome.printerName ?? "the hub's printer"}`, {
          description: outcome.message ?? "The hub retries in a moment.",
        });
        break;
      case "printed":
        if (!request.quietSuccess) toast.success(`Printed on ${outcome.printerName ?? "printer"}`);
        break;
    }
    return outcome;
  } catch (error) {
    toast.error(errorText(error), {
      duration: 15_000,
      action: { label: "Reprint", onClick: () => void printDocument(request) },
    });
    return null;
  }
}

/** Opens the drawer for a cash sale when the receipt wasn't printed on the drawer's printer. */
export async function openDrawerForSale(invoice: Api.InvoiceDetail): Promise<void> {
  try {
    await call("drawer_open", { invoice });
  } catch (error) {
    toast.error(errorText(error));
  }
}

export function paidInCash(invoice: Api.InvoiceDetail): boolean {
  return invoice.payments.some((p) => p.mode === "cash" && p.status !== "voided");
}

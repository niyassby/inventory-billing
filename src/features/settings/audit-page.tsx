import { DatePicker } from "@/components/ui/date-picker";
import { SelectField } from "@/components/ui/select-field";
import { useSearchFilter, useTableState } from "@/lib/table-state";
import { useMemo, useState } from "react";
import type { Api } from "@inventory/shared";
import { Download, FileSpreadsheet } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { call } from "@/api/v2";
import { downloadBlob, downloadText, tableToCsv, tableToXlsx } from "@/lib/export";
import { errorText } from "@/lib/queries";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import { Section } from "@/components/app/page";
import { showDateTime } from "@/lib/format";
import { useCommand, useList } from "@/lib/queries";

/** The exported rows as a report table, so CSV and Excel come out like every other export. */
function auditTable(
  rows: Api.AuditRow[],
  truncated: boolean,
  from: string,
  to: string,
): Api.ReportTable {
  return {
    report: "audit_log",
    title: "Audit log",
    from: from || (rows[0]?.occurredAt.slice(0, 10) ?? ""),
    to: to || (rows[rows.length - 1]?.occurredAt.slice(0, 10) ?? ""),
    columns: [
      { key: "when", label: "When", kind: "text" },
      { key: "who", label: "Who", kind: "text" },
      { key: "approvedBy", label: "Approved by", kind: "text" },
      { key: "device", label: "Device", kind: "text" },
      { key: "action", label: "Action", kind: "text" },
      { key: "entity", label: "Record", kind: "text" },
      { key: "entityId", label: "Record id", kind: "text" },
      { key: "details", label: "Details", kind: "text" },
    ],
    rows: rows.map((r) => [
      r.occurredAt,
      r.userName ?? "system",
      r.approvedByName ?? "",
      r.deviceName ?? "",
      r.action,
      r.entityType,
      r.entityId ?? "",
      r.summary ?? "",
    ]),
    totals: null,
    truncated,
  };
}
import { useCan } from "@/stores/session";

export function AuditSection() {
  const canView = useCan("audit.view");
  const canExport = useCan("audit.export");
  const [exporting, setExporting] = useState(false);
  const table = useTableState("audit", {
    size: 100,
    filters: { q: "", action: "", from: "", to: "" },
  });
  const [search, setSearch] = useSearchFilter(table);
  const actions = useCommand("audit_actions", {}, { enabled: canView });
  const query = useList(
    "audit_list",
    {
      search: table.filter("q") || null,
      action: table.filter("action") || null,
      from: table.filter("from") || null,
      to: table.filter("to") || null,
      ...table.input,
    },
    { enabled: canView },
  );
  const columns = useMemo<ColumnDef<Api.AuditRow>[]>(
    () => [
      {
        header: "When",
        accessorKey: "occurredAt",
        meta: { sortKey: "when" },
        cell: ({ row }) => showDateTime(row.original.occurredAt),
      },
      {
        header: "Who",
        accessorFn: (r) => r.userName ?? "system",
        meta: { sortKey: "who" },
        cell: ({ row }) => (
          <span>
            {row.original.userName ?? "system"}
            {row.original.approvedByName && (
              <span className="text-xs text-muted-foreground">
                {" "}
                · approved by {row.original.approvedByName}
              </span>
            )}
            {row.original.deviceName && (
              <span className="text-xs text-muted-foreground"> · {row.original.deviceName}</span>
            )}
          </span>
        ),
      },
      {
        header: "Action",
        accessorKey: "action",
        meta: { sortKey: "action" },
        cell: ({ row }) => <span className="font-medium">{row.original.action}</span>,
      },
      {
        header: "Details",
        accessorFn: (r) => r.summary ?? "",
        meta: { sortKey: "details" },
        cell: ({ row }) => (
          <span
            className="block max-w-xl truncate text-muted-foreground"
            title={row.original.summary ?? ""}
          >
            {row.original.summary}
          </span>
        ),
      },
    ],
    [],
  );
  const exportAs = async (format: "csv" | "xlsx") => {
    setExporting(true);
    try {
      const from = table.filter("from");
      const to = table.filter("to");
      const result = await call("audit_export", {
        search: table.filter("q") || null,
        action: table.filter("action") || null,
        from: from || null,
        to: to || null,
      });
      const data = auditTable(result.rows, result.truncated, from, to);
      const name = `audit-log-${data.from || "start"}-to-${data.to || "now"}`;
      if (format === "csv") downloadText(`${name}.csv`, tableToCsv(data), "text/csv;charset=utf-8");
      else downloadBlob(`${name}.xlsx`, await tableToXlsx(data));
      if (result.truncated)
        toast.warning("Only the first 50,000 rows were exported. Narrow the filters for the rest.");
    } catch (error) {
      toast.error(errorText(error));
    } finally {
      setExporting(false);
    }
  };
  if (!canView) return null;
  return (
    <Section title="Audit log">
      <p className="mb-3 text-sm text-muted-foreground">
        Sign-ins, price changes, discounts, voids, returns, stock changes and settings, with who did
        it and who approved it. It can't be edited.
      </p>
      <div className="mb-3 flex flex-wrap gap-3">
        <SearchInput
          aria-label="Search the log"
          placeholder="Search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <SelectField
          aria-label="Action"
          className="w-56"
          value={table.filter("action")}
          onValueChange={(v) => table.setFilter("action", v)}
          options={[
            { value: "", label: "All actions" },
            ...(actions.data ?? []).map((a) => ({ value: a, label: a })),
          ]}
        />
        <DatePicker
          aria-label="From"
          className="w-44"
          placeholder="From"
          clearable
          value={table.filter("from")}
          max={table.filter("to") || undefined}
          onChange={(v) => table.setFilter("from", v)}
        />
        <DatePicker
          aria-label="To"
          className="w-44"
          placeholder="To"
          clearable
          value={table.filter("to")}
          min={table.filter("from") || undefined}
          onChange={(v) => table.setFilter("to", v)}
        />
        {canExport && (
          <span className="ml-auto flex gap-2">
            <Button variant="outline" disabled={exporting} onClick={() => void exportAs("csv")}>
              <Download /> CSV
            </Button>
            <Button variant="outline" disabled={exporting} onClick={() => void exportAs("xlsx")}>
              <FileSpreadsheet /> Excel
            </Button>
          </span>
        )}
      </div>
      <DataTable
        viewKey="audit"
        sort={table.sortProps}
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        paging={query.data ? table.paging(query.data.total) : undefined}
        empty={{ title: "Nothing matches" }}
      />
    </Section>
  );
}

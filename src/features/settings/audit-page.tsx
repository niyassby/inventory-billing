import { useMemo, useState } from "react";
import type { Api } from "@inventory/shared";
import { Input } from "@/components/ui/input";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import type { ColumnDef } from "@/components/app/data-table";
import { SearchInput } from "@/components/app/search-input";
import { DataTable } from "@/components/app/data-table";
import { Section } from "@/components/app/page";
import { useDebounced } from "@/components/app/pickers";
import { showDateTime } from "@/lib/format";
import { useCommand, useList } from "@/lib/queries";
import { useCan } from "@/stores/session";

export function AuditSection() {
  const canView = useCan("audit.view");
  const [search, setSearch] = useState("");
  const [action, setAction] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [page, setPage] = useState(1);
  const term = useDebounced(search.trim(), 250);
  const actions = useCommand("audit_actions", {}, { enabled: canView });
  const query = useList(
    "audit_list",
    {
      search: term || null,
      action: action || null,
      from: from || null,
      to: to || null,
      page,
      pageSize: 100,
    },
    { enabled: canView },
  );
  const columns = useMemo<ColumnDef<Api.AuditRow>[]>(
    () => [
      {
        header: "When",
        accessorKey: "occurredAt",
        cell: ({ row }) => showDateTime(row.original.occurredAt),
      },
      {
        header: "Who",
        accessorFn: (r) => r.userName ?? "system",
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
        cell: ({ row }) => <span className="font-medium">{row.original.action}</span>,
      },
      {
        header: "Details",
        accessorFn: (r) => r.summary ?? "",
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
          className="max-w-xs"
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            setPage(1);
          }}
        />
        <NativeSelect
          aria-label="Action"
          value={action}
          onChange={(e) => {
            setAction(e.target.value);
            setPage(1);
          }}
        >
          <NativeSelectOption value="">All actions</NativeSelectOption>
          {actions.data?.map((a) => (
            <NativeSelectOption key={a} value={a}>
              {a}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        <Input
          type="date"
          aria-label="From"
          className="w-40"
          value={from}
          onChange={(e) => {
            setFrom(e.target.value);
            setPage(1);
          }}
        />
        <Input
          type="date"
          aria-label="To"
          className="w-40"
          value={to}
          onChange={(e) => {
            setTo(e.target.value);
            setPage(1);
          }}
        />
      </div>
      <DataTable
        viewKey="audit"
        columns={columns}
        data={query.data?.items}
        loading={query.isPending}
        error={query.error}
        rowKey={(r) => r.id}
        paging={
          query.data ? { page, pageSize: 100, total: query.data.total, onPage: setPage } : undefined
        }
        empty={{ title: "Nothing matches" }}
      />
    </Section>
  );
}

import { useRef, useState } from "react";
import type { ReactNode } from "react";
import type { ColumnDef, SortingState } from "@tanstack/react-table";
import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowUpDown, ChevronLeft, ChevronRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { EmptyState, ErrorState, LoadingRows } from "./page";

export type { ColumnDef };

/** Extra column options: right-aligned numbers, fixed widths. */
export interface ColumnMeta {
  align?: "right" | "center";
  className?: string;
}

interface Paging {
  page: number;
  pageSize: number;
  total: number;
  onPage: (page: number) => void;
}

interface DataTableProps<T> {
  columns: ColumnDef<T>[];
  data: T[] | undefined;
  loading?: boolean;
  error?: unknown;
  onRetry?: () => void;
  onRowClick?: (row: T) => void;
  rowKey: (row: T) => string;
  paging?: Paging;
  empty?: { title: string; description?: ReactNode; action?: ReactNode };
  /** Lists longer than this are virtualised (only visible rows are drawn). */
  virtualizeAbove?: number;
  /** Height of the scroll area when virtualised. */
  height?: number;
  className?: string;
}

const ROW_HEIGHT = 44;

function alignClass(meta: unknown): string {
  const m = meta as ColumnMeta | undefined;
  return cn(
    m?.align === "right" && "text-right tabular-nums",
    m?.align === "center" && "text-center",
    m?.className,
  );
}

/**
 * The list view used by every screen. Rows are buttons for the keyboard: ↑/↓ move, Enter opens.
 * Sorting is per page (the server sorts and pages the full list).
 */
export function DataTable<T>({
  columns,
  data,
  loading,
  error,
  onRetry,
  onRowClick,
  rowKey,
  paging,
  empty,
  virtualizeAbove = 120,
  height = 560,
  className,
}: DataTableProps<T>) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const table = useReactTable({
    data: data ?? [],
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getRowId: (row) => rowKey(row),
  });
  const scrollRef = useRef<HTMLDivElement>(null);
  const rows = table.getRowModel().rows;
  const virtual = rows.length > virtualizeAbove;
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    enabled: virtual,
  });

  if (error && !data) return <ErrorState error={error} {...(onRetry ? { onRetry } : {})} />;
  if (loading && !data) return <LoadingRows />;
  if (data && data.length === 0 && empty) return <EmptyState {...empty} />;

  const focusRow = (index: number) => {
    const target = scrollRef.current?.querySelector<HTMLElement>(`[data-row-index="${index}"]`);
    if (target) target.focus();
    else if (virtual) virtualizer.scrollToIndex(index);
  };

  const items = virtual
    ? virtualizer
        .getVirtualItems()
        .map((v) => ({ row: rows[v.index], index: v.index, start: v.start }))
    : rows.map((row, index) => ({ row, index, start: 0 }));
  const padTop = virtual ? (items[0]?.start ?? 0) : 0;
  const padBottom = virtual
    ? virtualizer.getTotalSize() - ((items.at(-1)?.start ?? 0) + ROW_HEIGHT)
    : 0;

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <div
        ref={scrollRef}
        className="overflow-auto rounded-2xl border border-border/60 bg-card"
        style={virtual ? { height } : undefined}
      >
        <table className="w-full caption-bottom text-sm">
          <thead className="sticky top-0 z-10 bg-card">
            {table.getHeaderGroups().map((group) => (
              <tr key={group.id} className="border-b border-border/60">
                {group.headers.map((header) => {
                  const sortable = header.column.getCanSort();
                  return (
                    <th
                      key={header.id}
                      scope="col"
                      className={cn(
                        "h-10 px-3 text-left align-middle text-xs font-medium whitespace-nowrap text-muted-foreground",
                        alignClass(header.column.columnDef.meta),
                      )}
                      aria-sort={
                        header.column.getIsSorted() === "asc"
                          ? "ascending"
                          : header.column.getIsSorted() === "desc"
                            ? "descending"
                            : undefined
                      }
                    >
                      {header.isPlaceholder ? null : sortable ? (
                        <button
                          type="button"
                          className="inline-flex items-center gap-1 hover:text-foreground"
                          onClick={header.column.getToggleSortingHandler()}
                        >
                          {flexRender(header.column.columnDef.header, header.getContext())}
                          <ArrowUpDown className="size-3 opacity-60" aria-hidden />
                        </button>
                      ) : (
                        flexRender(header.column.columnDef.header, header.getContext())
                      )}
                    </th>
                  );
                })}
              </tr>
            ))}
          </thead>
          <tbody>
            {padTop > 0 && (
              <tr style={{ height: padTop }} aria-hidden>
                <td colSpan={columns.length} />
              </tr>
            )}
            {items.map(({ row, index }) =>
              row ? (
                <tr
                  key={row.id}
                  data-row-index={index}
                  tabIndex={onRowClick ? 0 : -1}
                  onClick={onRowClick ? () => onRowClick(row.original) : undefined}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && onRowClick) onRowClick(row.original);
                    else if (e.key === "ArrowDown") {
                      e.preventDefault();
                      focusRow(Math.min(index + 1, rows.length - 1));
                    } else if (e.key === "ArrowUp") {
                      e.preventDefault();
                      focusRow(Math.max(index - 1, 0));
                    }
                  }}
                  className={cn(
                    "border-b border-border/40 transition-colors last:border-0",
                    onRowClick &&
                      "cursor-pointer hover:bg-muted/50 focus-visible:bg-muted focus-visible:outline-none",
                  )}
                  style={{ height: ROW_HEIGHT }}
                >
                  {row.getVisibleCells().map((cell) => (
                    <td
                      key={cell.id}
                      className={cn(
                        "px-3 align-middle whitespace-nowrap",
                        alignClass(cell.column.columnDef.meta),
                      )}
                    >
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </td>
                  ))}
                </tr>
              ) : null,
            )}
            {padBottom > 0 && (
              <tr style={{ height: padBottom }} aria-hidden>
                <td colSpan={columns.length} />
              </tr>
            )}
          </tbody>
        </table>
      </div>
      {paging && <Pager {...paging} />}
    </div>
  );
}

export function Pager({ page, pageSize, total, onPage }: Paging) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  if (total <= pageSize && page === 1) {
    return <p className="px-1 text-xs text-muted-foreground">{total} in total</p>;
  }
  const first = (page - 1) * pageSize + 1;
  const last = Math.min(page * pageSize, total);
  return (
    <div className="flex items-center justify-between px-1 text-xs text-muted-foreground">
      <span>
        {first}–{last} of {total}
      </span>
      <div className="flex items-center gap-1">
        <Button
          variant="outline"
          size="icon-sm"
          aria-label="Previous page"
          disabled={page <= 1}
          onClick={() => onPage(page - 1)}
        >
          <ChevronLeft />
        </Button>
        <span className="px-2">
          Page {page} of {pages}
        </span>
        <Button
          variant="outline"
          size="icon-sm"
          aria-label="Next page"
          disabled={page >= pages}
          onClick={() => onPage(page + 1)}
        >
          <ChevronRight />
        </Button>
      </div>
    </div>
  );
}

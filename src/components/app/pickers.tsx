import { forwardRef, useEffect, useId, useImperativeHandle, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { Api } from "@inventory/shared";
import { Search } from "lucide-react";
import { Input } from "@/components/ui/input";
import { useMoney } from "@/lib/business";
import { showQty } from "@/lib/format";
import { useCommand } from "@/lib/queries";
import { cn } from "@/lib/utils";

/** Waits until typing pauses (search as you type without a query per key). */
export function useDebounced<T>(value: T, ms = 150): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

export interface PickerHandle {
  focus(): void;
  clear(): void;
}

interface SearchSelectProps<T> {
  items: T[] | undefined;
  loading: boolean;
  term: string;
  onTermChange: (term: string) => void;
  onSelect: (item: T) => void;
  /** Enter with no highlighted item (for example a scanned barcode). */
  onSubmitTerm?: (term: string) => void;
  itemKey: (item: T) => string;
  renderItem: (item: T) => ReactNode;
  placeholder: string;
  autoFocus?: boolean;
  className?: string;
  ariaLabel: string;
  emptyText?: string;
}

/** Text field with a keyboard-driven result list (↑/↓, Enter, Esc). */
function SearchSelectInner<T>(
  {
    items,
    loading,
    term,
    onTermChange,
    onSelect,
    onSubmitTerm,
    itemKey,
    renderItem,
    placeholder,
    autoFocus,
    className,
    ariaLabel,
    emptyText = "Nothing found",
  }: SearchSelectProps<T>,
  ref: React.ForwardedRef<PickerHandle>,
) {
  const input = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const listId = useId();
  useImperativeHandle(ref, () => ({
    focus: () => input.current?.focus(),
    clear: () => onTermChange(""),
  }));
  useEffect(() => setActive(0), [items]);
  const list = items ?? [];
  const show = open && term.trim().length > 0;
  const choose = (item: T) => {
    onSelect(item);
    setOpen(false);
  };
  return (
    <div className={cn("relative", className)}>
      <Search
        className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
        aria-hidden
      />
      <Input
        ref={input}
        role="combobox"
        aria-label={ariaLabel}
        aria-expanded={show}
        aria-controls={listId}
        aria-autocomplete="list"
        autoComplete="off"
        autoFocus={autoFocus}
        className="pl-9"
        placeholder={placeholder}
        value={term}
        onChange={(e) => {
          onTermChange(e.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 120)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setOpen(true);
            setActive((a) => Math.min(a + 1, Math.max(list.length - 1, 0)));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((a) => Math.max(a - 1, 0));
          } else if (e.key === "Enter") {
            e.preventDefault();
            const item = show ? list[active] : undefined;
            if (item) choose(item);
            else if (term.trim()) onSubmitTerm?.(term.trim());
          } else if (e.key === "Escape") {
            setOpen(false);
          }
        }}
      />
      {show && (
        <ul
          id={listId}
          role="listbox"
          className="absolute z-50 mt-1 max-h-80 w-full overflow-auto rounded-xl border border-border bg-popover p-1 shadow-lg"
        >
          {list.length === 0 ? (
            <li className="px-3 py-2 text-sm text-muted-foreground">
              {loading ? "Searching…" : emptyText}
            </li>
          ) : (
            list.map((item, index) => (
              <li
                key={itemKey(item)}
                role="option"
                aria-selected={index === active}
                className={cn(
                  "cursor-pointer rounded-lg px-3 py-2 text-sm",
                  index === active && "bg-muted",
                )}
                onMouseEnter={() => setActive(index)}
                onMouseDown={(e) => {
                  e.preventDefault();
                  choose(item);
                }}
              >
                {renderItem(item)}
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}

export const SearchSelect = forwardRef(SearchSelectInner) as <T>(
  props: SearchSelectProps<T> & { ref?: React.Ref<PickerHandle> },
) => ReturnType<typeof SearchSelectInner>;

// ---------------------------------------------------------------------------------------------

interface ProductPickerProps {
  onSelect: (item: Api.SearchItem) => void;
  /** Enter on an unmatched term (a barcode that wasn't found). */
  onNotFound?: (term: string) => void;
  warehouseId?: string | null;
  autoFocus?: boolean;
  placeholder?: string;
  className?: string;
}

/** Item search by name, SKU or barcode (scanner-friendly: Enter picks the exact match). */
export const ProductPicker = forwardRef<PickerHandle, ProductPickerProps>(function ProductPicker(
  { onSelect, onNotFound, warehouseId, autoFocus, placeholder, className },
  ref,
) {
  const [term, setTerm] = useState("");
  const debounced = useDebounced(term.trim(), 120);
  const money = useMoney();
  const query = useCommand(
    "products_search",
    { term: debounced, limit: 20, warehouseId: warehouseId ?? null },
    { enabled: debounced.length > 0, placeholderData: undefined },
  );
  return (
    <SearchSelect<Api.SearchItem>
      ref={ref}
      ariaLabel="Find an item"
      items={debounced === term.trim() ? query.data : undefined}
      loading={query.isFetching || debounced !== term.trim()}
      term={term}
      onTermChange={setTerm}
      onSelect={(item) => {
        onSelect(item);
        setTerm("");
      }}
      onSubmitTerm={(t) => {
        // A scanner types fast and presses Enter: the first exact hit wins.
        const hit = query.data?.[0];
        if (
          hit &&
          debounced === t &&
          (hit.barcode === t || hit.sku.toLowerCase() === t.toLowerCase())
        ) {
          onSelect(hit);
          setTerm("");
        } else {
          onNotFound?.(t);
        }
      }}
      itemKey={(i) => `${i.variantId}:${i.unitId}`}
      placeholder={placeholder ?? "Search item, SKU or scan barcode"}
      autoFocus={autoFocus}
      className={className}
      renderItem={(i) => (
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <p className="truncate font-medium">
              {i.name}
              {i.factorNum !== 1 || i.factorDen !== 1 ? ` (${i.unitCode})` : ""}
            </p>
            <p className="text-xs text-muted-foreground">
              {i.sku}
              {i.barcode ? ` · ${i.barcode}` : ""}
              {!i.isService && ` · stock ${showQty(i.stock)}`}
            </p>
          </div>
          <span className="shrink-0 tabular-nums">{money(i.salePrice)}</span>
        </div>
      )}
    />
  );
});

interface PartyPickerProps {
  partyType: "customer" | "supplier";
  onSelect: (party: Api.PartyListItem) => void;
  autoFocus?: boolean;
  placeholder?: string;
  className?: string;
}

export const PartyPicker = forwardRef<PickerHandle, PartyPickerProps>(function PartyPicker(
  { partyType, onSelect, autoFocus, placeholder, className },
  ref,
) {
  const [term, setTerm] = useState("");
  const debounced = useDebounced(term.trim(), 150);
  const money = useMoney();
  const query = useCommand(
    "parties_list",
    { search: debounced, partyType, page: 1, pageSize: 15 },
    { enabled: debounced.length > 0, placeholderData: undefined },
  );
  return (
    <SearchSelect<Api.PartyListItem>
      ref={ref}
      ariaLabel={partyType === "customer" ? "Find a customer" : "Find a supplier"}
      items={debounced === term.trim() ? query.data?.items : undefined}
      loading={query.isFetching}
      term={term}
      onTermChange={setTerm}
      onSelect={(p) => {
        onSelect(p);
        setTerm("");
      }}
      itemKey={(p) => p.id}
      placeholder={
        placeholder ??
        (partyType === "customer" ? "Customer name or phone" : "Supplier name or phone")
      }
      autoFocus={autoFocus}
      className={className}
      renderItem={(p) => (
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <p className="truncate font-medium">{p.name}</p>
            <p className="text-xs text-muted-foreground">{p.phone ?? p.gstin ?? ""}</p>
          </div>
          {p.balance !== 0 && (
            <span className="shrink-0 text-xs tabular-nums">{money(p.balance)}</span>
          )}
        </div>
      )}
    />
  );
});

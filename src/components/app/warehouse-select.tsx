import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { useCommand } from "@/lib/queries";

/** Store chooser. Empty value = the main store. */
export function WarehouseSelect({
  id,
  value,
  onChange,
  label = "Store",
  allowDefault = true,
  emptyLabel,
  exclude,
}: {
  id?: string;
  value: string;
  onChange: (id: string) => void;
  label?: string;
  allowDefault?: boolean;
  /** Text of the empty choice (default: "Main store", or "Choose…" without a default). */
  emptyLabel?: string;
  exclude?: string;
}) {
  const stores = useCommand("stock_warehouses", {});
  return (
    <NativeSelect
      id={id}
      aria-label={label}
      className="w-full"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      {allowDefault && (
        <NativeSelectOption value="">{emptyLabel ?? "Main store"}</NativeSelectOption>
      )}
      {!allowDefault && <NativeSelectOption value="">{emptyLabel ?? "Choose…"}</NativeSelectOption>}
      {stores.data
        ?.filter((w) => w.id !== exclude && (!allowDefault || !w.isDefault))
        .map((w) => (
          <NativeSelectOption key={w.id} value={w.id}>
            {w.name}
          </NativeSelectOption>
        ))}
    </NativeSelect>
  );
}

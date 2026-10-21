import { SelectField } from "@/components/ui/select-field";
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
    <SelectField
      id={id}
      aria-label={label}
      className="w-full"
      value={value}
      onValueChange={onChange}
      options={[
        ...(allowDefault ? [{ value: "", label: emptyLabel ?? "Main store" }] : []),
        ...(!allowDefault ? [{ value: "", label: emptyLabel ?? "Choose…" }] : []),
        ...(stores.data ?? [])
          .filter((w) => w.id !== exclude && (!allowDefault || !w.isDefault))
          .map((w) => ({ value: w.id, label: w.name })),
      ]}
    />
  );
}

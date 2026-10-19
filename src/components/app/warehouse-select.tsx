import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { useCommand } from "@/lib/queries";

/** Store chooser. Empty value = the main store. */
export function WarehouseSelect({
  id,
  value,
  onChange,
  label = "Store",
  allowDefault = true,
  exclude,
}: {
  id?: string;
  value: string;
  onChange: (id: string) => void;
  label?: string;
  allowDefault?: boolean;
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
      {allowDefault && <NativeSelectOption value="">Main store</NativeSelectOption>}
      {!allowDefault && <NativeSelectOption value="">Choose…</NativeSelectOption>}
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

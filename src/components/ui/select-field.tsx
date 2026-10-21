import type { ReactNode } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

export interface SelectOption {
  value: string;
  label: ReactNode;
  disabled?: boolean;
}

/** The select primitive wants a real value for the empty choice ("All categories"). */
const EMPTY = "__empty__";

export interface SelectFieldProps {
  /** The chosen value, or "" for the empty choice. */
  value: string;
  onValueChange: (value: string) => void;
  options: SelectOption[];
  /** Shown while nothing is chosen and there is no option with the value "". */
  placeholder?: string;
  size?: "default" | "sm";
  id?: string;
  "aria-label"?: string;
  disabled?: boolean;
  invalid?: boolean;
  required?: boolean;
  className?: string;
}

/**
 * A dropdown over a list of options. It replaces the browser's own select: the same keyboard use
 * (Space or Enter opens, arrows move, type to jump, Enter picks, Esc closes), the same
 * `value` / `onValueChange` shape as the rest of the app, and "" as a valid empty choice.
 */
export function SelectField({
  value,
  onValueChange,
  options,
  placeholder,
  size,
  id,
  "aria-label": ariaLabel,
  disabled,
  invalid,
  required,
  className,
}: SelectFieldProps) {
  const toInner = (v: string) => (v === "" ? EMPTY : v);
  const items = options.map((o) => ({ value: toInner(o.value), label: o.label }));
  const hasEmpty = options.some((o) => o.value === "");
  const current = value === "" && !hasEmpty ? null : toInner(value);
  return (
    <Select
      value={current}
      onValueChange={(next) => onValueChange(next === null || next === EMPTY ? "" : next)}
      items={items}
      disabled={disabled}
      required={required}
    >
      <SelectTrigger
        id={id}
        aria-label={ariaLabel}
        aria-invalid={invalid || undefined}
        {...(size ? { size } : {})}
        className={cn("w-full min-w-28", className)}
      >
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent alignItemWithTrigger={false}>
        {options.map((o) => (
          <SelectItem key={toInner(o.value)} value={toInner(o.value)} disabled={o.disabled}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

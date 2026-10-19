import { forwardRef, useEffect, useState } from "react";
import type { ComponentProps } from "react";
import { Input } from "@/components/ui/input";
import { useLocale } from "@/lib/business";
import { moneyText, readMoney, readQty, readRate, showQty, showRate } from "@/lib/format";
import { cn } from "@/lib/utils";

type BaseProps = Omit<ComponentProps<typeof Input>, "value" | "onChange" | "type" | "onInvalid">;

interface NumberFieldProps extends BaseProps {
  value: number | null;
  onValueChange: (value: number | null) => void;
  /** Reported while typing; shown by the caller (for example in FieldError). */
  onParseError?: (message: string | null) => void;
}

/**
 * A text field for an exact number. The user types freely; the value is parsed without floats
 * and re-formatted on blur. Invalid text keeps the last good value and reports why.
 */
function useNumberText(
  value: number | null,
  format: (v: number) => string,
  parse: (text: string) => { value: number | null; error?: string },
  onValueChange: (v: number | null) => void,
  onParseError?: (message: string | null) => void,
) {
  const [text, setText] = useState(value === null ? "" : format(value));
  const [focused, setFocused] = useState(false);
  const [invalid, setInvalid] = useState(false);
  useEffect(() => {
    if (!focused) setText(value === null ? "" : format(value));
  }, [value, focused, format]);
  return {
    text,
    invalid,
    onChange: (next: string) => {
      setText(next);
      const parsed = parse(next);
      setInvalid(Boolean(parsed.error));
      onParseError?.(parsed.error ?? null);
      if (!parsed.error) onValueChange(parsed.value);
    },
    onFocus: () => {
      setFocused(true);
    },
    onBlur: () => {
      setFocused(false);
    },
  };
}

export const MoneyInput = forwardRef<HTMLInputElement, NumberFieldProps>(function MoneyInput(
  { value, onValueChange, onParseError, className, ...props },
  ref,
) {
  const locale = useLocale();
  const field = useNumberText(
    value,
    (v) => moneyText(v, locale),
    (t) => readMoney(t, locale),
    onValueChange,
    onParseError,
  );
  return (
    <Input
      ref={ref}
      inputMode="decimal"
      autoComplete="off"
      className={cn("text-right tabular-nums", className)}
      aria-invalid={field.invalid || undefined}
      value={field.text}
      onChange={(e) => field.onChange(e.target.value)}
      onFocus={(e) => {
        field.onFocus();
        e.target.select();
      }}
      onBlur={field.onBlur}
      {...props}
    />
  );
});

export const QtyInput = forwardRef<HTMLInputElement, NumberFieldProps & { maxDecimals?: number }>(
  function QtyInput(
    { value, onValueChange, onParseError, maxDecimals = 3, className, ...props },
    ref,
  ) {
    const field = useNumberText(
      value,
      showQty,
      (t) => readQty(t, maxDecimals),
      onValueChange,
      onParseError,
    );
    return (
      <Input
        ref={ref}
        inputMode="decimal"
        autoComplete="off"
        className={cn("text-right tabular-nums", className)}
        aria-invalid={field.invalid || undefined}
        value={field.text}
        onChange={(e) => field.onChange(e.target.value)}
        onFocus={(e) => {
          field.onFocus();
          e.target.select();
        }}
        onBlur={field.onBlur}
        {...props}
      />
    );
  },
);

const rateText = (v: number) => showRate(v).replace("%", "");

/** Percent field; the value is parts per million (18% = 180000). */
export const RateInput = forwardRef<HTMLInputElement, NumberFieldProps>(function RateInput(
  { value, onValueChange, onParseError, className, ...props },
  ref,
) {
  const field = useNumberText(value, rateText, readRate, onValueChange, onParseError);
  return (
    <div className="relative">
      <Input
        ref={ref}
        inputMode="decimal"
        autoComplete="off"
        className={cn("pr-7 text-right tabular-nums", className)}
        aria-invalid={field.invalid || undefined}
        value={field.text}
        onChange={(e) => field.onChange(e.target.value)}
        onFocus={(e) => {
          field.onFocus();
          e.target.select();
        }}
        onBlur={field.onBlur}
        {...props}
      />
      <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-sm text-muted-foreground">
        %
      </span>
    </div>
  );
});

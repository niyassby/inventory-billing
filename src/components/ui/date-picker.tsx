import { useState } from "react";
import { enGB, enIN, enUS } from "date-fns/locale";
import type { Locale } from "date-fns";
import { CalendarDays, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Calendar } from "@/components/ui/calendar";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useLocale } from "@/lib/business";
import { isoInRange, parseIsoDate, toIsoDate } from "@/lib/dates";
import { showDate } from "@/lib/format";
import { cn } from "@/lib/utils";

const CALENDAR_LOCALES: Record<string, Locale> = { "en-IN": enIN, "en-GB": enGB, "en-US": enUS };

export interface DatePickerProps {
  /** `YYYY-MM-DD`, or "" for no date. */
  value: string;
  onChange: (value: string) => void;
  id?: string;
  "aria-label"?: string;
  placeholder?: string;
  /** Earliest and latest allowed day, `YYYY-MM-DD`. */
  min?: string;
  max?: string;
  disabled?: boolean;
  invalid?: boolean;
  /** Shows a Clear button (for optional dates such as a filter). */
  clearable?: boolean;
  className?: string;
}

/**
 * A date field: shows the day in the business locale ("20 Oct 2026"), opens a calendar below it.
 * Space or Enter opens it, the arrow keys move between days, Enter picks, Esc closes. The value is
 * always a `YYYY-MM-DD` string, so forms and URLs keep the format they had with a native input.
 */
export function DatePicker({
  value,
  onChange,
  id,
  "aria-label": ariaLabel,
  placeholder = "Pick a date",
  min,
  max,
  disabled,
  invalid,
  clearable,
  className,
}: DatePickerProps) {
  const [open, setOpen] = useState(false);
  const locale = useLocale();
  const selected = parseIsoDate(value);
  const pick = (iso: string) => {
    onChange(iso);
    setOpen(false);
  };
  const today = toIsoDate(new Date());
  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        // Put the keyboard on the chosen day (or today) so the arrow keys move it straight away.
        if (next) {
          window.setTimeout(() => {
            const first = (selector: string) =>
              document.querySelector<HTMLElement>(`[data-slot="calendar"] ${selector}`);
            (
              first('button[data-selected-single="true"]') ??
              first('td[data-today="true"] button') ??
              first("td:not([data-outside]) button:not([disabled])")
            )?.focus();
          }, 60);
        }
      }}
    >
      <PopoverTrigger
        render={
          <Button
            id={id}
            type="button"
            variant="outline"
            disabled={disabled}
            aria-label={ariaLabel}
            aria-invalid={invalid || undefined}
            className={cn(
              "w-full min-w-36 justify-start gap-2 px-3 font-normal",
              !selected && "text-muted-foreground",
              className,
            )}
          />
        }
      >
        <CalendarDays className="text-muted-foreground" aria-hidden />
        <span className="num">{selected ? showDate(value, locale.locale) : placeholder}</span>
      </PopoverTrigger>
      <PopoverContent className="w-auto p-0" align="start">
        <Calendar
          mode="single"
          selected={selected}
          defaultMonth={selected ?? new Date()}
          captionLayout="dropdown"
          startMonth={new Date(2000, 0)}
          endMonth={new Date(new Date().getFullYear() + 10, 11)}
          locale={CALENDAR_LOCALES[locale.locale] ?? enUS}
          disabled={(date) => !isoInRange(toIsoDate(date), min, max)}
          onSelect={(date) => {
            if (date) pick(toIsoDate(date));
          }}
        />
        <div className="flex items-center justify-between gap-2 border-t border-border px-3 py-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={!isoInRange(today, min, max)}
            onClick={() => pick(today)}
          >
            Today
          </Button>
          {clearable && value && (
            <Button type="button" variant="ghost" size="sm" onClick={() => pick("")}>
              <X aria-hidden /> Clear
            </Button>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}

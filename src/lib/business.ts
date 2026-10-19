import { useMemo } from "react";
import type { Api, Currency } from "@inventory/shared";
import { useCommand } from "./queries";
import type { Locale } from "./format";
import { DEFAULT_LOCALE, showMoney } from "./format";

/** Business name, currency, tax defaults: everything screens and receipts need. */
export function usePublicSettings() {
  return useCommand("settings_public", {}, { staleTime: 5 * 60 * 1000 });
}

export function localeOf(settings: Api.PublicSettings | undefined): Locale {
  if (!settings) return DEFAULT_LOCALE;
  const currency: Currency = {
    code: settings.business.currency ?? DEFAULT_LOCALE.currency.code,
    minorDigits: settings.business.currencyMinorDigits ?? DEFAULT_LOCALE.currency.minorDigits,
  };
  return { currency, locale: settings.business.locale || DEFAULT_LOCALE.locale };
}

export function useLocale(): Locale {
  const { data } = usePublicSettings();
  return useMemo(() => localeOf(data), [data]);
}

/** `money(12345)` → "₹123.45" in the business currency. */
export function useMoney(): (minor: number) => string {
  const locale = useLocale();
  return useMemo(() => (minor: number) => showMoney(minor, locale), [locale]);
}

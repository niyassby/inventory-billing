/** Why a till's cash differs from what the system expects (the backend's reason codes). */
export const DIFFERENCE_REASONS = [
  { value: "counting_error", label: "Counting error" },
  { value: "wrong_change", label: "Wrong change given" },
  { value: "float_error", label: "Opening float wrong" },
  { value: "unrecorded_sale", label: "Sale not recorded" },
  { value: "unrecorded_payout", label: "Payout not recorded" },
  { value: "loss_or_theft", label: "Loss or theft" },
  { value: "other", label: "Other" },
];

export function reasonLabel(code: string | null | undefined): string {
  return DIFFERENCE_REASONS.find((r) => r.value === code)?.label ?? "";
}

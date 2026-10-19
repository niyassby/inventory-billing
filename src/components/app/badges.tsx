import { Badge } from "@/components/ui/badge";
import { label } from "@/lib/format";

type Tone = "default" | "secondary" | "destructive" | "outline";

const TONES: Record<string, Tone> = {
  finalized: "secondary",
  paid: "secondary",
  open: "outline",
  accepted: "secondary",
  converted: "secondary",
  partial: "outline",
  unpaid: "destructive",
  voided: "destructive",
  void: "destructive",
  cancelled: "destructive",
  rejected: "destructive",
  expired: "outline",
  draft: "outline",
  counting: "outline",
  received: "secondary",
  partially_received: "outline",
};

/** Document or payment status as a small badge ("Paid", "Voided", …). */
export function StatusBadge({ status }: { status: string }) {
  return <Badge variant={TONES[status] ?? "outline"}>{label(status)}</Badge>;
}

export function OfflineBadge() {
  return (
    <Badge variant="outline" title="Made on a counter while the hub was offline">
      Offline sale
    </Badge>
  );
}

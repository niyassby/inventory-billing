import { describeConnection, type ConnectionTone } from "@/api/connection";
import { useConnection } from "@/hooks/use-connection";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

const DOT: Record<ConnectionTone, string> = {
  ok: "bg-success",
  busy: "bg-info animate-pulse",
  warning: "bg-warning",
  error: "bg-danger",
  neutral: "bg-muted-foreground/50",
};

/** Always-visible connection state in the top bar. */
export function ConnectionBadge() {
  const { data } = useConnection();
  if (!data) return null;
  const view = describeConnection(data);
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span
            role="status"
            aria-live="polite"
            className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-card px-2.5 text-xs font-medium text-foreground shadow-xs"
          />
        }
      >
        <span aria-hidden className={cn("size-2 rounded-full", DOT[view.tone])} />
        {view.label}
      </TooltipTrigger>
      <TooltipContent>{view.detail}</TooltipContent>
    </Tooltip>
  );
}

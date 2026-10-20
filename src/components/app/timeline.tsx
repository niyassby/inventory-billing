import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export type TimelineTone = "neutral" | "success" | "warning" | "danger" | "info";

export interface TimelineEvent {
  key: string;
  title: ReactNode;
  /** When it happened, already formatted. */
  when: string;
  detail?: ReactNode;
  tone?: TimelineTone;
}

const DOT: Record<TimelineTone, string> = {
  neutral: "bg-muted-foreground/50",
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-danger",
  info: "bg-info",
};

/** What happened to a document, oldest first: issued, paid, returned, voided. */
export function Timeline({ events }: { events: TimelineEvent[] }) {
  return (
    <ol className="relative flex flex-col gap-4 border-l border-border pl-5">
      {events.map((e) => (
        <li key={e.key} className="relative">
          <span
            aria-hidden
            className={cn(
              "absolute top-1.5 -left-[1.62rem] size-2.5 rounded-full ring-4 ring-card",
              DOT[e.tone ?? "neutral"],
            )}
          />
          <p className="text-sm font-medium">{e.title}</p>
          <p className="text-caption">{e.when}</p>
          {e.detail && <p className="mt-0.5 text-sm text-muted-foreground">{e.detail}</p>}
        </li>
      ))}
    </ol>
  );
}

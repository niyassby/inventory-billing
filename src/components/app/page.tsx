import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { ArrowLeft, Inbox, TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { errorText } from "@/lib/queries";
import { cn } from "@/lib/utils";

interface PageProps {
  title: string;
  description?: ReactNode;
  /** Buttons on the right of the title. */
  actions?: ReactNode;
  /** Back link (detail and editor pages). */
  back?: { to: string; label: string };
  /** Filters and search, directly under the title. */
  toolbar?: ReactNode;
  children: ReactNode;
  className?: string;
}

/** The frame of every screen: title, actions, content. */
export function Page({
  title,
  description,
  actions,
  back,
  toolbar,
  children,
  className,
}: PageProps) {
  return (
    <div
      className={cn(
        "mx-auto flex w-full max-w-[90rem] animate-fade-in flex-col gap-5 p-4 md:p-6",
        className,
      )}
    >
      {back && (
        <Link
          to={back.to}
          className="inline-flex w-fit items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="size-4" aria-hidden />
          {back.label}
        </Link>
      )}
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="font-heading text-page-title">{title}</h1>
          {description && <p className="mt-0.5 text-sm text-muted-foreground">{description}</p>}
        </div>
        {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
      {toolbar && <div className="flex flex-wrap items-center gap-2">{toolbar}</div>}
      {children}
    </div>
  );
}

/** A titled block inside a page. */
export function Section({
  title,
  actions,
  children,
  className,
}: {
  title?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("rounded-xl border border-border bg-card p-4 shadow-xs", className)}>
      {(title ?? actions) && (
        <div className="mb-3 flex items-center justify-between gap-2">
          {title && <h2 className="font-heading text-section-title">{title}</h2>}
          {actions}
        </div>
      )}
      {children}
    </section>
  );
}

export function EmptyState({
  title,
  description,
  action,
}: {
  title: string;
  description?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border bg-card/50 px-6 py-12 text-center">
      <span className="flex size-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
        <Inbox className="size-5" strokeWidth={1.5} aria-hidden />
      </span>
      <p className="text-section-title">{title}</p>
      {description && <p className="max-w-md text-sm text-muted-foreground">{description}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function ErrorState({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
  return (
    <div
      role="alert"
      className="flex flex-col items-center justify-center gap-2 rounded-xl border border-danger/25 bg-danger-soft px-6 py-10 text-center"
    >
      <TriangleAlert className="size-6 text-danger" strokeWidth={1.5} aria-hidden />
      <p className="text-sm">{errorText(error)}</p>
      {onRetry && (
        <Button variant="outline" size="sm" onClick={onRetry}>
          Try again
        </Button>
      )}
    </div>
  );
}

export function LoadingRows({ rows = 6 }: { rows?: number }) {
  return (
    <div className="flex flex-col gap-2" role="status" aria-label="Loading">
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-9 w-full" />
      ))}
    </div>
  );
}

interface QueryLike<T> {
  data: T | undefined;
  error: unknown;
  isPending: boolean;
  refetch: () => unknown;
}

/** Loading → error → content, the same way everywhere. */
export function QueryView<T>({
  query,
  children,
  loading,
}: {
  query: QueryLike<T>;
  children: (data: T) => ReactNode;
  loading?: ReactNode;
}) {
  if (query.data !== undefined) return <>{children(query.data)}</>;
  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
  return <>{loading ?? <LoadingRows />}</>;
}

/** Label/value pairs for document headers. */
export function Facts({ items }: { items: [string, ReactNode][] }) {
  return (
    <dl className="grid grid-cols-2 gap-x-6 gap-y-3 text-sm md:grid-cols-4">
      {items.map(([k, v]) => (
        <div key={k} className="min-w-0">
          <dt className="text-xs text-muted-foreground">{k}</dt>
          <dd className="truncate font-medium">{v ?? "—"}</dd>
        </div>
      ))}
    </dl>
  );
}

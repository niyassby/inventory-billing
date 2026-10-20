import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { CircleCheck, Info, TriangleAlert, CircleX } from "lucide-react";

import { cn } from "@/lib/utils";

/** An inline notice: offline states, validation summaries, "why is this disabled". */
const alertVariants = cva(
  "relative flex w-full items-start gap-3 rounded-lg border px-3.5 py-3 text-sm [&>svg]:mt-0.5 [&>svg]:size-4 [&>svg]:shrink-0",
  {
    variants: {
      variant: {
        info: "border-info/25 bg-info-soft text-info",
        success: "border-success/25 bg-success-soft text-success",
        warning: "border-warning/30 bg-warning-soft text-warning",
        danger: "border-danger/25 bg-danger-soft text-danger",
        neutral: "border-border bg-muted text-foreground",
      },
    },
    defaultVariants: { variant: "info" },
  },
);

const ICONS = {
  info: Info,
  success: CircleCheck,
  warning: TriangleAlert,
  danger: CircleX,
  neutral: Info,
} as const;

function Alert({
  className,
  variant = "info",
  children,
  ...props
}: React.ComponentProps<"div"> & VariantProps<typeof alertVariants>) {
  const Icon = ICONS[variant ?? "info"];
  return (
    <div
      data-slot="alert"
      role={variant === "danger" || variant === "warning" ? "alert" : "status"}
      className={cn(alertVariants({ variant }), className)}
      {...props}
    >
      <Icon aria-hidden />
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

function AlertTitle({ className, ...props }: React.ComponentProps<"p">) {
  return <p data-slot="alert-title" className={cn("font-medium", className)} {...props} />;
}

function AlertDescription({ className, ...props }: React.ComponentProps<"p">) {
  return (
    <p
      data-slot="alert-description"
      className={cn("mt-0.5 text-foreground/80", className)}
      {...props}
    />
  );
}

export { Alert, AlertTitle, AlertDescription };

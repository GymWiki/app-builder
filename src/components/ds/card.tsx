import * as React from "react";
import { cn } from "@/lib/utils";

/** Kaart: rounded-xl, subtiele rand, zachte schaduw. `interactive` voegt hover/focus-staten toe. */
export function Card({ className, interactive, ...props }: React.ComponentProps<"div"> & { interactive?: boolean }) {
  return (
    <div
      className={cn(
        "bg-card text-card-foreground shadow-soft rounded-xl border",
        interactive &&
          "hover:shadow-card hover:border-primary/40 transition-[box-shadow,border-color,transform] duration-[var(--dur)] hover:-translate-y-px",
        className,
      )}
      {...props}
    />
  );
}

/** Paneel: een vlak binnen de werkruimte, met optionele kop. */
export function Panel({ className, ...props }: React.ComponentProps<"section">) {
  return <section className={cn("bg-card flex min-h-0 flex-col overflow-hidden", className)} {...props} />;
}

export function PanelHeader({
  title,
  actions,
  className,
}: {
  title: React.ReactNode;
  actions?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex h-11 shrink-0 items-center justify-between gap-2 border-b px-4", className)}>
      <h2 className="t-small text-muted-foreground font-semibold tracking-wide uppercase">{title}</h2>
      {actions}
    </div>
  );
}

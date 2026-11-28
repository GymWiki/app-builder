import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/** Lege toestand: icoon, korte uitleg en één duidelijke primaire actie. */
export function EmptyState({
  icon: Icon,
  title,
  body,
  action,
  className,
}: {
  icon: LucideIcon;
  title: string;
  body?: string;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("animate-in-up flex flex-col items-center px-6 py-16 text-center", className)}>
      <div className="relative mb-6">
        <div aria-hidden className="bg-accent absolute -inset-3 rounded-[28px] opacity-60" />
        <div className="bg-card text-primary-text shadow-card relative grid size-16 place-items-center rounded-2xl border">
          <Icon className="size-7" strokeWidth={1.75} />
        </div>
      </div>
      <h2 className="t-title">{title}</h2>
      {body && <p className="text-muted-foreground t-body mt-2 max-w-md">{body}</p>}
      {action && <div className="mt-6">{action}</div>}
    </div>
  );
}

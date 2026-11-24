import { Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

export function Spinner({ className, label }: { className?: string; label?: string }) {
  return (
    <span role="status" className="inline-flex">
      <Loader2 aria-hidden className={cn("text-muted-foreground size-4 animate-spin", className)} />
      {label && <span className="sr-only">{label}</span>}
    </span>
  );
}

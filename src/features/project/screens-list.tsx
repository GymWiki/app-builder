import { useQuery } from "@tanstack/react-query";
import { LayoutGrid, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/ipc";
import type { Screen } from "@/lib/types";
import { cn } from "@/lib/utils";

/** Alle schermen van de app; kies er één om er direct in de chat over te praten. */
export function ScreensList({
  projectId,
  selected,
  onSelect,
}: {
  projectId: string;
  selected: string | null;
  onSelect: (s: Screen | null) => void;
}) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["screens", projectId], queryFn: () => api.projectScreens(projectId) });
  const screens = q.data ?? [];
  return (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <p className="t-small text-muted-foreground">{t("workspace.screens.hint")}</p>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("workspace.screens.refresh")}
          title={t("workspace.screens.refresh")}
          onClick={() => void q.refetch()}
        >
          <RefreshCw className={cn(q.isFetching && "animate-spin")} />
        </Button>
      </div>
      {screens.length === 0 && !q.isPending ? (
        <div className="bg-muted/50 rounded-xl border border-dashed p-4 text-center">
          <LayoutGrid aria-hidden className="text-muted-foreground mx-auto mb-2 size-5" />
          <p className="t-body font-medium">{t("workspace.screens.empty")}</p>
          <p className="t-small text-muted-foreground mt-1">{t("workspace.screens.emptyHint")}</p>
        </div>
      ) : (
        <ul className="space-y-1.5">
          {screens.map((s) => {
            const on = selected === s.class;
            return (
              <li key={`${s.file}:${s.class}`}>
                <button
                  type="button"
                  aria-pressed={on}
                  onClick={() => onSelect(on ? null : s)}
                  className={cn(
                    "flex w-full items-center gap-3 rounded-xl border px-3 py-2 text-left transition-colors duration-[var(--dur)]",
                    on ? "border-primary bg-accent/60" : "hover:bg-muted/60",
                  )}
                >
                  <LayoutGrid
                    aria-hidden
                    className={cn("size-4 shrink-0", on ? "text-primary-text" : "text-muted-foreground")}
                  />
                  <span className="min-w-0 flex-1">
                    <span className="t-body block truncate font-medium">{s.class}</span>
                    <span className="t-small text-muted-foreground block truncate font-mono">{s.file}</span>
                  </span>
                  {on && (
                    <span className="t-small text-primary-text font-medium">{t("workspace.screens.selected")}</span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

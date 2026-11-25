import { useQuery } from "@tanstack/react-query";
import { Home, LayoutGrid, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { AskButton } from "@/features/sections/shared";
import { api } from "@/lib/ipc";
import type { Screen, ScreenGraph } from "@/lib/types";
import { cn } from "@/lib/utils";

interface Row {
  class: string;
  file: string;
  line: number;
  depth: number;
}

/** Zet de graaf om naar een ingesprongen boom vanaf het beginscherm (elk scherm één keer). */
export function treeRows(g: ScreenGraph): { rows: Row[]; unlinked: Row[] } {
  const by = new Map(g.nodes.map((n) => [n.class, n]));
  const seen = new Set<string>();
  const rows: Row[] = [];
  const visit = (name: string, depth: number) => {
    const n = by.get(name);
    if (!n || seen.has(name)) return;
    seen.add(name);
    rows.push({ class: n.class, file: n.file, line: n.line, depth });
    n.children.forEach((c) => visit(c, depth + 1));
  };
  if (g.home) visit(g.home, 0);
  const unlinked = g.nodes
    .filter((n) => !seen.has(n.class))
    .map((n) => ({ class: n.class, file: n.file, line: n.line, depth: 0 }));
  return { rows, unlinked };
}

/** Boom van schermen en navigatie; kies er één om er direct in de chat over te praten. */
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
  const q = useQuery({ queryKey: ["screens", projectId], queryFn: () => api.screensGraph(projectId) });
  const { rows, unlinked } = q.data ? treeRows(q.data) : { rows: [], unlinked: [] };
  const empty = !q.isPending && rows.length + unlinked.length === 0;

  const item = (r: Row, home: boolean) => {
    const on = selected === r.class;
    return (
      <li key={r.class} style={{ paddingLeft: r.depth * 16 }}>
        <button
          type="button"
          aria-pressed={on}
          onClick={() => onSelect(on ? null : { class: r.class, file: r.file, line: r.line })}
          className={cn(
            "flex w-full items-center gap-2.5 rounded-xl border px-3 py-2 text-left transition-colors duration-[var(--dur)]",
            on ? "border-primary bg-accent/60" : "hover:bg-muted/60",
          )}
        >
          {home ? (
            <Home aria-hidden className="text-primary-text size-4 shrink-0" />
          ) : (
            <LayoutGrid
              aria-hidden
              className={cn("size-4 shrink-0", on ? "text-primary-text" : "text-muted-foreground")}
            />
          )}
          <span className="min-w-0 flex-1">
            <span className="t-body block truncate font-medium">{r.class}</span>
            <span className="t-small text-muted-foreground block truncate font-mono">{r.file}</span>
          </span>
          {on && <span className="t-small text-primary-text font-medium">{t("sec.screens.selected")}</span>}
        </button>
      </li>
    );
  };

  return (
    <div>
      <div className="mb-2 flex items-center justify-between gap-2">
        <p className="t-small text-muted-foreground">{t("sec.screens.hint")}</p>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("sec.screens.refresh")}
          title={t("sec.screens.refresh")}
          onClick={() => void q.refetch()}
        >
          <RefreshCw className={cn(q.isFetching && "animate-spin")} />
        </Button>
      </div>
      {empty ? (
        <div className="bg-muted/50 rounded-xl border border-dashed p-4 text-center">
          <LayoutGrid aria-hidden className="text-muted-foreground mx-auto mb-2 size-5" />
          <p className="t-body font-medium">{t("sec.screens.none")}</p>
          <p className="t-small text-muted-foreground mt-1">{t("sec.screens.noneHint")}</p>
        </div>
      ) : (
        <>
          <p className="t-small text-muted-foreground mb-1.5 font-semibold tracking-wide uppercase">
            {t("sec.screens.home")}
          </p>
          <ul className="space-y-1.5">{rows.map((r, i) => item(r, i === 0))}</ul>
          {unlinked.length > 0 && (
            <>
              <p className="t-small text-muted-foreground mt-4 mb-1.5 font-semibold tracking-wide uppercase">
                {t("sec.screens.unlinked")}
              </p>
              <ul className="space-y-1.5">{unlinked.map((r) => item(r, false))}</ul>
            </>
          )}
        </>
      )}
      <div className="mt-4 flex flex-wrap gap-2">
        <AskButton
          projectId={projectId}
          prompt={t("sec.screens.addPrompt")}
          label={t("sec.screens.addScreen")}
          variant="outline"
        />
        {rows.length + unlinked.length > 1 && (
          <AskButton
            projectId={projectId}
            prompt={t("sec.screens.linkPrompt")}
            label={t("sec.screens.linkScreens")}
            variant="outline"
          />
        )}
      </div>
    </div>
  );
}

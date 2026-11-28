import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/ipc";
import { cn } from "@/lib/utils";

/** Overzicht van alle pagina's in het bereik als raster met miniaturen (uit de opgeslagen schermafbeeldingen). Een klik kiest die ene pagina. */
export function PagesGrid({
  projectId,
  pages,
  changed,
  working,
  refreshKey,
  onOpen,
}: {
  projectId: string;
  pages: string[];
  /** Pagina's die de laatste opdracht heeft aangepast (subtiele markering). */
  changed: string[];
  /** Pagina die nu wordt aangepast. */
  working: string | null;
  refreshKey: number;
  onOpen: (page: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="h-full overflow-y-auto p-4" data-testid="pages-grid">
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-4" aria-label={t("scope.gridLabel")}>
        {pages.map((p) => (
          <Thumb key={p} projectId={projectId} page={p} changed={changed.includes(p)} working={working === p} refreshKey={refreshKey} onOpen={onOpen} />
        ))}
      </ul>
    </div>
  );
}

function Thumb({
  projectId,
  page,
  changed,
  working,
  refreshKey,
  onOpen,
}: {
  projectId: string;
  page: string;
  changed: boolean;
  working: boolean;
  refreshKey: number;
  onOpen: (page: string) => void;
}) {
  const { t } = useTranslation();
  const shot = useQuery({
    queryKey: ["page-shot", projectId, page, refreshKey],
    queryFn: () => api.previewShotGet(projectId, page).catch(() => null),
    staleTime: 10_000,
  });
  return (
    <li>
      <button
        type="button"
        onClick={() => onOpen(page)}
        className={cn(
          "group focus-visible:ring-ring/60 w-full rounded-xl text-left outline-none focus-visible:ring-2",
          changed && "ring-primary/60 ring-2",
        )}
      >
        <div className="bg-muted relative aspect-[9/16] w-full overflow-hidden rounded-xl border">
          {shot.data ? (
            <img src={shot.data} alt="" className="size-full object-cover" draggable={false} />
          ) : (
            <div className="grid size-full animate-pulse place-items-center" aria-hidden>
              <span className="bg-background/60 h-2/3 w-2/3 rounded-lg" />
            </div>
          )}
          {working && <Loader2 aria-label={t("scope.working")} className="absolute top-2 right-2 size-4 animate-spin" />}
          {changed && !working && (
            <span className="bg-primary text-primary-foreground t-small absolute bottom-2 left-2 rounded-full px-2 py-0.5">
              {t("scope.changed")}
            </span>
          )}
        </div>
        <span className="t-small mt-1.5 block truncate font-medium">{page}</span>
      </button>
    </li>
  );
}

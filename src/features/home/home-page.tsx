import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, Plus, Search, SearchX, Sparkles, Wrench } from "lucide-react";
import { useDeferredValue, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { EmptyState } from "@/components/ds/empty-state";
import { PageHeader } from "@/components/ds/page-header";
import { AppTopBar } from "@/components/layout/top-bar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { qk, useSettings } from "@/hooks";
import { friendlyError } from "@/lib/errors";
import { api } from "@/lib/ipc";
import type { Project } from "@/lib/types";
import { NewProjectDialog } from "./new-project-dialog";
import { ProjectCard, ProjectCardSkeleton } from "./project-card";
import { RemoveProjectDialog } from "./remove-project-dialog";

export function HomePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const settings = useSettings();
  const [search, setSearch] = useState("");
  const query = useDeferredValue(search.trim());
  const [creating, setCreating] = useState(false);
  const [dialogKey, setDialogKey] = useState(0);
  const startCreating = () => {
    setDialogKey((k) => k + 1);
    setCreating(true);
  };
  const [removing, setRemoving] = useState<Project | null>(null);

  const projects = useQuery({
    queryKey: qk.projects(query),
    queryFn: () => api.projectsList(query),
    placeholderData: (prev) => prev,
  });
  const total = useQuery({ queryKey: qk.projects(""), queryFn: () => api.projectsList(""), staleTime: 5_000 });
  const hasAny = (total.data?.length ?? 0) > 0;

  const newButton = (
    <Button onClick={startCreating} className="rounded-lg">
      <Plus /> {t("home.newProject")}
    </Button>
  );

  return (
    <div className="flex h-full flex-col">
      <AppTopBar />
      <main className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-6xl px-6 py-10">
          {settings.data?.setup_completed === "skipped" && (
            <div role="status" className="bg-accent/60 mb-8 flex flex-wrap items-center gap-4 rounded-xl border p-4">
              <Wrench aria-hidden className="text-primary-text size-5 shrink-0" />
              <div className="min-w-0 flex-1">
                <p className="t-body font-medium">{t("home.setupBanner.title")}</p>
                <p className="text-muted-foreground t-small">{t("home.setupBanner.body")}</p>
              </div>
              <Button size="sm" onClick={() => navigate("/setup")}>
                {t("home.setupBanner.action")}
              </Button>
            </div>
          )}
          <PageHeader title={t("home.title")} subtitle={t("home.subtitle")} actions={hasAny ? newButton : undefined} />

          {hasAny && (
            <div className="relative mt-8 max-w-sm">
              <Search
                aria-hidden
                className="text-muted-foreground pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2"
              />
              <Input
                type="search"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={t("home.searchPlaceholder")}
                aria-label={t("home.searchLabel")}
                className="h-10 rounded-lg pl-9"
              />
            </div>
          )}

          <div className="mt-6">
            {projects.isError ? (
              <EmptyState
                icon={AlertTriangle}
                title={t("home.loadError")}
                body={friendlyError(projects.error, t).fix}
                action={
                  <Button variant="outline" onClick={() => projects.refetch()}>
                    {t("common.retry")}
                  </Button>
                }
              />
            ) : projects.isPending ? (
              <Grid>
                {[0, 1, 2].map((i) => (
                  <ProjectCardSkeleton key={i} />
                ))}
              </Grid>
            ) : projects.data.length > 0 ? (
              <Grid>
                {projects.data.map((p) => (
                  <ProjectCard key={p.id} project={p} onRemove={setRemoving} />
                ))}
              </Grid>
            ) : query ? (
              <EmptyState
                icon={SearchX}
                title={t("home.noResults.title", { query })}
                body={t("home.noResults.body")}
                action={
                  <Button variant="outline" onClick={() => setSearch("")}>
                    {t("home.clearSearch")}
                  </Button>
                }
              />
            ) : (
              <EmptyState
                icon={Sparkles}
                title={t("home.empty.title")}
                body={t("home.empty.body")}
                action={<>{newButton}</>}
              />
            )}
          </div>
        </div>
      </main>

      <NewProjectDialog key={dialogKey} open={creating} onOpenChange={setCreating} />
      <RemoveProjectDialog project={removing} onClose={() => setRemoving(null)} />
    </div>
  );
}

function Grid({ children }: { children: React.ReactNode }) {
  return <div className="animate-in-up grid gap-4 sm:grid-cols-2 lg:grid-cols-3">{children}</div>;
}

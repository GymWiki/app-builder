import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, CheckCircle2, Circle, Loader2 } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useParams } from "react-router";
import { toast } from "sonner";
import { Logo } from "@/components/layout/top-bar";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { restoreBuild, useBuild } from "./build-store";

/** Het bouwscherm: een raster met alle pagina's die één voor één verschijnen, voortgang in gewone taal en een eerlijke tijdsverwachting. */
export default function BuildPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { id = "" } = useParams();
  const project = useQuery({ queryKey: ["project", id], queryFn: () => api.projectGet(id) });
  const progress = useBuild((s) => s.progress[id]);
  const report = useBuild((s) => s.report[id]);
  useEffect(() => {
    void restoreBuild(id);
  }, [id]);

  const failed = report ? [...report.pagesFailed, ...report.pagesAttention] : [];
  const finished = !!report;

  // Alles gelukt: direct naar de startpagina van de nieuwe app, met één vriendelijke tip.
  useEffect(() => {
    if (!report || report.cancelled || failed.length > 0 || !report.coreOk) return;
    const timer = setTimeout(() => {
      toast.info(t("build.tip"), { duration: 9000 });
      navigate(`/project/${id}`, { replace: true });
    }, 1500);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [report]);

  const n = progress ? Math.min(progress.done + Math.max(progress.building.length, 1), progress.total) : 0;
  const text = !progress
    ? t("build.starting")
    : finished
      ? report.cancelled
        ? t("build.cancelled")
        : failed.length || !report.coreOk
          ? t("build.partial")
          : t("build.done")
      : progress.phase === "scaffold"
        ? t("build.phase.scaffold")
        : progress.phase === "core"
          ? t("build.phase.core")
          : progress.phase === "integrate"
            ? t("build.phase.integrate")
            : t("build.phase.pages", { n, total: progress.total });
  const eta = progress?.etaSeconds != null && !finished ? Math.max(1, Math.round(progress.etaSeconds / 60)) : null;

  const retry = async () => {
    try {
      useBuild.setState((s) => {
        const r = { ...s.report };
        delete r[id];
        return { report: r };
      });
      await api.onboardingRetry(id, failed);
    } catch (e) {
      toastError(e, t);
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <header className="mx-auto flex max-w-4xl items-center gap-2.5 px-6 pt-6">
        <Logo />
        <h1 className="t-heading">{project.data?.name ?? t("build.title")}</h1>
      </header>
      <main className="mx-auto max-w-4xl px-6 pt-8 pb-16">
        <div role="status" aria-live="polite" className="mb-6 space-y-2">
          <p className="t-display" data-testid="build-text">
            {text}
          </p>
          {progress && !finished && (
            <>
              <Progress value={progress.total ? (progress.done / progress.total) * 100 : 0} className="h-2" aria-label={text} />
              <p className="t-small text-muted-foreground">
                {eta ? t("build.eta", { minutes: eta }) : t("build.etaUnknown")} · {t("build.parallel", { count: progress.parallel })}
              </p>
            </>
          )}
        </div>

        <ul className="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-4" aria-label={t("build.pages")} data-testid="build-grid">
          {(progress?.pages ?? []).map((p) => (
            <li key={p.id}>
              <button
                type="button"
                disabled={p.status !== "done"}
                onClick={() => navigate(`/project/${id}`)}
                title={p.status === "done" ? t("build.lookAround") : undefined}
                className={cn("bg-card w-full rounded-2xl border p-3 text-left transition-colors", p.status === "done" && "hover:bg-accent/40", p.status === "building" && "ring-primary/50 ring-2")}
              >
                <div className="bg-muted mb-2 grid aspect-[9/14] place-items-center overflow-hidden rounded-xl" data-status={p.status}>
                  {p.status === "building" ? (
                    <Loader2 aria-label={t("build.building")} className="text-primary size-6 animate-spin" />
                  ) : p.status === "done" ? (
                    <CheckCircle2 aria-label={t("build.ready")} className="text-success size-7" />
                  ) : p.status === "failed" || p.status === "attention" ? (
                    <AlertTriangle aria-label={t("build.problem")} className="text-warning size-7" />
                  ) : (
                    <Circle aria-label={t("build.waiting")} className="text-muted-foreground size-6" />
                  )}
                </div>
                <span className="t-body block truncate font-medium">{p.name}</span>
                <span className="t-small text-muted-foreground">{t(`build.status.${p.status}`)}</span>
              </button>
            </li>
          ))}
        </ul>

        <div className="mt-8 flex flex-wrap items-center gap-3">
          {!finished && progress && (
            <Button variant="outline" onClick={() => void api.onboardingCancel(id)}>
              {t("build.cancel")}
            </Button>
          )}
          {finished && failed.length > 0 && !report.cancelled && (
            <div className="space-y-2">
              <p role="alert" className="t-small">
                {t("build.notReady", { pages: failed.join(", ") })}
              </p>
              <Button onClick={() => void retry()}>{t("build.retry")}</Button>
            </div>
          )}
          {finished && (
            <Button variant={failed.length ? "outline" : "default"} onClick={() => navigate(`/project/${id}`, { replace: true })}>
              {t("build.open")}
            </Button>
          )}
        </div>
      </main>
    </div>
  );
}

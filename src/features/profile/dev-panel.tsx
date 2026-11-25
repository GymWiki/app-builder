import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Card } from "@/components/ds/card";
import { Button } from "@/components/ui/button";
import { qk } from "@/hooks";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Project } from "@/lib/types";

function Metrics({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["runMetrics", projectId], queryFn: () => api.runMetrics(projectId) });
  const m = q.data;
  if (!m) return null;
  if (m.runs === 0) return <p className="t-small text-muted-foreground">{t("profile.dev.metrics.none")}</p>;
  const lv = (k: string) => m.byLevel[k] ?? 0;
  return (
    <ul className="t-small text-muted-foreground space-y-0.5 font-mono">
      <li>{t("profile.dev.metrics.runs", { n: m.runs })}</li>
      <li>
        {t("profile.dev.metrics.levels", {
          small: lv("small"),
          screen: lv("screen"),
          shared: lv("shared"),
          project: lv("project"),
        })}
      </li>
      <li>{t("profile.dev.metrics.expanded", { n: m.expanded })}</li>
      <li>{t("profile.dev.metrics.clean", { n: m.cleanFirstTime })}</li>
      <li>{t("profile.dev.metrics.repaired", { n: m.repaired })}</li>
      <li>{t("profile.dev.metrics.rolledBack", { n: m.rolledBack })}</li>
      <li>{t("profile.dev.metrics.ai", { n: m.aiQuestions })}</li>
      <li>{t("profile.dev.metrics.denied", { n: m.deniedEdits })}</li>
      <li>
        {t("profile.dev.metrics.avg", { s: (m.avgDurationMs / 1000).toFixed(1), files: m.avgFilesChanged.toFixed(1) })}
      </li>
      <li>{t("profile.dev.metrics.cost", { cost: m.totalCostUsd.toFixed(2) })}</li>
    </ul>
  );
}

function Info({ project }: { project: Project }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["profileInfo", project.id], queryFn: () => api.profileInfo(project.id) });
  const i = q.data;
  return (
    <li className="space-y-1 py-3">
      <p className="t-body font-medium">{project.name}</p>
      {i && (
        <ul className="t-small text-muted-foreground space-y-0.5 font-mono">
          <li>{t("profile.dev.version", { version: i.version, current: i.current })}</li>
          <li>{t("profile.dev.plugin", { state: t(i.plugin ? "profile.dev.yes" : "profile.dev.no") })}</li>
          <li>
            {t("profile.dev.dartMcp")}: {i.dart ? t("profile.dev.dart", { dart: i.dart }) : "–"} ·{" "}
            {t(i.dartMcpOk ? "profile.dev.okay" : "profile.dev.notOkay")}
          </li>
          <li>{t("profile.dev.structure", { structure: i.structure })}</li>
        </ul>
      )}
      <Metrics projectId={project.id} />
      <Button
        size="xs"
        variant="outline"
        onClick={() =>
          void api
            .profileApply(project.id)
            .then(() => q.refetch())
            .catch((e) => toastError(e, t))
        }
      >
        {t("profile.dev.reapply")}
      </Button>
    </li>
  );
}

/** Verborgen ontwikkelaarsmodus: technische toestand van het projectprofiel. */
export function DevPanel() {
  const { t } = useTranslation();
  const projects = useQuery({ queryKey: qk.projects(""), queryFn: () => api.projectsList("") });
  return (
    <Card className="space-y-2 p-6">
      <h2 className="t-heading">{t("profile.dev.title")}</h2>
      <p className="t-small text-muted-foreground">{t("profile.dev.on")}</p>
      <ul className="divide-y">
        {(projects.data ?? []).map((p) => (
          <Info key={p.id} project={p} />
        ))}
      </ul>
    </Card>
  );
}

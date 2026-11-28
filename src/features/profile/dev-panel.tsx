import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Card } from "@/components/ds/card";
import { Button } from "@/components/ui/button";
import { qk, useSettings } from "@/hooks";
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
      {(["byClass", "byModel"] as const).map((g) => (
        <li key={g} className="pt-1">
          <p className="font-sans font-medium">{t(`profile.dev.models.${g}`)}</p>
          <ul>
            {Object.entries(m[g] ?? {}).map(([name, st]) => (
              <li key={name}>
                {t("profile.dev.models.row", {
                  name,
                  runs: st.runs,
                  pct: st.runs ? Math.round((st.ok / st.runs) * 100) : 0,
                  clean: st.clean,
                  esc: st.escalated,
                  cost: st.costUsd.toFixed(2),
                  tokens: st.tokens,
                })}
              </li>
            ))}
          </ul>
        </li>
      ))}
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

function ModelOverride() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const settings = useSettings();
  const value = settings.data?.model_override || "auto";
  return (
    <div className="space-y-1 border-t pt-3">
      <label htmlFor="model-override" className="t-body block font-medium">
        {t("profile.dev.models.override")}
      </label>
      <select
        id="model-override"
        value={value}
        onChange={(e) =>
          void api
            .settingsSet("model_override", e.target.value)
            .then(() => qc.invalidateQueries({ queryKey: qk.settings }))
            .catch((err) => toastError(err, t))
        }
        className="bg-card h-9 rounded-md border px-2 text-sm"
      >
        {["auto", "light", "mid", "strong"].map((o) => (
          <option key={o} value={o}>
            {t(`profile.dev.models.${o}`)}
          </option>
        ))}
      </select>
      <p className="t-small text-muted-foreground">{t("profile.dev.models.help")}</p>
    </div>
  );
}

/** UX-meting: per flow wat Studio zelf deed (deterministisch), wat via de AI ging en hoeveel tokens dat naar schatting scheelde. */
function UxOverview() {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["uxSummary"], queryFn: () => api.uxSummary(), refetchInterval: 5000 });
  if (!q.data) return null;
  const flows = Object.entries(q.data.flows);
  return (
    <div className="space-y-1 border-t pt-3" data-testid="ux-overview">
      <h3 className="t-body font-medium">{t("profile.dev.ux.title")}</h3>
      {flows.length === 0 ? (
        <p className="t-small text-muted-foreground">{t("profile.dev.ux.none")}</p>
      ) : (
        <table className="t-small w-full text-left font-mono">
          <thead className="text-muted-foreground">
            <tr>
              <th>{t("profile.dev.ux.flow")}</th>
              <th>{t("profile.dev.ux.deterministic")}</th>
              <th>{t("profile.dev.ux.ai")}</th>
              <th>{t("profile.dev.ux.chat")}</th>
            </tr>
          </thead>
          <tbody>
            {flows.map(([flow, k]) => (
              <tr key={flow}>
                <td>{flow}</td>
                <td>{k.deterministic ?? 0}</td>
                <td>{k.ai ?? 0}</td>
                <td>{k.chat_turn ?? 0}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="t-small text-muted-foreground">
        {t("profile.dev.ux.saved", {
          tokens: q.data.estimatedTokensSaved.toLocaleString(),
          avg: q.data.avgTokensPerAiRun.toLocaleString(),
        })}
      </p>
    </div>
  );
}

/** Verborgen ontwikkelaarsmodus: technische toestand van het projectprofiel en de modelkeuze. */
export function DevPanel() {
  const { t } = useTranslation();
  const projects = useQuery({ queryKey: qk.projects(""), queryFn: () => api.projectsList("") });
  return (
    <Card className="space-y-2 p-6">
      <h2 className="t-heading">{t("profile.dev.title")}</h2>
      <p className="t-small text-muted-foreground">{t("profile.dev.on")}</p>
      <ModelOverride />
      <UxOverview />
      <ul className="divide-y">
        {(projects.data ?? []).map((p) => (
          <Info key={p.id} project={p} />
        ))}
      </ul>
    </Card>
  );
}

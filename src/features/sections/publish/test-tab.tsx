import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ChevronDown, Loader2, Play, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ds/status-badge";
import { useChat } from "@/features/chat/chat-store";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Meta, TestIssue, TestScreen } from "@/lib/types";
import { AskButton, Card2, Check, useAsk } from "../shared";
import { NO_LINES, runTest, useJobs } from "./jobs-store";

function Shot({ projectId, name }: { projectId: string; name: string }) {
  const q = useQuery({
    queryKey: ["shot", projectId, name],
    queryFn: () => api.fileDataUrl(projectId, `build/studio_test/${name}`).catch(() => null),
    staleTime: Infinity,
  });
  return q.data ? (
    <img src={q.data} alt={name} className="h-40 w-auto rounded-lg border bg-white object-contain" />
  ) : null;
}

export function TestTab({
  projectId,
  meta,
  onMeta,
}: {
  projectId: string;
  meta: Meta;
  onMeta: (m: Meta) => Promise<void>;
}) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const view = useQuery({ queryKey: ["testReport", projectId], queryFn: () => api.testrunReport(projectId) });
  const running = useJobs((s) => s.status[projectId]?.test === "running");
  const log = useJobs((s) => s.logs[projectId] ?? NO_LINES);
  const retest = useJobs((s) => s.retestPending[projectId] ?? false);
  const chatBusy = useChat((s) => s.byProject[projectId]?.running ?? false);
  const { ask } = useAsk(projectId);
  const [autoRetest, setAutoRetest] = useState(true);
  const [showLog, setShowLog] = useState(false);

  const refresh = async () => {
    await Promise.all([
      qc.invalidateQueries({ queryKey: ["testReport", projectId] }),
      qc.invalidateQueries({ queryKey: ["checklist", projectId] }),
      qc.invalidateQueries({ queryKey: ["project", projectId] }),
    ]);
  };
  const start = async () => {
    try {
      await runTest(projectId, refresh);
    } catch (e) {
      toastError(e, t);
    }
  };
  const issues: { screen: string; issue: TestIssue }[] = [
    ...(view.data?.report?.errors ?? []).map((i) => ({ screen: "", issue: i })),
    ...(view.data?.report?.screens ?? []).flatMap((s) => s.issues.map((i) => ({ screen: s.name, issue: i }))),
  ];
  const errors = issues.filter((i) => i.issue.severity === "error");
  const fix = (list: typeof issues) => {
    const problems = list
      .map(
        (i) =>
          `- ${i.screen ? `[${i.screen}] ` : ""}${t(`sec.publish.test.kind.${i.issue.kind}`, { defaultValue: i.issue.kind })}: ${i.issue.message}${i.issue.detail ? `\n  ${i.issue.detail.split("\n")[0]}` : ""}`,
      )
      .join("\n");
    useJobs.getState().setRetest(projectId, autoRetest);
    ask(t("sec.publish.test.fixPrompt", { problems }));
  };
  const r = view.data;
  void i18n;

  return (
    <div className="space-y-5">
      <Card2 title={t("sec.publish.test.title")}>
        <p className="t-body text-muted-foreground">{t("sec.publish.test.intro")}</p>
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={running || chatBusy} onClick={() => void start()}>
            {running ? <Loader2 className="animate-spin" /> : <Play />}{" "}
            {running
              ? t("sec.publish.test.running")
              : r?.report
                ? t("sec.publish.test.rerun")
                : t("sec.publish.test.run")}
          </Button>
          {r && r.passed && (
            <StatusBadge tone="success">
              <CheckCircle2 className="size-3" /> {t("sec.publish.test.passed")}
            </StatusBadge>
          )}
          {r && !r.passed && r.report && !r.stale && (
            <StatusBadge tone="danger">{t("sec.publish.test.failed")}</StatusBadge>
          )}
          {r && !r.report && <StatusBadge>{t("sec.publish.test.never")}</StatusBadge>}
        </div>
        {retest && chatBusy && <p className="t-small text-muted-foreground">{t("sec.publish.test.retesting")}</p>}
        {r?.stale && (
          <p role="status" className="text-warning t-small font-medium">
            {t("sec.publish.test.stale")}
          </p>
        )}
        <label className="t-small flex items-center gap-2">
          <input
            type="checkbox"
            checked={autoRetest}
            onChange={(e) => setAutoRetest(e.target.checked)}
            className="accent-primary size-4"
          />{" "}
          {t("sec.publish.test.autoRetest")}
        </label>
      </Card2>

      {r?.report && (
        <>
          <p className="t-body font-medium">
            {t("sec.publish.test.summary", {
              screens: r.report.screens.length,
              taps: r.report.tapped,
              errors: r.errors,
              warnings: r.warnings,
            })}
          </p>
          {errors.length > 0 && (
            <Button size="sm" onClick={() => fix(errors)} disabled={chatBusy}>
              {t("sec.publish.test.fixAll")}
            </Button>
          )}
          <ul className="space-y-3">
            {r.report.screens.map((s: TestScreen) => (
              <li key={s.name + s.depth} className="bg-card shadow-soft rounded-xl border p-4">
                <div className="flex flex-wrap items-start gap-4">
                  {s.screenshot && <Shot projectId={projectId} name={s.screenshot} />}
                  <div className="min-w-0 flex-1">
                    <h4 className="t-body font-semibold">{s.name}</h4>
                    {s.issues.length === 0 ? (
                      <p className="text-success t-small">{t("sec.publish.test.screenOk")}</p>
                    ) : (
                      <ul className="mt-1 space-y-2">
                        {s.issues.map((i, n) => (
                          <li key={n} className="t-small flex items-start gap-2">
                            <TriangleAlert
                              aria-hidden
                              className={
                                i.severity === "error"
                                  ? "text-destructive mt-0.5 size-3.5 shrink-0"
                                  : "text-warning mt-0.5 size-3.5 shrink-0"
                              }
                            />
                            <span className="min-w-0 flex-1">
                              <span className="font-medium">
                                {t(`sec.publish.test.kind.${i.kind}`, { defaultValue: i.kind })}
                              </span>
                              : {i.message}
                              {i.severity === "error" && (
                                <Button
                                  size="xs"
                                  variant="ghost"
                                  className="ml-1"
                                  disabled={chatBusy}
                                  onClick={() => fix([{ screen: s.name, issue: i }])}
                                >
                                  {t("sec.publish.test.fix")}
                                </Button>
                              )}
                            </span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                </div>
              </li>
            ))}
          </ul>
          <p className="t-small text-muted-foreground">{t("sec.publish.test.note")}</p>
        </>
      )}

      {r && (
        <Card2 title={t("sec.publish.test.honestTitle")}>
          <ul className="t-body list-disc space-y-1 pl-5">
            {r.notTested.map((k) => (
              <li key={k}>{t(`sec.publish.test.honest.${k}`)}</li>
            ))}
          </ul>
        </Card2>
      )}

      {r && r.nativeFeatures.length > 0 && (
        <Card2 title={t("sec.publish.test.manualTitle")}>
          <p className="t-small text-muted-foreground">{t("sec.publish.test.manualHelp")}</p>
          {r.nativeFeatures.map((k) => (
            <Check
              key={k}
              id={`manual-${k}`}
              checked={meta.publishing.manualChecks[k] ?? false}
              onChange={(v) =>
                void onMeta({
                  ...meta,
                  publishing: { ...meta.publishing, manualChecks: { ...meta.publishing.manualChecks, [k]: v } },
                })
              }
              label={t(`sec.publish.items.native_manual.missing.${k}`, { defaultValue: k })}
            />
          ))}
        </Card2>
      )}

      <Card2>
        <Check
          id="device-tested"
          checked={meta.publishing.deviceTested}
          onChange={(v) => void onMeta({ ...meta, publishing: { ...meta.publishing, deviceTested: v } })}
          label={t("sec.publish.test.deviceTested")}
        />
        <AskButton
          projectId={projectId}
          prompt={t("sec.publish.test.unitTestsPrompt")}
          label={t("sec.publish.test.unitTests")}
          variant="outline"
        />
      </Card2>

      {log.length > 0 && (
        <div>
          <button
            type="button"
            aria-expanded={showLog}
            onClick={() => setShowLog((v) => !v)}
            className="text-muted-foreground hover:text-foreground t-small inline-flex items-center gap-1 rounded underline-offset-2 hover:underline"
          >
            <ChevronDown className={showLog ? "size-3.5 rotate-180" : "size-3.5"} /> {t("sec.publish.test.log")}
          </button>
          {showLog && (
            <pre
              tabIndex={0}
              className="bg-muted t-small mt-2 max-h-60 overflow-auto rounded-lg p-3 font-mono whitespace-pre-wrap"
            >
              {log.join("\n")}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, ExternalLink, Loader2, Play } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { qk } from "@/hooks";
import { api, onEvent } from "@/lib/ipc";
import { openExternal } from "@/lib/open";
import { toastError } from "@/lib/toast";
import type { FlutterCheck } from "@/lib/types";

const TERMS_URL = "https://developer.android.com/studio/terms";

interface LicProgress {
  accepted: number;
  total: number | null;
  phase: "starting" | "asking" | "done" | "stalled";
}
interface Done {
  result: { ok: boolean; accepted: number; total: number | null; already: boolean; stalled: boolean; cancelled: boolean; method: string; log: string } | null;
  problems: { code: string }[];
  check: FlutterCheck | null;
  confirmed: boolean;
  seconds: number;
  alternative: boolean;
}

import { create } from "zustand";

export const useLic = create<{ running: boolean; progress: LicProgress | null; done: Done | null }>(() => ({
  running: false,
  progress: null,
  done: null,
}));

/** Luistert naar de voortgang van het accepteren. Op de installatiepagina aanroepen; de uitkomst blijft zichtbaar ook als de rij daarna groen wordt. */
export function initLicenseListeners(qc: import("@tanstack/react-query").QueryClient): () => void {
  const a = onEvent<LicProgress>("license-progress", (p) => useLic.setState({ progress: p, running: true }));
  const b = onEvent<Done>("license-done", (d) => {
    useLic.setState({ running: false, done: d, progress: null });
    // De controle (flutter doctor) is al gedaan: de uitkomst gaat rechtstreeks in de lijst, zonder "Opnieuw controleren".
    if (d.check) qc.setQueryData(qk.tool("flutter"), d.check);
    else void qc.invalidateQueries({ queryKey: qk.tool("flutter") });
    void qc.invalidateQueries({ queryKey: ["licenseConsent"] });
  });
  return () => {
    void a.then((f) => f());
    void b.then((f) => f());
  };
}

/** Android-licenties accepteren zonder terminal: één toestemming, voortgang in gewone taal, diagnose bij een vastloper, automatische controle na afloop. */
export function LicenseFlow({ showForm }: { showForm: boolean }) {
  const { t } = useTranslation();
  const status = useQuery({ queryKey: ["licenseConsent"], queryFn: api.androidLicensesStatus });
  const [agreed, setAgreed] = useState(false);
  const { running, progress, done } = useLic();
  const [details, setDetails] = useState(false);
  const setRunning = (v: boolean) => useLic.setState({ running: v });
  const setProgress = (p: LicProgress | null) => useLic.setState({ progress: p });
  const setDone = (d: Done | null) => useLic.setState({ done: d });

  const start = async (alternative: boolean) => {
    setDone(null);
    setProgress({ accepted: 0, total: null, phase: "starting" });
    setRunning(true);
    try {
      await api.androidLicensesAccept(agreed || !!status.data?.consent, alternative);
    } catch (e) {
      setRunning(false);
      setProgress(null);
      toastError(e, t);
    }
  };

  const consent = !!status.data?.consent;
  const total = progress?.total ?? null;
  const text = !progress
    ? ""
    : progress.phase === "stalled"
      ? t("tools.android.license.stalled")
      : progress.phase === "done"
        ? t("tools.android.license.checking")
        : total
          ? t("tools.android.license.progress", { accepted: progress.accepted, total })
          : t("tools.android.license.starting");

  const problemFix = (code: string) => (["java_missing", "sdk_missing", "cmdline_tools_missing"].includes(code) ? code : null);

  if (!showForm && !done?.result?.ok) return null;
  return (
    <div className="mt-4 space-y-3" data-testid="license-flow">
      {showForm && !running && !done?.result?.ok && (
        <>
          {!consent && (
            <label className="flex items-start gap-2.5">
              <input
                type="checkbox"
                checked={agreed}
                onChange={(e) => setAgreed(e.target.checked)}
                className="mt-1 size-4 shrink-0"
              />
              <span className="t-body">
                {t("tools.android.license.agree")}{" "}
                <button
                  type="button"
                  className="text-primary inline-flex items-center gap-1 underline underline-offset-2"
                  onClick={() => void openExternal(TERMS_URL)}
                >
                  {t("tools.android.license.terms")} <ExternalLink className="size-3.5" aria-hidden />
                </button>
              </span>
            </label>
          )}
          {consent && <p className="t-small text-muted-foreground">{t("tools.android.license.agreedBefore")}</p>}
          <Button size="sm" disabled={!consent && !agreed} onClick={() => void start(false)}>
            <Play /> {t("tools.android.acceptLicenses")}
          </Button>
        </>
      )}

      {running && (
        <div role="status" aria-live="polite" className="space-y-2">
          <div className="flex items-center gap-3">
            {total ? (
              <Progress value={((progress?.accepted ?? 0) / total) * 100} className="h-2" />
            ) : (
              <Loader2 aria-hidden className="size-4 animate-spin" />
            )}
            <Button size="xs" variant="ghost" onClick={() => void api.androidLicensesCancel()}>
              {t("tools.android.license.stop")}
            </Button>
          </div>
          <p className="t-small font-medium">{text}</p>
          {progress?.phase === "stalled" && <p className="t-small text-muted-foreground">{t("tools.android.license.stalledHelp")}</p>}
        </div>
      )}

      {done && !running && (
        <div role="status" className="space-y-2">
          {done.result?.ok && done.confirmed && (
            <p className="text-success t-small font-medium">
              {done.result.already
                ? t("tools.android.license.alreadyDone")
                : t("tools.android.license.allDone", { count: done.result.accepted, seconds: done.seconds })}
            </p>
          )}
          {done.result?.ok && !done.confirmed && (
            <p className="text-warning t-small">{t("tools.android.license.acceptedButDoctor")}</p>
          )}
          {done.result?.cancelled && <p className="t-small text-muted-foreground">{t("tools.android.license.cancelled")}</p>}
          {!done.result?.ok && !done.result?.cancelled && (
            <div className="border-warning/50 bg-warning/10 space-y-2 rounded-xl border p-3">
              <p className="t-small flex items-center gap-2 font-medium">
                <AlertTriangle className="size-4" aria-hidden /> {t("tools.android.license.failed")}
              </p>
              {done.problems.length > 0 && (
                <ul className="t-small list-disc space-y-1 pl-5">
                  {done.problems.map((p) => (
                    <li key={p.code}>{t(`tools.android.license.problems.${p.code}`, { defaultValue: p.code })}</li>
                  ))}
                </ul>
              )}
              <div className="flex flex-wrap gap-2">
                {!done.alternative && !done.problems.some((p) => problemFix(p.code) && p.code !== "java_missing") && (
                  <Button size="sm" onClick={() => void start(true)}>
                    {t("tools.android.license.fixForMe")}
                  </Button>
                )}
                {done.problems.some((p) => problemFix(p.code)) && (
                  <Button size="sm" variant="outline" onClick={() => void openExternal(t("tools.android.download"))}>
                    <ExternalLink /> {t("setup.download")}
                  </Button>
                )}
                <Button size="sm" variant="ghost" onClick={() => void start(done.alternative)}>
                  {t("tools.android.license.retry")}
                </Button>
              </div>
            </div>
          )}
          {done.result?.log && (
            <div>
              <button
                type="button"
                aria-expanded={details}
                onClick={() => setDetails((d) => !d)}
                className="text-muted-foreground hover:text-foreground t-small rounded underline-offset-2 hover:underline"
              >
                {details ? t("common.hideDetails") : t("common.showDetails")}
              </button>
              {details && (
                <pre className="bg-muted t-small mt-2 max-h-40 overflow-auto rounded-xl p-3 font-mono whitespace-pre-wrap">
                  {done.result.log}
                </pre>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

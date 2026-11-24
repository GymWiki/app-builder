import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  CircleDashed,
  Download,
  ExternalLink,
  Loader2,
  Play,
  Terminal,
  XCircle,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Card } from "@/components/ds/card";
import { Button } from "@/components/ui/button";
import type { InstallPlan } from "@/lib/types";
import { openExternal } from "@/lib/open";
import { cn } from "@/lib/utils";
import type { Row } from "./use-toolchain";

interface Props {
  row: Row;
  os: string;
  minVersion?: string;
  busy?: boolean;
  canSignIn?: boolean;
  onInstall: (component: string, label: string, plan: InstallPlan) => void;
  onSignIn: () => void;
  onRecheck: () => void;
  installPlans: Record<string, InstallPlan | null | undefined>;
  children?: React.ReactNode;
}

const ICON = {
  checking: <Loader2 className="text-muted-foreground size-5 animate-spin" />,
  ok: <CheckCircle2 className="text-success size-5" />,
  missing: <XCircle className="text-destructive size-5" />,
  outdated: <AlertTriangle className="text-warning size-5" />,
  warning: <AlertTriangle className="text-warning size-5" />,
};

export function ToolRow({
  row,
  os,
  minVersion,
  busy,
  canSignIn,
  onInstall,
  onSignIn,
  onRecheck,
  installPlans,
  children,
}: Props) {
  const { t } = useTranslation();
  const [stepsOpen, setStepsOpen] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const { id, state, status } = row;
  const isLogin = id === "claudeLogin";
  const ns = `tools.${id}`;
  const bad = state === "missing" || state === "outdated" || state === "warning";
  const issue = id === "android" ? (status?.issue ?? "other") : null;

  // Uitleg + stappen per situatie
  let steps: string[] = [];
  if (bad && !isLogin) {
    if (id === "android")
      steps = t(`tools.android.steps.${issue}`, { returnObjects: true, defaultValue: [] }) as string[];
    else if (id === "claude" && state === "outdated")
      steps = t("tools.claude.updateSteps", { returnObjects: true }) as string[];
    else {
      const osKey = os === "windows" ? "windows" : os === "macos" ? "macos" : "linux";
      steps = t(`${ns}.steps.${osKey}`, { returnObjects: true, defaultValue: [] }) as string[];
    }
  }

  let statusText: string;
  if (state === "checking") statusText = t("setup.checking");
  else if (state === "ok")
    statusText = status?.version ? t("setup.readyVersion", { version: status.version }) : t("setup.ready");
  else if (state === "outdated")
    statusText = t("setup.outdated", { version: status?.version ?? "?", min: minVersion ?? status?.minVersion ?? "?" });
  else if (id === "android" && issue) statusText = t(`tools.android.issues.${issue}`);
  else if (isLogin) statusText = t(canSignIn ? "setup.login.hint" : "setup.login.needsClaude");
  else statusText = t(state === "missing" ? "setup.missing" : "setup.warning");

  const installKey = id === "android" && issue === "licenses" ? "android_licenses" : id;
  const plan = installPlans[installKey];
  const canDownload =
    !isLogin && bad && id !== "android" ? true : id === "android" && (issue === "no_sdk" || issue === "cmdline_tools");
  const needsFlutterFirst = id === "android" && issue === "needs_flutter";

  return (
    <Card className={cn("animate-in-up p-5", state === "ok" && "bg-card/70")}>
      <div className="flex items-start gap-4">
        <div className="mt-0.5 shrink-0" aria-hidden>
          {id === "claudeLogin" && state === "missing" ? (
            <CircleDashed className="text-muted-foreground size-5" />
          ) : (
            ICON[state]
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="t-heading">{t(`${ns}.name`)}</h2>
            {isLogin && state === "ok" && (
              <span className="t-small text-success font-semibold">{t("setup.login.connected")}</span>
            )}
          </div>
          <p className="text-muted-foreground t-body">{t(`${ns}.why`)}</p>
          <p
            className={cn(
              "t-small mt-2 font-medium",
              state === "ok" ? "text-success" : bad ? "text-foreground" : "text-muted-foreground",
            )}
            aria-live="polite"
          >
            {!(isLogin && state === "ok") && statusText}
          </p>

          {bad && (
            <div className="mt-4 flex flex-wrap items-center gap-2">
              {plan && (
                <Button
                  size="sm"
                  disabled={busy}
                  onClick={() =>
                    onInstall(installKey, t(id === "android" ? "tools.android.acceptLicenses" : `${ns}.name`), plan)
                  }
                >
                  {id === "android" ? <Play /> : <Download />}
                  {id === "android" ? t("tools.android.acceptLicenses") : t("setup.install")}
                </Button>
              )}
              {isLogin && canSignIn && (
                <Button size="sm" disabled={busy} onClick={onSignIn}>
                  {t("setup.login.signIn")}
                </Button>
              )}
              {canDownload && (
                <Button size="sm" variant="outline" onClick={() => void openExternal(t(`${ns}.download`))}>
                  <ExternalLink /> {t("setup.download")}
                </Button>
              )}
              {!isLogin && !needsFlutterFirst && (
                <Button size="sm" variant="ghost" onClick={onRecheck}>
                  {t("common.checkAgain")}
                </Button>
              )}
              {steps.length > 0 && (
                <Button size="sm" variant="ghost" aria-expanded={stepsOpen} onClick={() => setStepsOpen((o) => !o)}>
                  {stepsOpen ? t("setup.hideSteps") : t("setup.showSteps")}
                  <ChevronDown
                    className={cn("transition-transform duration-[var(--dur)]", stepsOpen && "rotate-180")}
                  />
                </Button>
              )}
            </div>
          )}

          {stepsOpen && steps.length > 0 && (
            <ol className="bg-muted/60 t-body mt-3 list-decimal space-y-1.5 rounded-xl py-3 pr-4 pl-8">
              {steps.map((s, i) => (
                <li key={i}>{s}</li>
              ))}
            </ol>
          )}

          {bad && status?.detail && (
            <div className="mt-3">
              <button
                type="button"
                aria-expanded={detailsOpen}
                onClick={() => setDetailsOpen((o) => !o)}
                className="text-muted-foreground hover:text-foreground t-small inline-flex items-center gap-1.5 rounded"
              >
                <Terminal className="size-3.5" /> {detailsOpen ? t("common.hideDetails") : t("common.showDetails")}
              </button>
              {detailsOpen && (
                <pre className="bg-muted t-small mt-2 max-h-40 overflow-auto rounded-xl p-3 font-mono whitespace-pre-wrap">
                  {status.detail}
                </pre>
              )}
            </div>
          )}

          {children}
        </div>
      </div>
    </Card>
  );
}

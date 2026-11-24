import { useQueryClient } from "@tanstack/react-query";
import { Rocket } from "lucide-react";
import { lazy, Suspense, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { LanguageSwitch } from "@/components/ds/language-switch";
import { ThemeToggle } from "@/components/ds/theme-toggle";
import { Spinner } from "@/components/ds/spinner";
import { Logo } from "@/components/layout/top-bar";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { qk, useAppInfo } from "@/hooks";
import { api, onEvent } from "@/lib/ipc";
import type { InstallPlan } from "@/lib/types";
import { toastError } from "@/lib/toast";
import { ToolRow } from "./tool-row";
import { useToolchain, type RowId } from "./use-toolchain";

const PtyTerminal = lazy(() => import("./pty-terminal"));

interface TerminalSession {
  ptyId: string;
  /** Welke rij na afloop opnieuw gecontroleerd moet worden. */
  row: RowId;
  label: string;
  finished: boolean;
}

const PLAN_COMPONENTS = ["git", "claude", "android_licenses"] as const;

export function SetupPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { data: info } = useAppInfo();
  const { rows, recheck, doneCount, allOk } = useToolchain();
  const [terminal, setTerminal] = useState<TerminalSession | null>(null);
  const [confirm, setConfirm] = useState<{ component: string; label: string; plan: InstallPlan } | null>(null);
  const [plans, setPlans] = useState<Record<string, InstallPlan | null>>({});

  // Installatieplannen (vast lijstje in Rust) per component ophalen.
  useEffect(() => {
    void Promise.all(PLAN_COMPONENTS.map(async (c) => [c, await api.installPlan(c)] as const)).then((e) =>
      setPlans(Object.fromEntries(e)),
    );
  }, []);

  // Als een terminalsessie klaar is: de bijbehorende controle opnieuw uitvoeren.
  useEffect(() => {
    const un = onEvent<{ id: string; code: number | null }>("pty-exit", (p) => {
      setTerminal((cur) => {
        if (!cur || cur.ptyId !== p.id) return cur;
        void recheck(cur.row);
        return { ...cur, finished: true };
      });
    });
    return () => void un.then((f) => f());
  }, [recheck]);

  const open = async (row: RowId, label: string, start: () => Promise<string>) => {
    try {
      if (terminal && !terminal.finished) await api.ptyKill(terminal.ptyId);
      setTerminal({ ptyId: await start(), row, label, finished: false });
    } catch (e) {
      toastError(e, t);
    }
  };

  const finish = async (value: "true" | "skipped") => {
    try {
      await api.settingsSet("setup_completed", value);
      await qc.invalidateQueries({ queryKey: qk.settings });
      navigate("/", { replace: true });
    } catch (e) {
      toastError(e, t);
    }
  };

  const closeTerminal = async () => {
    if (terminal && !terminal.finished) await api.ptyKill(terminal.ptyId).catch(() => undefined);
    setTerminal(null);
  };

  const os = info?.os ?? "windows";
  const minOf = (id: RowId) =>
    id === "git"
      ? info?.minVersions.git
      : id === "flutter"
        ? info?.minVersions.flutter
        : id === "claude"
          ? info?.minVersions.claude
          : undefined;
  const claudeOk = rows.find((r) => r.id === "claude")?.state === "ok";
  const busy = !!terminal && !terminal.finished;

  return (
    <div className="h-full overflow-y-auto">
      <header className="mx-auto flex max-w-2xl items-center justify-between px-6 pt-6">
        <div className="flex items-center gap-2.5">
          <Logo />
          <span className="t-heading">{t("common.appName")}</span>
        </div>
        <div className="flex items-center gap-2">
          <LanguageSwitch />
          <ThemeToggle />
        </div>
      </header>

      <main className="mx-auto max-w-2xl px-6 pt-10 pb-16">
        <h1 className="t-display">{t("setup.title")}</h1>
        <p className="text-muted-foreground t-body mt-2">{t("setup.subtitle")}</p>

        <div className="mt-6 flex items-center gap-3" aria-live="polite">
          <Progress
            value={(doneCount / rows.length) * 100}
            className="h-2"
            aria-label={t("setup.progress", { done: doneCount, total: rows.length })}
          />
          <span className="t-small text-muted-foreground font-medium whitespace-nowrap tabular-nums">
            {t("setup.progress", { done: doneCount, total: rows.length })}
          </span>
        </div>

        <div className="mt-6 space-y-3">
          {rows.map((row) => (
            <ToolRow
              key={row.id}
              row={row}
              os={os}
              minVersion={minOf(row.id)}
              busy={busy}
              canSignIn={claudeOk}
              installPlans={plans}
              onInstall={(component, label, plan) => setConfirm({ component, label, plan })}
              onSignIn={() => void open("claudeLogin", t("tools.claudeLogin.name"), api.ptyOpenClaudeLogin)}
              onRecheck={() => void recheck(row.id)}
            >
              {terminal?.row === row.id && (
                <div className="mt-4 space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="t-small text-muted-foreground inline-flex items-center gap-2 font-medium">
                      {!terminal.finished && <Spinner className="size-3.5" />}
                      {terminal.label} ·{" "}
                      {terminal.finished
                        ? t("setup.terminal.done")
                        : row.id === "claudeLogin"
                          ? t("setup.login.waiting")
                          : t("setup.terminal.running")}
                    </span>
                    <Button size="xs" variant="ghost" onClick={() => void closeTerminal()}>
                      {t("setup.terminal.close")}
                    </Button>
                  </div>
                  <Suspense fallback={<div className="h-56 animate-pulse rounded-xl bg-[#0f1117]" />}>
                    <PtyTerminal ptyId={terminal.ptyId} label={t("setup.terminal.title")} />
                  </Suspense>
                </div>
              )}
            </ToolRow>
          ))}
        </div>

        <div className="mt-8 flex flex-wrap items-center justify-between gap-4">
          <p className="text-muted-foreground t-small max-w-sm">
            {allOk ? t("setup.finishHint") : t("setup.skipHint")}
          </p>
          <div className="flex items-center gap-2">
            {!allOk && (
              <Button variant="ghost" onClick={() => void finish("skipped")}>
                {t("common.skipForNow")}
              </Button>
            )}
            <Button disabled={!allOk} onClick={() => void finish("true")}>
              <Rocket /> {t("setup.finish")}
            </Button>
          </div>
        </div>
      </main>

      <AlertDialog open={!!confirm} onOpenChange={(o) => !o && setConfirm(null)}>
        <AlertDialogContent className="rounded-2xl">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("setup.confirmInstall.title", { tool: confirm?.label })}</AlertDialogTitle>
            <AlertDialogDescription>{t("setup.confirmInstall.body")}</AlertDialogDescription>
          </AlertDialogHeader>
          <pre className="bg-muted t-small overflow-x-auto rounded-xl p-3 font-mono">{confirm?.plan.display}</pre>
          <p className="text-muted-foreground t-small">{t("setup.confirmInstall.note")}</p>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (!confirm) return;
                const { component, label } = confirm;
                const row: RowId = component === "android_licenses" ? "android" : (component as RowId);
                void open(row, label, () => api.ptyOpenInstall(component));
              }}
            >
              {t("setup.confirmInstall.run")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDown, ArrowUp, Info, LogIn, RotateCcw } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import { useTranslation } from "react-i18next";
import { StatusBadge, type Tone } from "@/components/ds/status-badge";
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
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { EngineRow, EnginesView } from "@/lib/types";

const PtyTerminal = lazy(() => import("@/components/pty-terminal"));

function state(
  e: EngineRow,
  now: number,
): { key: "ready" | "notInstalled" | "notLoggedIn" | "unknown" | "limited"; tone: Tone } {
  if (!e.installed) return { key: "notInstalled", tone: "neutral" };
  if (e.limitedUntil && e.limitedUntil > now) return { key: "limited", tone: "warning" };
  if (e.loggedIn === false) return { key: "notLoggedIn", tone: "warning" };
  if (e.loggedIn === null) return { key: "unknown", tone: "info" };
  return { key: "ready", tone: "success" };
}

/** Instellingen → AI-verbindingen: welke AI's Studio kent, hun staat, de volgorde van gebruik en de uitleg over waar je code heen gaat. */
export function EnginesCard() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["engines"], queryFn: () => api.enginesList(), refetchInterval: 20_000 });
  const [pending, setPending] = useState<EngineRow | null>(null);
  const [terminal, setTerminal] = useState<{ id: string; name: string } | null>(null);
  const view = q.data;
  if (!view)
    return (
      <p role="status" className="text-muted-foreground t-small">
        {t("common.loading")}
      </p>
    );
  const now = Math.floor(q.dataUpdatedAt / 1000);
  const set = (v: EnginesView) => qc.setQueryData(["engines"], v);
  const setOrder = (order: string[]) =>
    void api
      .enginesSetOrder(order)
      .then(set)
      .catch((e) => toastError(e, t));
  const inOrder = (id: string) => view.order.includes(id);
  const move = (id: string, d: -1 | 1) => {
    const o = [...view.order];
    const i = o.indexOf(id);
    const j = i + d;
    if (i < 0 || j < 0 || j >= o.length) return;
    [o[i], o[j]] = [o[j]!, o[i]!];
    setOrder(o);
  };
  const toggle = (e: EngineRow, on: boolean) => {
    if (on) setPending(e);
    else if (view.order.length > 1) setOrder(view.order.filter((x) => x !== e.id));
  };
  const login = async (e: EngineRow) => {
    try {
      setTerminal({ id: await api.engineOpenLogin(e.id), name: e.name });
    } catch (er) {
      toastError(er, t);
    }
  };

  return (
    <div className="space-y-4">
      <p className="text-muted-foreground t-small">{t("settings.engines.intro")}</p>
      <ul className="space-y-3" aria-label={t("settings.engines.title")}>
        {view.engines.map((e) => {
          const s = state(e, now);
          const pos = view.order.indexOf(e.id);
          return (
            <li key={e.id} className="rounded-xl border p-3.5" data-testid={`engine-${e.id}`}>
              <div className="flex flex-wrap items-center gap-2">
                <span className="t-body font-medium">{e.name}</span>
                <StatusBadge tone={s.tone}>
                  {t(`settings.engines.status.${s.key}`, {
                    time: e.limitedUntil
                      ? new Date(e.limitedUntil * 1000).toLocaleTimeString(i18n.language, {
                          hour: "2-digit",
                          minute: "2-digit",
                        })
                      : "",
                  })}
                </StatusBadge>
                {pos === 0 && <StatusBadge tone="accent">{t("settings.engines.usedFirst")}</StatusBadge>}
                {pos > 0 && <StatusBadge tone="neutral">{t("settings.engines.usedNext")}</StatusBadge>}
                <span className="text-muted-foreground t-small ml-auto font-mono">{e.version ?? ""}</span>
              </div>
              <p className="t-small text-muted-foreground mt-1 flex items-start gap-1.5">
                <Info aria-hidden className="mt-0.5 size-3.5 shrink-0" />
                <span>
                  {t("settings.engines.notice", { name: e.name, provider: e.provider })}{" "}
                  {t("settings.engines.terms", { provider: e.provider })}
                </span>
              </p>
              {!e.proven && <p className="t-small text-muted-foreground mt-1">{t("settings.engines.unproven")}</p>}
              {e.runs > 0 && (
                <p className="t-small text-muted-foreground mt-1">
                  {t("settings.engines.stats", { runs: e.runs, clean: e.clean, rolled: e.rolledBack })}
                </p>
              )}
              <div className="mt-2.5 flex flex-wrap items-center gap-2">
                {!e.installed && (
                  <p className="t-small font-mono">{t("settings.engines.installHint", { command: e.installHint })}</p>
                )}
                {e.installed && e.loggedIn !== true && (
                  <Button size="sm" variant="outline" onClick={() => void login(e)}>
                    <LogIn /> {t("settings.engines.login")}
                  </Button>
                )}
                {e.limitedUntil && e.limitedUntil > now && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() =>
                      void api
                        .engineClearLimit(e.id)
                        .then(set)
                        .catch((er) => toastError(er, t))
                    }
                  >
                    <RotateCcw /> {t("settings.engines.retry")}
                  </Button>
                )}
                <label className="t-small flex items-center gap-2">
                  <input
                    type="checkbox"
                    className="accent-primary size-4"
                    checked={inOrder(e.id)}
                    disabled={inOrder(e.id) && view.order.length === 1}
                    onChange={(ev) => toggle(e, ev.target.checked)}
                  />
                  {t("settings.engines.use")}
                </label>
                {pos >= 0 && view.order.length > 1 && (
                  <span className="ml-auto flex gap-1">
                    <Button
                      size="icon-xs"
                      variant="ghost"
                      disabled={pos === 0}
                      aria-label={t("settings.engines.moveUp", { name: e.name })}
                      onClick={() => move(e.id, -1)}
                    >
                      <ArrowUp />
                    </Button>
                    <Button
                      size="icon-xs"
                      variant="ghost"
                      disabled={pos === view.order.length - 1}
                      aria-label={t("settings.engines.moveDown", { name: e.name })}
                      onClick={() => move(e.id, 1)}
                    >
                      <ArrowDown />
                    </Button>
                  </span>
                )}
              </div>
            </li>
          );
        })}
      </ul>
      <p className="text-muted-foreground t-small">{t("settings.engines.order")}</p>
      {terminal && (
        <div className="space-y-2">
          <p className="t-small font-medium">{t("settings.engines.terminalTitle", { name: terminal.name })}</p>
          <Suspense fallback={null}>
            <div className="h-56 overflow-hidden rounded-xl border">
              <PtyTerminal ptyId={terminal.id} label={t("settings.engines.terminalTitle", { name: terminal.name })} />
            </div>
          </Suspense>
        </div>
      )}
      <AlertDialog open={pending !== null} onOpenChange={(o) => !o && setPending(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("settings.engines.linkTitle", { name: pending?.name })}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("settings.engines.linkBody", { name: pending?.name, provider: pending?.provider })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                if (pending) setOrder([...view.order, pending.id]);
                setPending(null);
              }}
            >
              {t("settings.engines.linkConfirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

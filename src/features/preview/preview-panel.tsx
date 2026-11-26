import { AlertTriangle, Loader2, Play, RefreshCw, Smartphone, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { StatusBadge, type Tone } from "@/components/ds/status-badge";
import { useChat } from "@/features/chat/chat-store";
import { PhoneFrame, type DeviceId } from "@/features/project/phone-frame";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Device, PreviewStatus } from "@/lib/types";
import { DeviceDialog } from "./device-dialog";
import { usePreview } from "./preview-store";
import { summarize } from "@/lib/selection-label";
import type { Selection } from "@/lib/types";
import { DEVICES } from "@/features/project/phone-frame";

/** Het vak dat de gebruiker heeft getekend, in procenten van het telefoonscherm (0–100). */
export interface PickBox {
  x: number;
  y: number;
  w: number;
  h: number;
}
const MIN_BOX = 4; // kleiner dan dit (in %) telt als een klik, niet als een vak

const TONE: Record<PreviewStatus, Tone> = {
  stopped: "neutral",
  starting: "info",
  running: "success",
  reloading: "info",
  error: "danger",
};

export function PreviewPanel({
  projectId,
  device,
  dark,
  route,
  screenName,
  pick,
  onPick,
  selection,
  selecting,
}: {
  projectId: string;
  device: DeviceId;
  dark: boolean;
  /** Pad van de gekozen pagina (go_router); de preview springt er naartoe. */
  route?: string | null;
  /** Naam van de gekozen pagina (voor de melding onder de werkbalk). */
  screenName?: string | null;
  /** Het gekozen element (positie in procenten van het telefoonscherm), of `null` = de hele pagina. */
  pick?: PickBox | null;
  onPick?: (p: PickBox | null) => void;
  /** Wat de widget inspector in het vak vond (alleen in inspect-modus) en of dat nog wordt opgezocht. */
  selection?: Selection | null;
  selecting?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const state = usePreview((s) => s.state);
  const frame = usePreview((s) => s.frame);
  const inspect = state.mode === "inspect";
  const mine = state.projectId === projectId;
  const status: PreviewStatus = mine ? state.status : "stopped";
  const [pending, setPending] = useState(false);
  const [devices, setDevices] = useState(false);
  const [showLog, setShowLog] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [frameKey, setFrameKey] = useState(0);
  const [drag, setDrag] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const [tooSmall, setTooSmall] = useState(false);
  const chatRunning = useChat((s) => s.byProject[projectId]?.running ?? false);
  const onDevice = mine && state.target !== "web";

  // Na een herstel (fout → actief) de pagina opnieuw laden, voor het geval hot restart niets doorgaf.
  const prev = useRef(status);
  useEffect(() => {
    if (prev.current === "error" && status === "running") setFrameKey((k) => k + 1);
    prev.current = status;
  }, [status]);

  // De preview start vanzelf: de gebruiker wil de app meteen zien, zonder eerst op een knop te hoeven drukken.
  const autoStarted = useRef<string | null>(null);
  useEffect(() => {
    if (autoStarted.current === projectId) return;
    autoStarted.current = projectId;
    void api
      .previewState()
      .then((st) => {
        if (st.projectId === projectId && st.status !== "stopped") return;
        if (st.projectId !== null && st.projectId !== projectId && st.status !== "stopped") return;
        return api.previewStart(projectId, "web");
      })
      .catch((e) => toastError(e, t));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  // In inspect-modus bepaalt het gekozen toestel de grootte van de pagina in Chrome (en dus wat het vak betekent).
  const dims = DEVICES[device];
  useEffect(() => {
    if (inspect) void api.previewViewport(dims.w, dims.h).catch(() => undefined);
  }, [inspect, dims.w, dims.h, state.status]);

  useEffect(() => {
    if (!showLog) return;
    const load = () =>
      void api
        .previewLog()
        .then(setLog)
        .catch(() => undefined);
    load();
    const id = setInterval(load, 1500);
    return () => clearInterval(id);
  }, [showLog]);

  const start = async (target: string) => {
    setPending(true);
    try {
      await api.previewStart(projectId, target);
    } catch (e) {
      toastError(e, t);
    } finally {
      setPending(false);
    }
  };
  const stop = async () => {
    setPending(true);
    try {
      await api.previewStop();
    } catch (e) {
      toastError(e, t);
    } finally {
      setPending(false);
    }
  };
  const refresh = () => {
    setFrameKey((k) => k + 1);
    void api.previewReload().catch(() => undefined);
  };

  const problems = mine ? state.problems : [];
  const fixWithClaude = () => {
    const error = problems
      .map((p) => p.excerpt || p.message)
      .join("\n\n")
      .slice(0, 4000);
    void useChat
      .getState()
      .send(projectId, t("preview.problem.fixPrompt", { error }), null, i18n.language.slice(0, 2))
      .catch((e) => toastError(e, t));
  };

  const active = status === "running" || status === "reloading";
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
        <StatusBadge tone={TONE[status]}>{t(`preview.status.${status}`)}</StatusBadge>
        <div className="flex-1" />
        <Button size="sm" variant="outline" onClick={() => setDevices(true)}>
          <Smartphone /> {t("preview.realDevice")}
        </Button>
        {status === "stopped" || (status === "error" && !state.url) ? (
          <>
            <Button size="sm" disabled={pending} onClick={() => void start("web")}>
              {pending ? <Loader2 className="animate-spin" /> : <Play />} {t("preview.start")}
            </Button>
          </>
        ) : (
          <>
            {active && !onDevice && (
              <Button
                size="icon-sm"
                variant="ghost"
                aria-label={t("preview.refresh")}
                title={t("preview.refresh")}
                onClick={refresh}
              >
                <RefreshCw className={status === "reloading" ? "animate-spin" : ""} />
              </Button>
            )}
            <Button size="sm" variant="outline" disabled={pending} onClick={() => void stop()}>
              <Square className="fill-current" /> {t("preview.stop")}
            </Button>
          </>
        )}
      </div>

      {active && !onDevice && state.url && (
        <div
          role="status"
          data-testid="pick-banner"
          className={cn(
            "t-small flex shrink-0 items-center gap-2 border-b px-3 py-2",
            pick ? "bg-warning/15" : "bg-accent/60",
          )}
        >
          <span className="min-w-0 flex-1">
            {tooSmall && !pick ? (
              <>
                <strong>{t("preview.pick.tooSmallTitle")}</strong> {t("preview.pick.tooSmallBody")}
              </>
            ) : pick ? (
              <>
                <strong>
                  {selecting
                    ? t("preview.pick.looking")
                    : selection && selection.targets.length > 0
                      ? (() => {
                          const m = summarize(selection);
                          return m.kind === "many"
                            ? t("preview.pick.many", { count: m.count, screen: m.screen ?? "" })
                            : t(m.text ? "preview.pick.oneText" : "preview.pick.one", {
                                kind: t(`preview.pick.kinds.${m.element}`, { widget: m.widget }),
                                text: m.text,
                                screen: m.screen ?? "",
                              });
                        })()
                      : t("preview.pick.elementTitle")}
                </strong>{" "}
                {t("preview.pick.elementBody")}
              </>
            ) : (
              <>
                <strong>{t("preview.pick.pageTitle", { name: screenName ?? "" })}</strong> {t("preview.pick.pageBody")}
              </>
            )}
          </span>
          {pick && (
            <Button size="xs" variant="outline" onClick={() => onPick?.(null)}>
              {t("preview.pick.back")}
            </Button>
          )}
        </div>
      )}

      <div className="min-h-0 flex-1">
        <PhoneFrame device={device} dark={dark}>
          {active && !onDevice && state.url && inspect ? (
            frame ? (
              <img
                src={frame}
                alt=""
                draggable={false}
                className="pointer-events-none absolute inset-0 size-full bg-white select-none"
              />
            ) : (
              <Loader2 aria-hidden className="absolute inset-0 m-auto size-8 animate-spin opacity-50" />
            )
          ) : null}
          {active && !onDevice && state.url && !inspect ? (
            <iframe
              key={frameKey}
              src={route ? `${state.url.replace(/#.*$/, "")}#${route}` : state.url}
              title={t("preview.title")}
              className="absolute inset-0 size-full border-0 bg-white"
              sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
            />
          ) : null}
          {active && !onDevice && state.url ? (
            // Klikken in de app doen hier niets (dat hoort bij "Testrun"): met slepen teken je een vak voor de chat.
            <div
              data-testid="pick-layer"
              aria-label={t("preview.pick.layer")}
              title={t("preview.pick.layer")}
              className="absolute inset-0 z-10 cursor-crosshair touch-none select-none"
              onPointerDown={(e) => {
                if (e.button !== 0) return;
                e.currentTarget.setPointerCapture(e.pointerId);
                const r = e.currentTarget.getBoundingClientRect();
                const x = ((e.clientX - r.left) / r.width) * 100;
                const y = ((e.clientY - r.top) / r.height) * 100;
                setTooSmall(false);
                setDrag({ x0: x, y0: y, x1: x, y1: y });
              }}
              onPointerMove={(e) => {
                if (!drag) return;
                const r = e.currentTarget.getBoundingClientRect();
                const clamp = (v: number) => Math.min(100, Math.max(0, v));
                setDrag({
                  ...drag,
                  x1: clamp(((e.clientX - r.left) / r.width) * 100),
                  y1: clamp(((e.clientY - r.top) / r.height) * 100),
                });
              }}
              onPointerUp={() => {
                if (!drag) return;
                const box = {
                  x: Math.round(Math.min(drag.x0, drag.x1)),
                  y: Math.round(Math.min(drag.y0, drag.y1)),
                  w: Math.round(Math.abs(drag.x1 - drag.x0)),
                  h: Math.round(Math.abs(drag.y1 - drag.y0)),
                };
                setDrag(null);
                if (box.w < MIN_BOX || box.h < MIN_BOX) setTooSmall(true);
                else onPick?.(box);
              }}
              onPointerCancel={() => setDrag(null)}
            >
              {(() => {
                const live = drag
                  ? {
                      x: Math.min(drag.x0, drag.x1),
                      y: Math.min(drag.y0, drag.y1),
                      w: Math.abs(drag.x1 - drag.x0),
                      h: Math.abs(drag.y1 - drag.y0),
                    }
                  : pick;
                return live ? (
                  <span
                    aria-hidden
                    data-testid="pick-box"
                    className="border-warning pointer-events-none absolute rounded-sm border-2 shadow-[0_0_0_9999px_rgba(0,0,0,0.38)]"
                    style={{ left: `${live.x}%`, top: `${live.y}%`, width: `${live.w}%`, height: `${live.h}%` }}
                  />
                ) : screenName ? (
                  <span
                    aria-hidden
                    className="border-primary pointer-events-none absolute inset-0 border-2 border-dashed"
                  />
                ) : null;
              })()}
            </div>
          ) : (
            <div className="grid h-full place-items-center px-6 text-center">
              <div className="max-w-[16rem]">
                {status === "starting" ? (
                  <Loader2 aria-hidden className="mx-auto mb-4 size-8 animate-spin opacity-50" />
                ) : (
                  <Smartphone aria-hidden className="mx-auto mb-4 size-8 opacity-40" strokeWidth={1.5} />
                )}
                <p className="t-heading">
                  {onDevice && active
                    ? t("preview.onDevice", { device: state.target })
                    : status === "starting"
                      ? t("preview.status.starting")
                      : t("preview.emptyTitle")}
                </p>
                <p className="t-small mt-1 opacity-70">
                  {onDevice && active
                    ? t("preview.onDeviceBody")
                    : status === "starting"
                      ? t("preview.startingBody")
                      : t("preview.emptyBody")}
                </p>
              </div>
            </div>
          )}
        </PhoneFrame>
      </div>

      {problems.length > 0 && (
        <div role="alert" className="bg-destructive/8 border-destructive/30 shrink-0 border-t p-3">
          <div className="flex flex-wrap items-center gap-3">
            <AlertTriangle aria-hidden className="text-destructive size-4 shrink-0" />
            <div className="min-w-0 flex-1">
              <p className="t-body text-destructive font-medium">
                {t("preview.problem.title", { count: problems.length })}
              </p>
              <p className="t-small text-foreground/80 truncate" title={problems[0]!.message}>
                {problems[0]!.file
                  ? `${t("preview.problem.where", { file: problems[0]!.file, line: problems[0]!.line })} · `
                  : ""}
                {problems[0]!.message}
              </p>
            </div>
            <Button
              size="sm"
              disabled={chatRunning}
              title={chatRunning ? t("preview.problem.busy") : undefined}
              onClick={fixWithClaude}
            >
              {t("preview.problem.fix")}
            </Button>
          </div>
        </div>
      )}

      {(status !== "stopped" || log.length > 0) && (
        <div className="shrink-0 border-t px-3 py-1.5">
          <button
            type="button"
            aria-expanded={showLog}
            onClick={() => setShowLog((v) => !v)}
            className="text-muted-foreground hover:text-foreground t-small rounded underline-offset-2 hover:underline"
          >
            {showLog ? t("preview.problem.hideLog") : t("preview.problem.showLog")}
          </button>
          {showLog && (
            <pre
              tabIndex={0}
              aria-label={t("preview.log")}
              className="bg-muted t-small mt-1.5 max-h-36 overflow-auto rounded-lg p-2 font-mono whitespace-pre-wrap"
            >
              {log.slice(-80).join("\n") || "…"}
            </pre>
          )}
        </div>
      )}

      <DeviceDialog
        open={devices}
        onOpenChange={setDevices}
        onRun={(d: Device) => {
          setDevices(false);
          void start(d.id);
        }}
      />
    </div>
  );
}

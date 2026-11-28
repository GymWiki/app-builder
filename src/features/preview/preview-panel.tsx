import { AlertTriangle, FlaskConical, Link2Off, Loader2, RefreshCw, Smartphone } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { StatusBadge, type Tone } from "@/components/ds/status-badge";
import { useChat } from "@/features/chat/chat-store";
import { PhoneFrame, type DeviceId } from "@/features/project/phone-frame";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Device, PreviewStatus, ScreenNode } from "@/lib/types";
import { AskButton } from "@/features/sections/shared";
import { Skeleton } from "@/components/ui/skeleton";
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

/** Een label alleen bij een afwijking: bijwerken of fout. Als alles loopt (of nog opstart) staat er niets. */
const TONE: Partial<Record<PreviewStatus, Tone>> = { reloading: "info", error: "danger" };

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

export function PreviewPanel({
  projectId,
  device,
  dark,
  screen,
  nodes,
  pick,
  onPick,
  selection,
  selecting,
  onTestrun,
}: {
  projectId: string;
  device: DeviceId;
  dark: boolean;
  /** De gekozen pagina; `route` is het pad dat de preview opent (met voorbeeldwaarden), leeg = niet verbonden. */
  screen?: { class: string; route?: string | null } | null;
  /** Alle pagina's van de app (voor het op de achtergrond vastleggen van schermafbeeldingen). */
  nodes?: ScreenNode[];
  /** Het gekozen element (positie in procenten van het telefoonscherm), of `null` = de hele pagina. */
  pick?: PickBox | null;
  onPick?: (p: PickBox | null) => void;
  /** Wat de widget inspector in het vak vond (alleen in inspect-modus) en of dat nog wordt opgezocht. */
  selection?: Selection | null;
  selecting?: boolean;
  /** "Start testrun" (de AI-testrun); klikken in de app zelf doet niets, dat is hiervoor gereserveerd. */
  onTestrun?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const state = usePreview((s) => s.state);
  const frame = usePreview((s) => s.frame);
  const inspect = state.mode === "inspect";
  const mine = state.projectId === projectId;
  const status: PreviewStatus = mine ? state.status : "stopped";
  const route = screen?.route ?? null;
  const screenName = screen?.class ?? null;
  const storeRoute = usePreview((s) => s.route);
  const crawling = usePreview((s) => s.crawling);
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

  const start = (target: string) => api.previewStart(projectId, target).catch((e) => toastError(e, t));
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
  const live = active && !onDevice && !!state.url;
  const routeOk = !!route && storeRoute === route;

  // ── de pagina die in de lijst staat, staat ook in de preview ──
  // Navigeren gaat zonder herladen (hash van de web-preview); na een hot restart blijft de route staan, dus alleen als hij afwijkt.
  useEffect(() => {
    if (!inspect || !live || !route || crawling || status !== "running") return;
    if (usePreview.getState().route === route) return;
    usePreview.setState({ navAt: Date.now() });
    void api.previewNavigate(route).catch(() => undefined);
  }, [inspect, live, route, crawling, status, state.reloads]);

  // ── opgeslagen schermafbeelding per pagina: meteen zichtbaar, daarna vervangen door de live preview ──
  const [shot, setShot] = useState<{ cls: string; data: string | null } | null>(null);
  const [liveFor, setLiveFor] = useState<string | null>(null);
  const shotFor = shot && shot.cls === screenName ? shot.data : null;
  useEffect(() => {
    if (!screenName || !inspect) return;
    let cancelled = false;
    void api
      .previewShotGet(projectId, screenName)
      .then((d) => !cancelled && setShot({ cls: screenName, data: d }))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [projectId, screenName, inspect]);

  const saveShot = (cls: string, frameData: string | null) => {
    if (!frameData) return;
    const comma = frameData.indexOf(",");
    void api
      .previewShotSave(projectId, cls, frameData.slice(comma + 1))
      .then(() => setShot((cur) => (cur && cur.cls !== cls ? cur : { cls, data: frameData })))
      .catch(() => undefined);
  };
  // Zodra de pagina echt getekend is (route bereikt, even rust) wordt hij live getoond en de afbeelding bijgewerkt.
  // Ook na elke hot reload opnieuw, zodat de opgeslagen afbeelding bij de code blijft passen.
  const hasFrame = frame !== null;
  useEffect(() => {
    if (!inspect || !live || !screenName || !routeOk || !hasFrame || crawling) return;
    const id = setTimeout(() => {
      setLiveFor(screenName);
      saveShot(screenName, usePreview.getState().frame);
    }, 800);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inspect, live, screenName, routeOk, hasFrame, crawling, state.reloads]);

  // ── de andere pagina's één voor één op de achtergrond vastleggen (alleen als niemand iets doet) ──
  const crawled = useRef<Set<string>>(new Set());
  const nodesKey = (nodes ?? []).map((n) => `${n.class}=${n.previewRoute ?? n.route ?? ""}`).join("|");
  const firstLive = liveFor !== null;
  useEffect(() => {
    if (!inspect || !live || !firstLive || chatRunning || !nodes || status !== "running") return;
    let cancelled = false;
    const seen = crawled.current;
    const mine: string[] = [];
    void (async () => {
      await sleep(1500);
      if (cancelled) return;
      const have = new Set(await api.previewShotsList(projectId).catch(() => [] as string[]));
      const todo = nodes.filter((n) => n.route && !have.has(n.class) && !crawled.current.has(n.class));
      if (todo.length === 0 || cancelled) return;
      usePreview.setState({ crawling: true });
      for (const n of todo) {
        if (cancelled) break;
        const target = n.previewRoute ?? n.route!;
        crawled.current.add(n.class);
        mine.push(n.class);
        usePreview.setState({ navAt: Date.now() });
        await api.previewNavigate(target).catch(() => undefined);
        await sleep(2200);
        if (cancelled) break;
        if (usePreview.getState().route === target) {
          saveShot(n.class, usePreview.getState().frame);
          mine.pop();
        }
      }
      if (!cancelled) usePreview.setState({ crawling: false });
    })();
    return () => {
      cancelled = true;
      mine.forEach((c) => seen.delete(c));
      usePreview.setState({ crawling: false });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inspect, live, firstLive, chatRunning, nodesKey, status, projectId, screenName]);

  // Wat er in het telefoonscherm staat: live als de pagina klaar is, anders de opgeslagen afbeelding, anders een skeleton.
  const isLive = inspect && live && !crawling && liveFor === screenName && routeOk && frame !== null;
  const unlinked = !!screenName && !route;
  const failure = mine ? state.failure : null;
  const noApp = !live && !onDevice;
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center gap-2 border-b px-3">
        {TONE[status] && <StatusBadge tone={TONE[status]!}>{t(`preview.status.${status}`)}</StatusBadge>}
        <div className="flex-1" />
        <Button size="sm" variant="outline" onClick={() => setDevices(true)}>
          <Smartphone /> {t("preview.realDevice")}
        </Button>
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
        <Button size="sm" onClick={onTestrun} disabled={!onTestrun}>
          <FlaskConical /> {t("preview.testrun")}
        </Button>
      </div>

      {onDevice && active && (
        <div role="status" className="t-small flex shrink-0 items-center gap-2 border-b bg-accent/60 px-3 py-2">
          <span className="min-w-0 flex-1">{t("preview.onDevice", { device: state.target })}</span>
          <Button size="xs" variant="outline" onClick={() => void start("web")}>
            {t("preview.backToComputer")}
          </Button>
        </div>
      )}

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
          {inspect && live && !unlinked ? (
            isLive ? (
              <img
                src={frame!}
                alt=""
                draggable={false}
                className="pointer-events-none absolute inset-0 size-full bg-white select-none"
              />
            ) : shotFor ? (
              <>
                <img
                  src={shotFor}
                  alt=""
                  data-testid="stored-shot"
                  draggable={false}
                  className="pointer-events-none absolute inset-0 size-full bg-white select-none"
                />
                {!crawling && (
                  <Loader2
                    aria-label={t("preview.loadingPage")}
                    className="absolute top-2 right-2 z-20 size-4 animate-spin opacity-40"
                  />
                )}
              </>
            ) : (
              <PageSkeleton name={screenName} title={t("preview.loadingPage")} />
            )
          ) : null}
          {!inspect && live && !unlinked && state.url ? (
            <iframe
              key={frameKey}
              src={route ? `${state.url.replace(/#.*$/, "")}#${route}` : state.url}
              title={t("preview.title")}
              className="absolute inset-0 size-full border-0 bg-white"
              sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
            />
          ) : null}
          {live && !unlinked ? (
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
          ) : null}
          {unlinked && !onDevice ? (
            <div data-testid="not-linked" className="grid h-full place-items-center px-6 text-center">
              <div className="max-w-[16rem]">
                <Link2Off aria-hidden className="mx-auto mb-4 size-8 opacity-50" strokeWidth={1.5} />
                <p className="t-heading">{t("preview.notLinked.title")}</p>
                <p className="t-small mt-1 mb-3 opacity-70">{t("preview.notLinked.body", { screen: screenName })}</p>
                <AskButton
                  projectId={projectId}
                  prompt={t("preview.notLinked.prompt", { screen: screenName })}
                  label={t("preview.notLinked.connect")}
                />
              </div>
            </div>
          ) : null}
          {noApp && !unlinked ? (
            failure || status === "error" ? (
              <div data-testid="preview-failure" className="grid h-full place-items-center px-6 text-center">
                <div className="max-w-[16rem]">
                  <AlertTriangle aria-hidden className="text-destructive mx-auto mb-4 size-8" strokeWidth={1.5} />
                  <p className="t-heading">
                    {failure?.code === "flutter_missing"
                      ? t("preview.failure.flutterMissing")
                      : t(status === "error" && !failure ? "preview.failure.startFailed" : "preview.failure.crashed")}
                  </p>
                  <p className="t-small mt-1 mb-3 opacity-70">
                    {failure?.code === "flutter_missing"
                      ? t("preview.failure.flutterMissingBody")
                      : t("preview.failure.startFailedBody")}
                  </p>
                  {failure?.code === "flutter_missing" ? (
                    <Button size="sm" asChild>
                      <Link to="/setup">{t("preview.failure.check")}</Link>
                    </Button>
                  ) : (
                    <Button size="sm" disabled={chatRunning} onClick={fixWithClaude}>
                      {t("preview.failure.fix")}
                    </Button>
                  )}
                </div>
              </div>
            ) : (
              <PageSkeleton name={screenName} title={t("preview.loadingPage")} />
            )
          ) : null}
          {onDevice && active ? (
            <div className="grid h-full place-items-center px-6 text-center">
              <div className="max-w-[16rem]">
                <Smartphone aria-hidden className="mx-auto mb-4 size-8 opacity-40" strokeWidth={1.5} />
                <p className="t-small opacity-70">{t("preview.onDeviceBody")}</p>
              </div>
            </div>
          ) : null}
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

      {(status === "error" || problems.length > 0 || log.length > 0) && (
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

/** Neutrale plek in het telefoonscherm zolang er nog geen afbeelding of live preview is: de naam van de pagina en een rustig laadbeeld. */
function PageSkeleton({ name, title }: { name: string | null; title: string }) {
  return (
    <div role="status" data-testid="page-skeleton" className="absolute inset-0 flex flex-col bg-white text-neutral-700">
      <div className="flex h-14 items-center bg-neutral-100 px-4">
        <span className="t-heading truncate">{name}</span>
      </div>
      <div className="flex-1 space-y-3 p-4">
        <Skeleton className="h-6 w-2/3" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-5/6" />
        <Skeleton className="h-10 w-1/2" />
      </div>
      <p className="t-small flex items-center justify-center gap-2 pb-6 opacity-70">
        <Loader2 aria-hidden className="size-4 animate-spin" /> {title}
      </p>
    </div>
  );
}

import { useQuery } from "@tanstack/react-query";
import { ArrowLeft, FolderSearch, ListTree, PanelLeft, PanelRight, Settings, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useParams } from "react-router";
import { ChatPanel, type ChatContext } from "@/features/chat/chat-panel";
import { ScreensTree, type PickedScreen } from "./screens-tree";
import { ThemePanel } from "@/features/sections/theme-panel";
import { DataView } from "@/features/sections/data-view";
import { AssetsView } from "@/features/sections/assets-view";
import { SettingsView } from "@/features/sections/settings-view";
import { VersionsView } from "@/features/sections/versions-view";
import { IntegrationsView } from "@/features/integrations/integrations-view";
import { SecretsView } from "@/features/integrations/secrets-view";
import type { JumpTarget } from "@/features/integrations/use-integrations";
import { PublishView, type PublishTab } from "@/features/sections/publish/publish-view";
import { useChecklist } from "@/features/sections/publish/use-checklist";
import { AutoRetest } from "@/features/sections/publish/auto-retest";
import { PreviewPanel } from "@/features/preview/preview-panel";
import { Group, Panel, Separator, useDefaultLayout, usePanelRef } from "react-resizable-panels";
import { EmptyState } from "@/components/ds/empty-state";
import { Panel as Pane } from "@/components/ds/card";
import { Spinner } from "@/components/ds/spinner";
import { StatusBadge } from "@/components/ds/status-badge";
import { Logo } from "@/components/layout/top-bar";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { qk } from "@/hooks";
import { api, onEvent } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { DEVICES, type DeviceId } from "./phone-frame";
import { SECTIONS, type SectionId } from "./sections";
import { Moon, Sun } from "lucide-react";

const STATUS_TONE = { draft: "neutral", testing: "info", ready: "success", published: "accent" } as const;

function safeStorage(): Pick<Storage, "getItem" | "setItem"> {
  try {
    localStorage.getItem("x");
    return localStorage;
  } catch {
    const m = new Map<string, string>();
    return { getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v) };
  }
}

export default function ProjectPage() {
  const { t } = useTranslation();
  const { id = "" } = useParams();
  const project = useQuery({ queryKey: qk.project(id), queryFn: () => api.projectGet(id) });
  const [section, setSection] = useState<SectionId>("screens");
  const [device, setDevice] = useState<DeviceId>("medium");
  const [previewDark, setPreviewDark] = useState(false);
  const [screen, setScreen] = useState<PickedScreen | null>(null);
  const [contextOff, setContextOff] = useState(false);
  const [publishTab, setPublishTab] = useState<PublishTab>("checklist");
  const [jump, setJump] = useState<JumpTarget | null>(null);
  const checklist = useChecklist(id);
  const evalDone = checklist.data?.done ?? 0;
  const evalTotal = checklist.data?.total ?? 9;
  const canPublish = checklist.data?.canPublish ?? false;

  const treeRef = usePanelRef();
  const chatRef = usePanelRef();
  const [menuHidden, setMenuHidden] = useState(false);
  const [treeHidden, setTreeHidden] = useState(false);
  const [chatHidden, setChatHidden] = useState(false);
  const layout = useDefaultLayout({ id: "studio-screens-layout", storage: safeStorage() });

  // Bijhouden wanneer het project voor het laatst is geopend.
  useEffect(() => {
    if (id) void api.projectOpen(id).catch(() => undefined);
  }, [id]);

  // Springen naar een kaart (Integraties) of een kopje/veld (Geheimen): vanuit de checklist, een knop of een verzoek van Claude.
  const jumpTo = (kind: "integrations" | "secrets", integration: string, field?: string) => {
    setJump({ integration, field, nonce: Date.now() });
    setSection(kind);
  };
  useEffect(() => {
    // Claude kan de gebruiker vragen een geheim in te vullen (MCP-tool); Claude ziet de waarde nooit.
    const un = onEvent<{ projectId: string; integration: string; field: string; integrationOn: boolean }>(
      "open-secret",
      (e) => {
        if (e.projectId === id) {
          setJump({ integration: e.integration, field: e.integrationOn ? e.field : undefined, nonce: Date.now() });
          setSection(e.integrationOn ? "secrets" : "integrations");
        }
      },
    );
    return () => void un.then((f) => f());
  }, [id]);
  /** Bestemmingen uit de checklist: `secrets:<id>`, `integrations:<id>`, `leaks` of een gewone sectie. */
  const goto = (dest: string) => {
    if (dest.startsWith("secrets:")) return jumpTo("secrets", dest.slice(8));
    if (dest.startsWith("integrations:")) return jumpTo("integrations", dest.slice(13));
    if (dest === "leaks") return jumpTo("secrets", "leaks");
    setSection(dest === "preview" ? "screens" : (dest as SectionId));
  };

  if (project.isPending) {
    return (
      <div className="grid h-full place-items-center">
        <Spinner className="size-6" label={t("common.loading")} />
      </div>
    );
  }
  if (project.isError) {
    const f = friendlyError(project.error, t);
    return (
      <div className="grid h-full place-items-center">
        <EmptyState
          icon={FolderSearch}
          title={t("workspace.notFound.title")}
          body={`${t("workspace.notFound.body")} ${f.fix}`}
          action={
            <Button asChild>
              <Link to="/">{t("common.back")}</Link>
            </Button>
          }
        />
      </div>
    );
  }

  const p = project.data;
  const onScreens = section === "screens";
  // Wat de gebruiker nu bekijkt gaat automatisch mee naar Claude (een gekozen scherm gaat voor het menu).
  const context: ChatContext | null =
    contextOff || !screen
      ? null
      : {
          label: t("chat.context.screen", { name: screen.class }),
          prompt: `The user selected the screen "${screen.class}" (${screen.file}). "This screen" means that one.`,
        };
  const toggle = (ref: typeof treeRef, hidden: boolean) => (hidden ? ref.current?.expand() : ref.current?.collapse());

  return (
    <div className="flex h-full flex-col">
      {/* ── bovenbalk ── */}
      <header className="bg-background flex h-14 shrink-0 items-center gap-2 border-b px-3">
        <Button asChild variant="ghost" size="icon" aria-label={t("common.back")} title={t("common.back")}>
          <Link to="/">
            <ArrowLeft />
          </Link>
        </Button>
        <Logo className="hidden sm:block" />
        <h1 className="t-heading max-w-[28ch] truncate">{p.name}</h1>
        <StatusBadge tone={STATUS_TONE[p.status]} className="hidden md:inline-flex">
          {t(`status.${p.status}`)}
        </StatusBadge>

        <div className="flex-1" />

        <div
          role="radiogroup"
          aria-label={t("workspace.device.label")}
          className="bg-muted hidden rounded-lg p-0.5 lg:inline-flex"
        >
          {(Object.keys(DEVICES) as DeviceId[]).map((d) => (
            <button
              key={d}
              type="button"
              role="radio"
              aria-checked={device === d}
              onClick={() => setDevice(d)}
              className={cn(
                "t-small h-7 rounded-md px-2.5 font-medium transition-colors duration-[var(--dur)]",
                device === d ? "bg-card text-foreground shadow-soft" : "text-muted-foreground hover:text-foreground",
              )}
            >
              {t(`workspace.device.${d}`)}
            </button>
          ))}
        </div>
        <Button
          variant="ghost"
          size="icon"
          aria-pressed={previewDark}
          aria-label={t("workspace.previewTheme")}
          title={t("workspace.previewTheme")}
          onClick={() => setPreviewDark((d) => !d)}
        >
          {previewDark ? <Moon /> : <Sun />}
        </Button>

        <span className="text-muted-foreground t-small hidden px-1 tabular-nums sm:inline">v{p.version}</span>

        <Tooltip>
          <TooltipTrigger asChild>
            <span tabIndex={canPublish ? -1 : 0} className="inline-flex rounded-lg" aria-describedby="publish-hint">
              <Button
                disabled={!canPublish}
                className="gap-3 rounded-lg"
                onClick={() => {
                  setSection("publish");
                  setPublishTab("release");
                }}
              >
                <Sparkles /> {t("workspace.publish")}
                <span className="flex items-center gap-1.5" aria-hidden>
                  <Progress
                    value={(evalDone / Math.max(1, evalTotal)) * 100}
                    className="bg-primary-foreground/30 h-1.5 w-10"
                  />
                  <span className="t-small tabular-nums">
                    {t("workspace.publishProgress", { done: evalDone, total: evalTotal })}
                  </span>
                </span>
              </Button>
            </span>
          </TooltipTrigger>
          <TooltipContent id="publish-hint">
            {canPublish ? t("sec.publish.allGreen") : t("workspace.publishHint", { done: evalDone, total: evalTotal })}
          </TooltipContent>
        </Tooltip>

        <div className="bg-border mx-1 h-5 w-px" aria-hidden />
        <Button
          variant="ghost"
          size="icon"
          aria-pressed={!menuHidden}
          aria-label={t(menuHidden ? "workspace.panels.showMenu" : "workspace.panels.hideMenu")}
          title={t(menuHidden ? "workspace.panels.showMenu" : "workspace.panels.hideMenu")}
          onClick={() => setMenuHidden((h) => !h)}
        >
          <PanelLeft />
        </Button>
        {onScreens && (
          <>
            <Button
              variant="ghost"
              size="icon"
              aria-pressed={!treeHidden}
              aria-label={t(treeHidden ? "sec.screens.tree.show" : "sec.screens.tree.hide")}
              title={t(treeHidden ? "sec.screens.tree.show" : "sec.screens.tree.hide")}
              onClick={() => toggle(treeRef, treeHidden)}
            >
              <ListTree />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              aria-pressed={!chatHidden}
              aria-label={t(chatHidden ? "workspace.panels.showRight" : "workspace.panels.hideRight")}
              title={t(chatHidden ? "workspace.panels.showRight" : "workspace.panels.hideRight")}
              onClick={() => toggle(chatRef, chatHidden)}
            >
              <PanelRight />
            </Button>
          </>
        )}
        <Button asChild variant="ghost" size="icon" aria-label={t("settings.title")} title={t("settings.title")}>
          <Link to="/settings">
            <Settings />
          </Link>
        </Button>
      </header>

      {/* ── werkruimte: menu | (alleen bij Schermen: pagina-boom | preview | chat) of een volledige pagina ── */}
      <div className="flex min-h-0 flex-1">
        <Pane
          className={cn("bg-sidebar w-56 shrink-0 border-r", menuHidden && "hidden")}
          inert={menuHidden}
          aria-hidden={menuHidden}
        >
          <nav aria-label={t("workspace.menuLabel")} className="flex flex-col gap-0.5 p-2">
            {SECTIONS.map(({ id: sid, icon: Icon }) => (
              <button
                key={sid}
                type="button"
                aria-current={section === sid ? "page" : undefined}
                onClick={() => {
                  setSection(sid);
                  setContextOff(false);
                }}
                className={cn(
                  "t-body flex items-center gap-2.5 rounded-lg px-3 py-2 text-left font-medium transition-colors duration-[var(--dur)]",
                  section === sid
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                <Icon className="size-4 shrink-0" />
                <span className="truncate">{t(`workspace.sections.${sid}.name`)}</span>
              </button>
            ))}
          </nav>
        </Pane>

        {onScreens ? (
          <Group
            orientation="horizontal"
            className="min-h-0 min-w-0 flex-1"
            defaultLayout={layout.defaultLayout}
            onLayoutChanged={layout.onLayoutChanged}
          >
            <Panel
              id="tree"
              panelRef={treeRef}
              defaultSize="22%"
              minSize="15%"
              maxSize="35%"
              collapsible
              collapsedSize={0}
              onResize={(s) => setTreeHidden(s.asPercentage === 0)}
            >
              <Pane className="h-full border-r" aria-label={t("sec.screens.tree.title")} inert={treeHidden}>
                <ScreensTree
                  key={p.id}
                  projectId={p.id}
                  selected={screen?.class ?? null}
                  onSelect={(sc) => {
                    setScreen(sc);
                    setContextOff(false);
                  }}
                />
              </Pane>
            </Panel>
            <ResizeHandle />

            <Panel id="preview" minSize="25%">
              <section aria-label={t("workspace.previewLabel")} className="h-full">
                <PreviewPanel projectId={p.id} device={device} dark={previewDark} route={screen?.route} />
              </section>
            </Panel>
            <ResizeHandle />

            <Panel
              id="chat"
              panelRef={chatRef}
              defaultSize="30%"
              minSize="22%"
              maxSize="50%"
              collapsible
              collapsedSize={0}
              onResize={(s) => setChatHidden(s.asPercentage === 0)}
            >
              <Pane className="h-full border-l" aria-label={t("workspace.chatLabel")} inert={chatHidden}>
                <ChatPanel projectId={p.id} context={context} onDismissContext={() => setContextOff(true)} />
              </Pane>
            </Panel>
          </Group>
        ) : (
          <section
            aria-label={t(`workspace.sections.${section}.name`)}
            className="bg-background min-w-0 flex-1 overflow-y-auto"
          >
            {section === "theme" && (
              <div className="mx-auto max-w-2xl p-6">
                <h2 className="t-heading mb-1">{t("workspace.sections.theme.name")}</h2>
                <p className="t-body text-muted-foreground mb-5">{t("workspace.sections.theme.desc")}</p>
                <ThemePanel projectId={p.id} />
              </div>
            )}
            {section === "data" && <DataView projectId={p.id} />}
            {section === "assets" && <AssetsView projectId={p.id} />}
            {section === "settings" && <SettingsView project={p} />}
            {section === "versions" && <VersionsView project={p} />}
            {section === "integrations" && (
              <IntegrationsView
                projectId={p.id}
                target={jump}
                onAddSecrets={(t2) => jumpTo("secrets", t2.integration, t2.field)}
              />
            )}
            {section === "secrets" && (
              <SecretsView projectId={p.id} target={jump} onGoIntegrations={() => setSection("integrations")} />
            )}
            {section === "publish" && <PublishView project={p} tab={publishTab} onTab={setPublishTab} goto={goto} />}
          </section>
        )}
      </div>
      <AutoRetest projectId={p.id} />
    </div>
  );
}

function ResizeHandle() {
  return (
    <Separator className="bg-border data-[separator=hover]:bg-primary/60 data-[separator=active]:bg-primary focus-visible:bg-primary relative w-px transition-colors duration-[var(--dur)] outline-none after:absolute after:-inset-x-1 after:inset-y-0 focus-visible:w-0.5" />
  );
}

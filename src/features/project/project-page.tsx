import { useQuery } from "@tanstack/react-query";
import { ArrowLeft, FolderSearch, PanelLeft, PanelRight, Settings, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useParams } from "react-router";
import { ChatPanel, type ChatContext } from "@/features/chat/chat-panel";
import type { Screen } from "@/lib/types";
import { ScreensList } from "./screens-list";
import { Group, Panel, Separator, useDefaultLayout, usePanelRef } from "react-resizable-panels";
import { EmptyState } from "@/components/ds/empty-state";
import { Panel as Pane, PanelHeader } from "@/components/ds/card";
import { Spinner } from "@/components/ds/spinner";
import { StatusBadge } from "@/components/ds/status-badge";
import { Logo } from "@/components/layout/top-bar";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { qk } from "@/hooks";
import { api } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { DEVICES, PhoneFrame, type DeviceId } from "./phone-frame";
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
  const [screen, setScreen] = useState<Screen | null>(null);
  const [contextOff, setContextOff] = useState(false);

  const menuRef = usePanelRef();
  const sideRef = usePanelRef();
  const [menuHidden, setMenuHidden] = useState(false);
  const [sideHidden, setSideHidden] = useState(false);
  const layout = useDefaultLayout({ id: "studio-project-layout", storage: safeStorage() });
  const sideLayout = useDefaultLayout({ id: "studio-side-layout", storage: safeStorage() });

  // Bijhouden wanneer het project voor het laatst is geopend.
  useEffect(() => {
    if (id) void api.projectOpen(id).catch(() => undefined);
  }, [id]);

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
  const current = SECTIONS.find((s) => s.id === section)!;
  const sectionNameEn = {
    screens: "Screens",
    data: "Data & login",
    theme: "Theme",
    assets: "Assets",
    settings: "App settings",
    versions: "Versions",
    publish: "Publish",
  }[section];
  // Wat de gebruiker nu bekijkt gaat automatisch mee naar Claude (een gekozen scherm gaat voor het menu).
  const context: ChatContext | null = contextOff
    ? null
    : screen
      ? {
          label: t("chat.context.screen", { name: screen.class }),
          prompt: `The user selected the screen "${screen.class}" (${screen.file}). "This screen" means that one.`,
        }
      : section !== "screens"
        ? {
            label: t("chat.context.section", { name: t(`workspace.sections.${section}.name`) }),
            prompt: `The user is looking at the "${sectionNameEn}" section of Studio.`,
          }
        : null;
  const toggle = (ref: typeof menuRef, hidden: boolean) => (hidden ? ref.current?.expand() : ref.current?.collapse());

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
            <span tabIndex={0} className="inline-flex rounded-lg" aria-describedby="publish-hint">
              <Button disabled className="gap-3 rounded-lg">
                <Sparkles /> {t("workspace.publish")}
                <span className="flex items-center gap-1.5" aria-hidden>
                  <Progress
                    value={(p.checklistDone / Math.max(1, p.checklistTotal)) * 100}
                    className="bg-primary-foreground/30 h-1.5 w-10"
                  />
                  <span className="t-small tabular-nums">
                    {t("workspace.publishProgress", { done: p.checklistDone, total: p.checklistTotal })}
                  </span>
                </span>
              </Button>
            </span>
          </TooltipTrigger>
          <TooltipContent id="publish-hint">
            {t("workspace.publishHint", { done: p.checklistDone, total: p.checklistTotal })}
          </TooltipContent>
        </Tooltip>

        <div className="bg-border mx-1 h-5 w-px" aria-hidden />
        <Button
          variant="ghost"
          size="icon"
          aria-pressed={!menuHidden}
          aria-label={t(menuHidden ? "workspace.panels.showMenu" : "workspace.panels.hideMenu")}
          title={t(menuHidden ? "workspace.panels.showMenu" : "workspace.panels.hideMenu")}
          onClick={() => toggle(menuRef, menuHidden)}
        >
          <PanelLeft />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          aria-pressed={!sideHidden}
          aria-label={t(sideHidden ? "workspace.panels.showRight" : "workspace.panels.hideRight")}
          title={t(sideHidden ? "workspace.panels.showRight" : "workspace.panels.hideRight")}
          onClick={() => toggle(sideRef, sideHidden)}
        >
          <PanelRight />
        </Button>
        <Button asChild variant="ghost" size="icon" aria-label={t("settings.title")} title={t("settings.title")}>
          <Link to="/settings">
            <Settings />
          </Link>
        </Button>
      </header>

      {/* ── werkruimte: menu | preview | eigenschappen + chat ── */}
      <Group
        orientation="horizontal"
        className="min-h-0 flex-1"
        defaultLayout={layout.defaultLayout}
        onLayoutChanged={layout.onLayoutChanged}
      >
        <Panel
          id="menu"
          panelRef={menuRef}
          defaultSize="18%"
          minSize="13%"
          maxSize="30%"
          collapsible
          collapsedSize={0}
          onResize={(s) => setMenuHidden(s.asPercentage === 0)}
        >
          <Pane className="bg-sidebar h-full border-r" inert={menuHidden}>
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
        </Panel>
        <ResizeHandle />

        <Panel id="preview" minSize="30%">
          <section aria-label={t("workspace.previewLabel")} className="h-full">
            <PhoneFrame device={device} dark={previewDark} />
          </section>
        </Panel>
        <ResizeHandle />

        <Panel
          id="side"
          panelRef={sideRef}
          defaultSize="28%"
          minSize="20%"
          maxSize="45%"
          collapsible
          collapsedSize={0}
          onResize={(s) => setSideHidden(s.asPercentage === 0)}
        >
          <Pane className="h-full border-l" aria-label={t("workspace.propertiesLabel")} inert={sideHidden}>
            <Group
              orientation="vertical"
              id="studio-side"
              defaultLayout={sideLayout.defaultLayout}
              onLayoutChanged={sideLayout.onLayoutChanged}
            >
              <Panel id="properties" defaultSize="26%" minSize="12%" collapsible collapsedSize={0}>
                <div className="flex h-full flex-col">
                  <PanelHeader title={t(`workspace.sections.${section}.name`)} />
                  <div className="min-h-0 flex-1 overflow-y-auto p-4">
                    {section === "screens" ? (
                      <ScreensList
                        projectId={p.id}
                        selected={screen?.class ?? null}
                        onSelect={(sc) => {
                          setScreen(sc);
                          setContextOff(false);
                        }}
                      />
                    ) : (
                      <>
                        <p className="t-body text-muted-foreground">{t(`workspace.sections.${section}.desc`)}</p>
                        <div className="bg-muted/50 mt-4 rounded-xl border border-dashed p-4">
                          <p className="t-body flex items-center gap-2 font-medium">
                            <current.icon aria-hidden className="text-primary-text size-4" />{" "}
                            {t("workspace.comingSoon")}
                          </p>
                          <p className="t-small text-muted-foreground mt-1">{t("workspace.comingSoonBody")}</p>
                        </div>
                      </>
                    )}
                  </div>
                </div>
              </Panel>
              <HorizontalHandle />
              <Panel id="chat" minSize="30%">
                <ChatPanel projectId={p.id} context={context} onDismissContext={() => setContextOff(true)} />
              </Panel>
            </Group>
          </Pane>
        </Panel>
      </Group>
    </div>
  );
}

function ResizeHandle() {
  return (
    <Separator className="bg-border data-[separator=hover]:bg-primary/60 data-[separator=active]:bg-primary focus-visible:bg-primary relative w-px transition-colors duration-[var(--dur)] outline-none after:absolute after:-inset-x-1 after:inset-y-0 focus-visible:w-0.5" />
  );
}

function HorizontalHandle() {
  return (
    <Separator className="bg-border data-[separator=hover]:bg-primary/60 data-[separator=active]:bg-primary focus-visible:bg-primary relative h-px transition-colors duration-[var(--dur)] outline-none after:absolute after:inset-x-0 after:-inset-y-1 focus-visible:h-0.5" />
  );
}

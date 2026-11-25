import {
  AlertTriangle,
  Check,
  FilePen,
  Globe,
  Loader2,
  Search,
  ShieldQuestion,
  Sparkles,
  Terminal,
  Wrench,
  X,
  Camera,
  Undo2,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Button } from "@/components/ui/button";
import { useSettings } from "@/hooks";
import type { PermissionDecision, PermissionRequest, ToolKind } from "@/lib/types";
import { cn } from "@/lib/utils";
import type { Item } from "./chat-model";
import { Markdown } from "./markdown";

const TOOL_ICON: Record<ToolKind, typeof Wrench> = {
  read: Search,
  edit: FilePen,
  command: Terminal,
  network: Globe,
  studio: Sparkles,
  other: Wrench,
};

export function UserBubble({ item }: { item: Extract<Item, { kind: "user" }> }) {
  return (
    <div className="flex justify-end">
      <div className="bg-primary text-primary-foreground t-body max-w-[88%] rounded-2xl rounded-br-md px-3.5 py-2 whitespace-pre-wrap">
        {item.text}
      </div>
    </div>
  );
}

export function AssistantBubble({ item }: { item: Extract<Item, { kind: "assistant" }> }) {
  return (
    <div className="flex">
      <div className="bg-muted t-body max-w-[94%] rounded-2xl rounded-bl-md px-3.5 py-2.5">
        <Markdown text={item.text} />
        {item.live && (
          <span
            aria-hidden
            className="bg-foreground/60 ml-0.5 inline-block h-3.5 w-1.5 animate-pulse rounded-sm align-middle"
          />
        )}
      </div>
    </div>
  );
}

export function ToolCard({ item }: { item: Extract<Item, { kind: "tool" }> }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const Icon = TOOL_ICON[item.tool];
  const state = item.status === "running" ? "running" : item.status === "ok" ? "done" : "failed";
  const target = item.summary || item.path || "";
  const expandable = !!(item.result || item.detail) && item.tool !== "edit";
  return (
    <div
      className={cn(
        "bg-card shadow-soft rounded-xl border px-3 py-2",
        item.status === "error" && "border-destructive/40",
      )}
    >
      <div className="flex items-center gap-2.5">
        <span
          className={cn(
            "grid size-6 shrink-0 place-items-center rounded-md",
            item.tool === "edit" ? "bg-accent text-accent-foreground" : "bg-muted text-muted-foreground",
          )}
        >
          <Icon className="size-3.5" aria-hidden />
        </span>
        <span className="t-small min-w-0 flex-1 truncate font-medium" title={target}>
          {t(`chat.tool.${item.tool}.${state}`, { target })}
        </span>
        {item.status === "running" ? (
          <Loader2 className="text-muted-foreground size-3.5 animate-spin" aria-hidden />
        ) : item.status === "ok" ? (
          <Check className="text-success size-3.5" aria-label="" />
        ) : (
          <X className="text-destructive size-3.5" aria-hidden />
        )}
      </div>
      {expandable && (
        <div className="mt-1.5 pl-8.5">
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setOpen((o) => !o)}
            className="text-muted-foreground hover:text-foreground t-small rounded underline-offset-2 hover:underline"
          >
            {t("chat.details")}
          </button>
          {open && (
            <pre className="bg-muted t-small mt-1.5 max-h-40 overflow-auto rounded-lg p-2 font-mono whitespace-pre-wrap">
              {[item.detail, item.result].filter(Boolean).join("\n\n")}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

export function SnapshotChip({ item }: { item: Extract<Item, { kind: "snapshot" }> }) {
  const { t } = useTranslation();
  return (
    <p className="text-muted-foreground t-small flex items-center justify-center gap-1.5">
      <Camera className="size-3.5" aria-hidden /> {t("chat.snapshot", { label: item.label })}
    </p>
  );
}

/** "Scherm|Home" -> weergavenaam; het niveau zelf is alleen in de ontwikkelaarsmodus te zien. */
export function StatusLine({ item }: { item: Extract<Item, { kind: "status" }> }) {
  const { t } = useTranslation();
  const dev = useSettings().data?.dev_mode === "true";
  // Modelkeuze en opschaling zijn alleen voor de ontwikkelaarsmodus: de gewone gebruiker ziet er niets van.
  if (item.code === "model" || item.code === "model_escalated") {
    if (!dev) return null;
    const [cls, tier, model, reason] = (item.detail ?? "").split("|");
    return (
      <p role="status" className="text-muted-foreground t-small text-center font-mono">
        {item.code === "model"
          ? t("chat.status.model", { cls, tier, model: model || "—", reason })
          : t("chat.status.model_escalated", { model: item.detail })}
      </p>
    );
  }
  const [level, screen] = (item.detail ?? "").split("|");
  const name = (screen ?? "").replace(/(Screen|Page)$/, "");
  const code = item.code === "scope" && !name ? "scope_app" : item.code;
  const Icon =
    code === "rolled_back" || code === "scope_reverted"
      ? Undo2
      : code === "checking" || code === "repairing"
        ? Loader2
        : Check;
  return (
    <p role="status" className="text-muted-foreground t-small flex items-center justify-center gap-1.5">
      <Icon className={cn("size-3.5", Icon === Loader2 && "animate-spin")} aria-hidden />
      {t(`chat.status.${code}`, { screen: name })}
      {dev && item.code === "scope" && level ? <span className="font-mono"> · {level}</span> : null}
    </p>
  );
}

export function ErrorCard({ item }: { item: Extract<Item, { kind: "error" }> }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const code = ["not_logged_in", "tool_missing", "outdated"].includes(item.code) ? item.code : "failed";
  const needsSetup = code !== "failed";
  return (
    <div role="alert" className="bg-destructive/8 border-destructive/30 rounded-xl border p-3.5">
      <p className="t-body text-destructive flex items-center gap-2 font-medium">
        <AlertTriangle className="size-4" aria-hidden /> {t(`chat.errors.${code}.title`)}
      </p>
      <p className="t-small text-foreground/80 mt-1">{t(`chat.errors.${code}.fix`)}</p>
      <div className="mt-2.5 flex flex-wrap items-center gap-2">
        {needsSetup && (
          <Button asChild size="sm" variant="outline">
            <Link to="/setup">{t("common.openSetup")}</Link>
          </Button>
        )}
        {item.message && !needsSetup && (
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setOpen((o) => !o)}
            className="text-muted-foreground hover:text-foreground t-small underline-offset-2 hover:underline"
          >
            {open ? t("common.hideDetails") : t("common.showDetails")}
          </button>
        )}
      </div>
      {open && (
        <pre className="bg-muted t-small mt-2 max-h-32 overflow-auto rounded-lg p-2 font-mono whitespace-pre-wrap">
          {item.message}
        </pre>
      )}
    </div>
  );
}

export function PermissionCard({
  req,
  onDecide,
}: {
  req: PermissionRequest;
  onDecide: (d: PermissionDecision) => void;
}) {
  const { t } = useTranslation();
  const known = ["command", "network", "edit", "read", "studio"].includes(req.kind) ? req.kind : "other";
  return (
    <div role="alert" className="border-warning/50 bg-warning/10 rounded-xl border p-3.5">
      <p className="t-body flex items-center gap-2 font-medium">
        <ShieldQuestion className="text-warning size-4.5" aria-hidden /> {t(`chat.permission.title.${known}`)}
      </p>
      <pre className="bg-card t-small mt-2 max-h-28 overflow-auto rounded-lg border p-2 font-mono break-all whitespace-pre-wrap">
        {req.detail ?? req.summary}
      </pre>
      <div className="mt-3 flex flex-wrap gap-2">
        <Button size="sm" autoFocus onClick={() => onDecide("allow_once")}>
          {t("chat.permission.allowOnce")}
        </Button>
        <Button size="sm" variant="outline" onClick={() => onDecide("allow_always")}>
          {t("chat.permission.allowAlways")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => onDecide("deny")}>
          {t("chat.permission.deny")}
        </Button>
      </div>
      <p className="text-muted-foreground t-small mt-2">{t("chat.permission.hint")}</p>
    </div>
  );
}

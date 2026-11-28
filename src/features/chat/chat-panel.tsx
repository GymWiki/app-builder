import { cn } from "@/lib/utils";
import { useQueryClient } from "@tanstack/react-query";
import { Send, SquarePen, Square, Sparkles, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { EmptyState } from "@/components/ds/empty-state";
import { Spinner } from "@/components/ds/spinner";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toastError } from "@/lib/toast";
import { ProfileStatusLine } from "@/features/profile/profile-status-line";
import { AdvancedTerminal } from "./advanced-terminal";
import { useChat } from "./chat-store";
import {
  AssistantBubble,
  ErrorCard,
  PermissionCard,
  SnapshotChip,
  StatusLine,
  ToolCard,
  UserBubble,
} from "./message-views";

export interface ChatContext {
  /** Leesbare tekst voor de chip. */
  label: string;
  /** Wat er aan de prompt wordt toegevoegd (Engels, voor Claude). */
  prompt: string;
  /** `element` = alleen één onderdeel van een pagina; wordt opvallend getoond zodat niemand per ongeluk te smal vraagt. */
  kind?: "page" | "element" | "scope";
}

const EMPTY: never[] = [];

export function ChatPanel({
  projectId,
  context,
  onDismissContext,
  onScopeSend,
  suggestions,
}: {
  projectId: string;
  context: ChatContext | null;
  onDismissContext: () => void;
  /** Bij een bereik van meerdere pagina's neemt de pagina de opdracht over (Studio kiest de uitvoering); `true` = afgehandeld. */
  onScopeSend?: (text: string) => boolean;
  /** Snelle suggesties passend bij het bereik (standaard de algemene). */
  suggestions?: string[];
}) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const chat = useChat((s) => s.byProject[projectId]);
  const permissions = useChat((s) => s.permissions).filter((p) => p.projectId === projectId);
  const { load, send, cancel, newSession, respond } = useChat.getState();
  const [draft, setDraft] = useState("");
  const [elapsed, setElapsed] = useState(0);
  const log = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  const items = chat?.items ?? EMPTY;
  const running = chat?.running ?? false;

  useEffect(() => {
    void load(projectId).catch((e) => toastError(e, t));
  }, [projectId, load, t]);

  // Na elke beurt opnieuw naar schermen kijken: Claude kan er nieuwe hebben gemaakt.
  const wasRunning = useRef(false);
  useEffect(() => {
    if (wasRunning.current && !running) void qc.invalidateQueries({ queryKey: ["screens", projectId] });
    wasRunning.current = running;
  }, [running, qc, projectId]);

  useEffect(() => {
    if (!running || !chat?.startedAt) return;
    const started = chat.startedAt;
    const tick = () => setElapsed(Math.floor((Date.now() - started) / 1000));
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [running, chat?.startedAt]);

  // Automatisch meescrollen, maar niet als de gebruiker omhoog heeft gescrold.
  useLayoutEffect(() => {
    const el = log.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [items, permissions.length]);

  const submit = async () => {
    const text = draft.trim();
    if (!text || running) return;
    setDraft("");
    stick.current = true;
    if (onScopeSend?.(text)) return;
    try {
      await send(projectId, text, context?.prompt ?? null, i18n.language.slice(0, 2));
    } catch (e) {
      setDraft(text);
      toastError(e, t);
    }
  };

  return (
    <Tabs defaultValue="chat" className="flex min-h-0 flex-1 flex-col gap-0">
      <div className="flex h-11 shrink-0 items-center justify-between gap-2 border-b px-3">
        <TabsList className="h-8 rounded-lg">
          <TabsTrigger value="chat" className="rounded-md px-3 text-xs">
            {t("chat.tabChat")}
          </TabsTrigger>
          <TabsTrigger value="advanced" className="rounded-md px-3 text-xs">
            {t("chat.tabAdvanced")}
          </TabsTrigger>
        </TabsList>
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={running || items.length === 0}
          aria-label={t("chat.newConversation")}
          title={t("chat.newConversation")}
          onClick={() => void newSession(projectId).catch((e) => toastError(e, t))}
        >
          <SquarePen />
        </Button>
      </div>

      <TabsContent value="chat" className="flex min-h-0 flex-1 flex-col">
        <ProfileStatusLine projectId={projectId} />
        <div
          ref={log}
          role="log"
          aria-live="polite"
          aria-label={t("chat.title")}
          tabIndex={0}
          onScroll={(e) => {
            const el = e.currentTarget;
            stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 60;
          }}
          className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3"
        >
          {items.length === 0 && permissions.length === 0 ? (
            <div className="flex h-full flex-col">
              <EmptyState icon={Sparkles} title={t("chat.empty.title")} body={t("chat.empty.body")} className="py-6" />
              <div className="mt-auto flex flex-col gap-2">
                {(suggestions ?? (t("chat.suggestions", { returnObjects: true }) as string[])).map((s) => (
                  <button
                    key={s}
                    type="button"
                    onClick={() => setDraft(s)}
                    className="hover:bg-muted t-small rounded-xl border px-3 py-2 text-left transition-colors duration-[var(--dur)]"
                  >
                    {s}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            items.map((it) => {
              switch (it.kind) {
                case "user":
                  return <UserBubble key={it.key} item={it} />;
                case "assistant":
                  return <AssistantBubble key={it.key} item={it} />;
                case "tool":
                  return <ToolCard key={it.key} item={it} />;
                case "snapshot":
                  return <SnapshotChip key={it.key} item={it} />;
                case "status":
                  return <StatusLine key={it.key} item={it} />;
                case "error":
                  return <ErrorCard key={it.key} item={it} />;
              }
            })
          )}
          {permissions.map((p) => (
            <PermissionCard key={p.id} req={p} onDecide={(d) => void respond(p.id, d).catch((e) => toastError(e, t))} />
          ))}
        </div>

        <div className="shrink-0 border-t p-3">
          {running && (
            <div className="mb-2 flex items-center justify-between" role="status">
              <span className="text-muted-foreground t-small inline-flex items-center gap-2">
                <Spinner className="size-3.5" /> {t("chat.working")} <span className="tabular-nums">{elapsed}s</span>
              </span>
              <Button size="xs" variant="outline" onClick={() => void cancel(projectId)}>
                <Square className="fill-current" /> {t("chat.stop")}
              </Button>
            </div>
          )}
          {context && (
            <span
              data-kind={context.kind ?? "page"}
              className={cn(
                "t-small mb-2 inline-flex max-w-full items-center gap-1 rounded-full py-0.5 pr-1 pl-2.5 font-medium",
                context.kind === "element"
                  ? "border-warning bg-warning/15 text-foreground border"
                  : "bg-accent text-accent-foreground",
              )}
            >
              <span className="truncate">{context.label}</span>
              <button
                type="button"
                onClick={onDismissContext}
                aria-label={t("chat.context.remove")}
                title={t("chat.context.remove")}
                className="hover:bg-accent-foreground/15 grid size-5 place-items-center rounded-full"
              >
                <X className="size-3" />
              </button>
            </span>
          )}
          <div className="bg-card focus-within:border-ring flex items-end gap-2 rounded-xl border p-2 transition-colors duration-[var(--dur)]">
            <textarea
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                  e.preventDefault();
                  void submit();
                }
              }}
              rows={2}
              aria-label={t("chat.title")}
              aria-describedby="chat-send-hint"
              placeholder={t("chat.placeholder")}
              className="placeholder:text-muted-foreground t-body max-h-40 min-h-0 flex-1 resize-none bg-transparent px-1 outline-none"
            />
            <Button
              size="icon-sm"
              disabled={!draft.trim() || running}
              onClick={() => void submit()}
              aria-label={t("chat.send")}
              title={t("chat.send")}
            >
              <Send />
            </Button>
          </div>
          <p id="chat-send-hint" className="text-muted-foreground t-small mt-1.5">
            {t("chat.sendHint")}
          </p>
        </div>
      </TabsContent>

      <TabsContent value="advanced" className="flex min-h-0 flex-1 flex-col">
        <AdvancedTerminal projectId={projectId} />
      </TabsContent>
    </Tabs>
  );
}

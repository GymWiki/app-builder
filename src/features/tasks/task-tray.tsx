import { AlertTriangle, CheckCircle2, Loader2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useChat } from "@/features/chat/chat-store";
import { useTasks, type Task } from "./task-store";

function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(since);
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  const s = Math.max(0, Math.floor((now - since) / 1000));
  return <span className="tabular-nums">{`${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`}</span>;
}

/** Wat Claude nu doet, in gewone taal: de laatste handeling uit de chat. */
function useStep(task: Task): string | null {
  const items = useChat((s) => s.byProject[task.projectId]?.items);
  const tools = (items ?? []).slice(task.from).filter((i) => i.kind === "tool");
  const last = tools[tools.length - 1];
  return last && last.kind === "tool" ? last.summary : null;
}

function ScopeRow({ task }: { task: Task }) {
  const { t, i18n } = useTranslation();
  const { cancel, dismiss, undo, retryFailed } = useTasks();
  const r = task.scope!;
  const running = task.status === "running";
  const pct = r.total ? Math.round((r.done / r.total) * 100) : 0;
  const text = running
    ? task.cancelled
      ? t("tasks.stopping")
      : r.current === "shared"
        ? t("scope.sharedStep")
        : r.current === "plan"
          ? t("scope.planStep")
          : r.strategy === "fanout"
            ? t("scope.pageOf", { n: Math.min(r.done + 1, r.total), total: r.total })
            : t("tasks.working")
    : task.status === "cancelled"
      ? t("tasks.cancelled")
      : r.undone
        ? t("scope.undone")
        : t("scope.summary", { pages: r.changed.length, shared: r.sharedDone ? 1 : 0, count: r.changed.length });
  return (
    <li className="bg-card shadow-soft space-y-2 rounded-xl border p-3" data-testid="scope-task">
      <div className="flex items-start gap-3">
        {running ? (
          <Loader2 aria-hidden className="text-primary mt-0.5 size-4 shrink-0 animate-spin" />
        ) : r.failed.length ? (
          <AlertTriangle aria-hidden className="text-warning mt-0.5 size-4 shrink-0" />
        ) : (
          <CheckCircle2 aria-hidden className="text-success mt-0.5 size-4 shrink-0" />
        )}
        <div className="min-w-0 flex-1">
          <p className="t-body font-medium">{task.title}</p>
          <p className="t-small text-muted-foreground" aria-live="polite">
            {text}
          </p>
        </div>
        {running ? (
          <Button size="xs" variant="ghost" disabled={task.cancelled} onClick={() => void cancel(task.id)}>
            {t("tasks.cancel")}
          </Button>
        ) : (
          <Button size="icon-sm" variant="ghost" aria-label={t("tasks.dismiss")} onClick={() => dismiss(task.id)}>
            <X />
          </Button>
        )}
      </div>
      {running && r.strategy === "fanout" && <Progress value={pct} className="h-1.5" />}
      {!running && r.failed.length > 0 && (
        <div className="space-y-1.5">
          <p className="t-small">{t("scope.failedPages", { pages: r.failed.join(", ") })}</p>
          <Button size="xs" variant="outline" onClick={() => void retryFailed(task.id, i18n.language.slice(0, 2))}>
            {t("scope.retryFailed")}
          </Button>
        </div>
      )}
      {!running && r.snapshot && !r.undone && (
        <Button size="xs" variant="outline" onClick={() => void undo(task.id)}>
          {t("scope.undo")}
        </Button>
      )}
    </li>
  );
}

function Row({ task }: { task: Task }) {
  const { t } = useTranslation();
  const step = useStep(task);
  const { cancel, retry, dismiss } = useTasks();
  const running = task.status === "running";
  return (
    <li className="bg-card shadow-soft flex items-start gap-3 rounded-xl border p-3">
      {running ? (
        <Loader2 aria-hidden className="text-primary mt-0.5 size-4 shrink-0 animate-spin" />
      ) : task.status === "done" ? (
        <CheckCircle2 aria-hidden className="text-success mt-0.5 size-4 shrink-0" />
      ) : (
        <AlertTriangle aria-hidden className="text-destructive mt-0.5 size-4 shrink-0" />
      )}
      <div className="min-w-0 flex-1">
        <p className="t-body font-medium">
          {task.title}
          {running && (
            <span className="text-muted-foreground t-small ml-2 font-normal">
              <Elapsed since={task.startedAt} />
            </span>
          )}
        </p>
        <p className="t-small text-muted-foreground truncate" title={step ?? undefined}>
          {running
            ? task.cancelled
              ? t("tasks.stopping")
              : (step ?? t("tasks.working"))
            : task.status === "done"
              ? t("tasks.done")
              : task.status === "cancelled"
                ? t("tasks.cancelled")
                : ["not_logged_in", "tool_missing", "outdated"].includes(task.errorCode ?? "")
                  ? t(`chat.errors.${task.errorCode}.fix`)
                  : t("tasks.failed")}
        </p>
        {!running && task.status !== "done" && (
          <Button size="xs" variant="outline" className="mt-2" onClick={() => void retry(task.id)}>
            {t("tasks.retry")}
          </Button>
        )}
      </div>
      {running ? (
        <Button size="xs" variant="ghost" disabled={task.cancelled} onClick={() => void cancel(task.id)}>
          {t("tasks.cancel")}
        </Button>
      ) : (
        <Button size="icon-sm" variant="ghost" aria-label={t("tasks.dismiss")} onClick={() => dismiss(task.id)}>
          <X />
        </Button>
      )}
    </li>
  );
}

/** Achtergrondtaken van dit project: voortgang, annuleren en de uitkomst, op elke pagina (ook waar de chat niet staat). */
export function TaskTray({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const all = useTasks((s) => s.tasks);
  const tasks = all.filter((x) => x.projectId === projectId);
  if (tasks.length === 0) return null;
  return (
    <ul
      aria-label={t("tasks.title")}
      aria-live="polite"
      data-testid="task-tray"
      className="pointer-events-none fixed right-4 bottom-4 z-40 w-80 max-w-[calc(100vw-2rem)] space-y-2 [&>li]:pointer-events-auto"
    >
      {tasks.map((x) => (
        x.scope ? <ScopeRow key={x.id} task={x} /> : <Row key={x.id} task={x} />
      ))}
    </ul>
  );
}

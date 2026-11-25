import { Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useChat } from "@/features/chat/chat-store";
import { cn } from "@/lib/utils";
import type { Applied } from "@/lib/types";
import { toastError } from "@/lib/toast";
import { toast } from "sonner";

export const inputCls =
  "bg-card placeholder:text-muted-foreground focus-visible:border-ring h-10 w-full rounded-lg border px-3 t-body outline-none transition-colors duration-[var(--dur)] disabled:opacity-60";

/** "Laat Claude dit doen": stuurt een vooraf geschreven opdracht naar de chat van dit project. */
export function useAsk(projectId: string) {
  const { t, i18n } = useTranslation();
  const running = useChat((s) => s.byProject[projectId]?.running ?? false);
  const ask = (prompt: string) => {
    void useChat
      .getState()
      .send(projectId, prompt, null, i18n.language.slice(0, 2))
      .then(() => toast.success(t("chat.title")))
      .catch((e) => toastError(e, t));
  };
  return { ask, busy: running };
}

export function AskButton({
  projectId,
  prompt,
  label,
  variant = "default",
}: {
  projectId: string;
  prompt: string;
  label?: string;
  variant?: "default" | "outline" | "secondary";
}) {
  const { t } = useTranslation();
  const { ask, busy } = useAsk(projectId);
  return (
    <Button
      size="sm"
      variant={variant}
      disabled={busy}
      title={busy ? t("sec.common.askBusy") : undefined}
      onClick={() => ask(prompt)}
    >
      <Sparkles /> {label ?? t("sec.common.ask")}
    </Button>
  );
}

export function SectionPage({
  title,
  intro,
  children,
  className,
}: {
  title: string;
  intro?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className="h-full overflow-y-auto">
      <div className={cn("mx-auto max-w-3xl space-y-6 px-6 py-6", className)}>
        <header>
          <h2 className="t-title">{title}</h2>
          {intro && <p className="text-muted-foreground t-body mt-1">{intro}</p>}
        </header>
        {children}
      </div>
    </div>
  );
}

export function Field({
  label,
  help,
  htmlFor,
  children,
}: {
  label: string;
  help?: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <label htmlFor={htmlFor} className="t-body block font-medium">
        {label}
      </label>
      {children}
      {help && <p className="text-muted-foreground t-small">{help}</p>}
    </div>
  );
}

export function Check({
  id,
  checked,
  onChange,
  label,
  help,
  disabled,
}: {
  id: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  help?: string;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-start gap-3">
      <input
        id={id}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
        className="accent-primary mt-1 size-4 shrink-0 rounded"
      />
      <label htmlFor={id} className="t-body">
        {label}
        {help && <span className="text-muted-foreground t-small block">{help}</span>}
      </label>
    </div>
  );
}

/** Resultaat van het schrijven naar platformbestanden: wat is aangepast, wat niet. */
export function AppliedNotice({
  result,
  projectId,
  askPrompt,
}: {
  result: Applied | null;
  projectId: string;
  askPrompt: string;
}) {
  const { t } = useTranslation();
  if (!result) return null;
  return (
    <div role="status" className="space-y-2">
      {result.applied.length > 0 && (
        <p className="text-success t-small font-medium">
          {t("sec.common.applied", { list: result.applied.join(", ") })}
        </p>
      )}
      {result.skipped.length > 0 && (
        <div className="border-warning/50 bg-warning/10 rounded-xl border p-3">
          <p className="t-body font-medium">{t("sec.common.skippedTitle")}</p>
          <ul className="t-small text-foreground/80 mt-1 list-disc pl-5">
            {result.skipped.map((s) => (
              <li key={s.target}>
                {s.target}: {s.reason}
              </li>
            ))}
          </ul>
          <p className="t-small text-muted-foreground mt-1">{t("sec.common.skippedHelp")}</p>
          <div className="mt-2">
            <AskButton projectId={projectId} prompt={askPrompt} label={t("sec.common.askFix")} variant="outline" />
          </div>
        </div>
      )}
    </div>
  );
}

export function Card2({
  title,
  children,
  className,
}: {
  title?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("bg-card shadow-soft space-y-3 rounded-xl border p-5", className)}>
      {title && <h3 className="t-heading">{title}</h3>}
      {children}
    </section>
  );
}

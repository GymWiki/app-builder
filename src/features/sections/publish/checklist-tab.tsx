import { AlertCircle, CheckCircle2, Circle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import type { ChecklistItemState, Evaluation } from "@/lib/types";
import { cn } from "@/lib/utils";

/** Eén punt: titel, wat er mist en de volgende actie. */
function Item({ it, onNext }: { it: ChecklistItemState; onNext: (n: string) => void }) {
  const { t } = useTranslation();
  const done = it.status === "done";
  const Icon = done ? CheckCircle2 : it.status === "failed" ? AlertCircle : Circle;
  return (
    <li className={cn("bg-card shadow-soft rounded-xl border p-4", done && "opacity-80")}>
      <div className="flex items-start gap-3">
        <Icon
          aria-hidden
          className={cn(
            "mt-0.5 size-5 shrink-0",
            done ? "text-success" : it.status === "failed" ? "text-destructive" : "text-muted-foreground",
          )}
        />
        <div className="min-w-0 flex-1">
          <h4 className="t-body font-semibold">
            {t(`sec.publish.items.${it.key}.title`)} <span className="sr-only">{done ? "✓" : ""}</span>
          </h4>
          <p className="t-small text-muted-foreground">{t(`sec.publish.items.${it.key}.desc`)}</p>
          {!done && it.missing.length > 0 && (
            <ul className="t-small mt-2 list-disc space-y-0.5 pl-5">
              {it.missing.map((m) => (
                <li key={m}>{t(`sec.publish.items.${it.key}.missing.${m}`, { defaultValue: m })}</li>
              ))}
            </ul>
          )}
          {!done && it.next && (
            <Button size="xs" variant="outline" className="mt-2.5" onClick={() => onNext(it.next!)}>
              {t(`sec.publish.actions.${it.next.replace(":", "_")}`, { defaultValue: t("sec.publish.goTo") })}
            </Button>
          )}
        </div>
      </div>
    </li>
  );
}

export function ChecklistTab({ evaluation, onNext }: { evaluation: Evaluation; onNext: (n: string) => void }) {
  const { t } = useTranslation();
  const required = evaluation.items.filter((i) => i.blocking);
  const advice = evaluation.items.filter((i) => !i.blocking);
  return (
    <div className="space-y-6">
      <div className="space-y-2" aria-live="polite">
        <div className="flex items-center gap-3">
          <Progress
            value={(evaluation.done / evaluation.total) * 100}
            className="h-2"
            aria-label={t("sec.publish.progress", { done: evaluation.done, total: evaluation.total })}
          />
          <span className="t-small text-muted-foreground font-medium whitespace-nowrap tabular-nums">
            {t("sec.publish.progress", { done: evaluation.done, total: evaluation.total })}
          </span>
        </div>
        {evaluation.canPublish && <p className="text-success t-body font-medium">{t("sec.publish.allGreen")}</p>}
      </div>
      <section>
        <h3 className="t-heading mb-2">{t("sec.publish.required")}</h3>
        <ul className="space-y-2.5">
          {required.map((i) => (
            <Item key={i.key} it={i} onNext={onNext} />
          ))}
        </ul>
      </section>
      <section>
        <h3 className="t-heading mb-2">{t("sec.publish.advice")}</h3>
        <ul className="space-y-2.5">
          {advice.map((i) => (
            <Item key={i.key} it={i} onNext={onNext} />
          ))}
        </ul>
      </section>
    </div>
  );
}

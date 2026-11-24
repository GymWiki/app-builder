import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, Loader2 } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { friendlyError } from "@/lib/errors";
import { api, onEvent } from "@/lib/ipc";
import { log } from "@/lib/log";
import type { TemplateId } from "@/lib/types";
import { cn } from "@/lib/utils";
import { TEMPLATES } from "./templates";

const STEPS = ["prepare", "flutter_create", "template", "memory", "git", "save"] as const;

/** Het formulier begint elke keer leeg doordat de ouder deze component met een nieuwe `key` opent. */
export function NewProjectDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const nameId = useId();
  const descId = useId();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [template, setTemplate] = useState<TemplateId>("blank");
  const [step, setStep] = useState<string | null>(null);

  const create = useMutation({
    mutationFn: () => api.projectCreate({ name: name.trim(), template, description: description.trim() }),
    onSuccess: async (project) => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      onOpenChange(false);
      navigate(`/project/${project.id}`);
    },
    onError: (e) => log("error", `create project failed: ${String(e)}`),
  });

  useEffect(() => {
    if (!open) return;
    const un = onEvent<{ step: string }>("project-create-progress", (p) => setStep(p.step));
    return () => void un.then((f) => f());
  }, [open]);

  const busy = create.isPending;
  const stepIndex = step ? STEPS.indexOf(step as (typeof STEPS)[number]) : -1;
  const error = create.error ? friendlyError(create.error, t) : null;

  return (
    <Dialog open={open} onOpenChange={(o) => !busy && onOpenChange(o)}>
      <DialogContent
        className="rounded-2xl sm:max-w-xl"
        onInteractOutside={(e) => busy && e.preventDefault()}
        onEscapeKeyDown={(e) => busy && e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className="t-title">{busy ? t("newProject.creating") : t("newProject.title")}</DialogTitle>
          <DialogDescription>{busy ? t("newProject.slowHint") : t("newProject.description")}</DialogDescription>
        </DialogHeader>

        {busy ? (
          <div className="space-y-4 py-2" aria-live="polite">
            <Progress value={Math.max(8, ((stepIndex + 1) / STEPS.length) * 100)} className="h-2" />
            <ol className="space-y-2">
              {STEPS.map((s, i) => (
                <li
                  key={s}
                  className={cn("t-body flex items-center gap-2.5", i > stepIndex && "text-muted-foreground")}
                >
                  {i < stepIndex ? (
                    <Check className="text-success size-4" />
                  ) : i === stepIndex ? (
                    <Loader2 className="text-primary-text size-4 animate-spin" />
                  ) : (
                    <span className="bg-border ml-1.5 size-1.5 rounded-full" />
                  )}
                  {t(`newProject.steps.${s}`)}
                </li>
              ))}
            </ol>
          </div>
        ) : (
          <form
            className="space-y-5"
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim()) create.mutate();
            }}
          >
            <div className="space-y-2">
              <Label htmlFor={nameId}>{t("newProject.nameLabel")}</Label>
              <Input
                id={nameId}
                autoFocus
                maxLength={60}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("newProject.namePlaceholder")}
                className="h-10 rounded-lg"
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor={descId}>{t("newProject.descLabel")}</Label>
              <Input
                id={descId}
                maxLength={300}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t("newProject.descPlaceholder")}
                className="h-10 rounded-lg"
              />
            </div>
            <fieldset className="space-y-2">
              <legend className="t-small mb-2 font-medium">{t("newProject.templateLabel")}</legend>
              <div role="radiogroup" className="grid grid-cols-2 gap-2.5">
                {TEMPLATES.map(({ id, icon: Icon }) => {
                  const selected = template === id;
                  return (
                    <button
                      key={id}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      onClick={() => setTemplate(id)}
                      className={cn(
                        "flex items-start gap-3 rounded-xl border p-3 text-left transition-colors duration-[var(--dur)]",
                        selected ? "border-primary bg-accent/60 ring-primary/30 ring-2" : "hover:bg-muted/60",
                      )}
                    >
                      <span
                        className={cn(
                          "grid size-9 shrink-0 place-items-center rounded-lg",
                          selected ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground",
                        )}
                      >
                        <Icon className="size-4.5" />
                      </span>
                      <span className="min-w-0">
                        <span className="t-body block font-medium">{t(`newProject.templates.${id}.name`)}</span>
                        <span className="text-muted-foreground t-small block">
                          {t(`newProject.templates.${id}.desc`)}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </fieldset>

            {error && (
              <div role="alert" className="bg-destructive/8 border-destructive/30 rounded-xl border p-3.5">
                <p className="t-body text-destructive font-medium">{error.title}</p>
                <p className="t-small text-foreground/80 mt-0.5">{error.fix}</p>
                {error.action === "setup" && (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className="mt-2.5"
                    onClick={() => {
                      onOpenChange(false);
                      navigate("/setup");
                    }}
                  >
                    {t("common.openSetup")}
                  </Button>
                )}
              </div>
            )}

            <DialogFooter>
              <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
                {t("common.cancel")}
              </Button>
              <Button type="submit" disabled={!name.trim()}>
                {t("newProject.create")}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

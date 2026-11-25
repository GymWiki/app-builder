import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { AskButton, Field, inputCls } from "./shared";

export const PRESETS = ["#4F46E5", "#0D9488", "#2563EB", "#DB2777", "#EA580C", "#16A34A", "#7C3AED", "#475569"];
export const isHex = (s: string) => /^#[0-9a-fA-F]{6}$/.test(s);
const FONTS = ["Inter", "Poppins", "Lora", "Roboto"] as const;
const KITS = ["material", "cupertino", "rounded", "minimal"] as const;

/** Thema: kleur live in de preview; lettertype en stijl via Claude. */
export function ThemePanel({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const theme = useQuery({ queryKey: ["theme", projectId], queryFn: () => api.themeGet(projectId) });
  const [edit, setEdit] = useState<string | null>(null);
  const [note, setNote] = useState<{ wired: boolean } | null>(null);
  const current = theme.data?.seedColor ?? null;
  const hex = edit ?? current ?? "#4F46E5";
  const setHex = setEdit;

  const apply = async (value: string) => {
    if (!isHex(value)) return;
    try {
      const r = await api.themeSet(projectId, value);
      setNote({ wired: r.wired });
      await qc.invalidateQueries({ queryKey: ["theme", projectId] });
      setEdit(null);
      toast.success(t("sec.theme.applied"));
    } catch (e) {
      toastError(e, t);
    }
  };

  return (
    <div className="space-y-5">
      <section className="space-y-3">
        <h3 className="t-heading">{t("sec.theme.title")}</h3>
        <div role="radiogroup" aria-label={t("sec.theme.seed")} className="flex flex-wrap gap-2">
          {PRESETS.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={current === c}
              aria-label={c}
              title={c}
              onClick={() => void apply(c)}
              style={{ background: c }}
              className={cn(
                "size-8 rounded-full border-2 border-transparent transition-transform duration-[var(--dur)] hover:scale-110",
                current === c && "ring-ring ring-offset-background ring-2 ring-offset-2",
              )}
            />
          ))}
        </div>
        <Field label={t("sec.theme.hex")} htmlFor="theme-hex">
          <div className="flex gap-2">
            <input
              id="theme-hex"
              value={hex}
              onChange={(e) => setHex(e.target.value.trim())}
              aria-invalid={!isHex(hex)}
              className={cn(inputCls, "font-mono")}
            />
            <Button disabled={!isHex(hex) || hex.toUpperCase() === current} onClick={() => void apply(hex)}>
              {t("sec.theme.apply")}
            </Button>
          </div>
          {!isHex(hex) && (
            <p role="alert" className="text-destructive t-small">
              {t("sec.theme.hexInvalid")}
            </p>
          )}
        </Field>
        {(note?.wired === false || (theme.data && !theme.data.managed && !note)) && (
          <div className="border-warning/50 bg-warning/10 rounded-xl border p-3">
            <p className="t-small">{t("sec.theme.notWired")}</p>
            <div className="mt-2">
              <AskButton
                projectId={projectId}
                prompt={t("sec.theme.wirePrompt")}
                label={t("sec.theme.wire")}
                variant="outline"
              />
            </div>
          </div>
        )}
      </section>

      <section className="space-y-2">
        <h3 className="t-heading">{t("sec.theme.fontTitle")}</h3>
        <p className="t-small text-muted-foreground">{t("sec.theme.fontHelp")}</p>
        <div className="flex flex-wrap gap-2">
          {FONTS.map((f) => (
            <AskButton
              key={f}
              projectId={projectId}
              prompt={t("sec.theme.fontPrompt", { font: f })}
              label={t(`sec.theme.fonts.${f}`)}
              variant="outline"
            />
          ))}
        </div>
      </section>

      <section className="space-y-2">
        <h3 className="t-heading">{t("sec.theme.kitTitle")}</h3>
        <p className="t-small text-muted-foreground">{t("sec.theme.kitHelp")}</p>
        <div className="flex flex-wrap gap-2">
          {KITS.map((k) => (
            <AskButton
              key={k}
              projectId={projectId}
              prompt={t("sec.theme.kitPrompt", { kit: t(`sec.theme.kits.${k}`) })}
              label={t(`sec.theme.kits.${k}`)}
              variant="outline"
            />
          ))}
        </div>
      </section>
    </div>
  );
}

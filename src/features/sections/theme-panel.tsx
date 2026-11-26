import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";
import { AskButton, Field, inputCls } from "./shared";

export const PRESETS = ["#4F46E5", "#0D9488", "#2563EB", "#DB2777", "#EA580C", "#16A34A", "#7C3AED", "#475569"];
export const isHex = (s: string) => /^#[0-9a-fA-F]{6}$/.test(s);
export const FONTS = ["Inter", "Poppins", "Lora", "Roboto"] as const;
const KITS = ["material", "cupertino", "rounded", "minimal"] as const;

/** Thema: alles wat je kiest wordt meteen toegepast door Studio zelf (geen knop, geen chat); de preview verversen gebeurt vanzelf. */
export function ThemePanel({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const theme = useQuery({ queryKey: ["theme", projectId], queryFn: () => api.themeGet(projectId) });
  const [edit, setEdit] = useState<string | null>(null);
  const [note, setNote] = useState<{ wired: boolean; pubGetFailed: boolean } | null>(null);
  const current = theme.data?.seedColor ?? null;
  const hex = edit ?? current ?? "#4F46E5";
  const font = theme.data?.font ?? "";
  const kit = theme.data?.kit ?? "material";

  const apply = async (patch: { seedColor?: string; font?: string; kit?: string }, message: string) => {
    try {
      // Zonder gekozen kleur schrijft Studio meteen de standaardkleur mee, zodat lettertype en stijl ook werken.
      const full = { seedColor: current ?? "#4F46E5", ...patch };
      const r = await api.themeApply(projectId, full);
      setNote({ wired: r.wired, pubGetFailed: r.pubGetFailed });
      await qc.invalidateQueries({ queryKey: ["theme", projectId] });
      setEdit(null);
      toast.success(message);
    } catch (e) {
      toastError(e, t);
    }
  };

  // Een geldige code in het veld wordt vanzelf opgeslagen (even wachten tot je klaar bent met typen).
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (edit === null || !isHex(edit) || edit.toUpperCase() === current) return;
    timer.current = setTimeout(() => void apply({ seedColor: edit }, t("sec.theme.applied")), 600);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [edit]);

  const choice = (label: string, selected: boolean, onClick: () => void, key: string) => (
    <button
      key={key}
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onClick}
      className={cn(
        "t-small rounded-lg border px-3 py-1.5 transition-colors",
        selected ? "border-primary bg-primary/10 font-medium" : "hover:bg-accent",
      )}
    >
      {label}
    </button>
  );

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
              onClick={() => void apply({ seedColor: c }, t("sec.theme.applied"))}
              style={{ background: c }}
              className={cn(
                "size-8 rounded-full border-2 border-transparent transition-transform duration-[var(--dur)] hover:scale-110",
                current === c && "ring-ring ring-offset-background ring-2 ring-offset-2",
              )}
            />
          ))}
        </div>
        <Field label={t("sec.theme.hex")} htmlFor="theme-hex">
          <input
            id="theme-hex"
            value={hex}
            onChange={(e) => setEdit(e.target.value.trim())}
            aria-invalid={!isHex(hex)}
            className={cn(inputCls, "font-mono")}
          />
          {!isHex(hex) && (
            <p role="alert" className="text-destructive t-small">
              {t("sec.theme.hexInvalid")}
            </p>
          )}
        </Field>
        {theme.data?.deviation && !note && (
          <p role="status" className="t-small text-muted-foreground">
            {t("sec.theme.deviation")}
          </p>
        )}
        {note?.pubGetFailed && (
          <p role="alert" className="text-destructive t-small">
            {t("sec.theme.pubGetFailed")}
          </p>
        )}
        {note && !note.wired && (
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
        <div role="radiogroup" aria-label={t("sec.theme.fontTitle")} className="flex flex-wrap gap-2">
          {choice(t("sec.theme.defaultFont"), font === "", () => void apply({ font: "" }, t("sec.theme.saved")), "default")}
          {FONTS.map((f) =>
            choice(t(`sec.theme.fonts.${f}`), font === f, () => void apply({ font: f }, t("sec.theme.saved")), f),
          )}
        </div>
      </section>

      <section className="space-y-2">
        <h3 className="t-heading">{t("sec.theme.kitTitle")}</h3>
        <p className="t-small text-muted-foreground">{t("sec.theme.kitHelp")}</p>
        <div role="radiogroup" aria-label={t("sec.theme.kitTitle")} className="flex flex-wrap gap-2">
          {KITS.map((k) =>
            choice(t(`sec.theme.kits.${k}`), kit === k, () => void apply({ kit: k }, t("sec.theme.saved")), k),
          )}
        </div>
      </section>
    </div>
  );
}

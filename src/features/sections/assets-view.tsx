import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { ImagePlus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { api, inTauri } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { IconReport } from "@/lib/types";
import { cn } from "@/lib/utils";
import { PRESETS, isHex } from "./theme-panel";
import { AskButton, Card2, Check, Field, SectionPage, inputCls } from "./shared";

export function AssetsView({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [report, setReport] = useState<IconReport | null>(null);
  const [busy, setBusy] = useState(false);
  const icon = useQuery({
    queryKey: ["icon", projectId],
    queryFn: () => api.fileDataUrl(projectId, "store/icon-512.png").catch(() => null),
  });

  const pick = async () => {
    const path = inTauri()
      ? await open({
          multiple: false,
          title: t("sec.assets.pickTitle"),
          filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg"] }],
        })
      : "/demo/logo.png";
    if (typeof path !== "string") return;
    setBusy(true);
    try {
      setReport(await api.assetsGenerateIcon(projectId, path));
      await qc.invalidateQueries({ queryKey: ["icon", projectId] });
      await qc.invalidateQueries({ queryKey: ["checklist", projectId] });
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SectionPage title={t("sec.assets.title")} intro={t("sec.assets.intro")}>
      <Card2>
        <div className="flex flex-wrap items-center gap-5">
          <div className="bg-muted grid size-24 shrink-0 place-items-center overflow-hidden rounded-[22px] border">
            {icon.data ? (
              <img src={icon.data} alt={t("sec.assets.current")} className="size-full object-cover" />
            ) : (
              <ImagePlus aria-hidden className="text-muted-foreground size-7" />
            )}
          </div>
          <div className="space-y-2">
            <p className="t-body font-medium">{icon.data ? t("sec.assets.current") : t("sec.assets.none")}</p>
            <Button disabled={busy} onClick={() => void pick()}>
              <ImagePlus /> {busy ? t("sec.assets.generating") : t("sec.assets.pick")}
            </Button>
          </div>
        </div>
        {report && (
          <div role="status" className="space-y-1">
            <p className="text-success t-small font-medium">{t("sec.assets.done", { count: report.written.length })}</p>
            {report.warnings.map((w) => (
              <p key={w} className="text-warning t-small">
                {t(`sec.assets.warnings.${w}`)}
              </p>
            ))}
          </div>
        )}
      </Card2>

      <SplashCard projectId={projectId} hasIcon={!!icon.data} />

      <Card2 title={t("sec.assets.imagesTitle")}>
        <p className="t-small text-muted-foreground">{t("sec.assets.imagesHelp")}</p>
        <AskButton
          projectId={projectId}
          prompt={t("sec.assets.imagesPrompt")}
          label={t("sec.assets.images")}
          variant="outline"
        />
      </Card2>
    </SectionPage>
  );
}

/** Splash-scherm: kleur (standaard de themakleur) en optioneel het app-icoon; Studio schrijft de Android- en iOS-bestanden zelf, zonder chat. */
function SplashCard({ projectId, hasIcon }: { projectId: string; hasIcon: boolean }) {
  const { t } = useTranslation();
  const theme = useQuery({ queryKey: ["theme", projectId], queryFn: () => api.themeGet(projectId) });
  const [color, setColor] = useState<string | null>(null);
  const [logo, setLogo] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<{ written: string[]; warnings: string[] } | null>(null);
  const hex = color ?? theme.data?.seedColor ?? "#4F46E5";
  const useLogo = (logo ?? true) && hasIcon;
  const apply = async () => {
    setBusy(true);
    try {
      setReport(await api.assetsSplashApply(projectId, hex, useLogo));
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
    }
  };
  return (
    <Card2 title={t("sec.assets.splashTitle")}>
      <p className="t-small text-muted-foreground">{t("sec.assets.splashHelp")}</p>
      <div role="radiogroup" aria-label={t("sec.assets.splashColor")} className="flex flex-wrap gap-2">
        {PRESETS.map((c) => (
          <button
            key={c}
            type="button"
            role="radio"
            aria-checked={hex.toUpperCase() === c}
            aria-label={c}
            title={c}
            onClick={() => setColor(c)}
            style={{ background: c }}
            className={cn(
              "size-8 rounded-full border-2 border-transparent transition-transform duration-[var(--dur)] hover:scale-110",
              hex.toUpperCase() === c && "ring-ring ring-offset-background ring-2 ring-offset-2",
            )}
          />
        ))}
      </div>
      <Field label={t("sec.assets.splashColor")} htmlFor="splash-hex">
        <input
          id="splash-hex"
          value={hex}
          aria-invalid={!isHex(hex)}
          onChange={(e) => setColor(e.target.value.trim())}
          className={cn(inputCls, "font-mono")}
        />
      </Field>
      <Check
        id="splash-logo"
        checked={useLogo}
        disabled={!hasIcon}
        onChange={setLogo}
        label={t("sec.assets.splashLogo")}
        help={hasIcon ? undefined : t("sec.assets.splashNoIcon")}
      />
      <Button disabled={busy || !isHex(hex)} onClick={() => void apply()}>
        {t("sec.assets.splashApply")}
      </Button>
      {report && (
        <div role="status" className="space-y-1">
          <p className="text-success t-small font-medium">{t("sec.assets.done", { count: report.written.length })}</p>
          <p className="t-small text-muted-foreground">{t("sec.assets.splashAndroid12")}</p>
          {report.warnings.map((w) => (
            <p key={w} className="text-warning t-small">
              {t(`sec.assets.splashWarnings.${w}`, { defaultValue: w })}
            </p>
          ))}
        </div>
      )}
    </Card2>
  );
}

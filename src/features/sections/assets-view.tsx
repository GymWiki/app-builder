import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { ImagePlus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { api, inTauri } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { IconReport } from "@/lib/types";
import { AskButton, Card2, SectionPage } from "./shared";

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

      <Card2 title={t("sec.assets.splashTitle")}>
        <p className="t-small text-muted-foreground">{t("sec.assets.splashHelp")}</p>
        <AskButton
          projectId={projectId}
          prompt={t("sec.assets.splashPrompt")}
          label={t("sec.assets.splash")}
          variant="outline"
        />
      </Card2>

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

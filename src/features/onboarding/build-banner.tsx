import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useBuild } from "./build-store";

/** Rondkijken in al klare pagina's terwijl de rest nog wordt gebouwd: een smalle balk met de voortgang en een link naar het bouwscherm. */
export function BuildBanner({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const progress = useBuild((s) => s.progress[projectId]);
  const report = useBuild((s) => s.report[projectId]);
  if (!progress || report || progress.phase === "done") return null;
  const n = Math.min(progress.done + Math.max(progress.building.length, 1), progress.total);
  return (
    <div role="status" className="bg-accent/60 t-small flex shrink-0 items-center gap-2 border-b px-4 py-1.5">
      <Loader2 aria-hidden className="size-3.5 animate-spin" />
      <span className="min-w-0 flex-1">{t("build.banner", { n, total: progress.total })}</span>
      <Link to={`/build/${projectId}`} className="text-primary underline underline-offset-2">
        {t("build.bannerLink")}
      </Link>
    </div>
  );
}

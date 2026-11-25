import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Card } from "@/components/ds/card";
import { Button } from "@/components/ui/button";
import { qk } from "@/hooks";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Project } from "@/lib/types";

function Info({ project }: { project: Project }) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["profileInfo", project.id], queryFn: () => api.profileInfo(project.id) });
  const i = q.data;
  return (
    <li className="space-y-1 py-3">
      <p className="t-body font-medium">{project.name}</p>
      {i && (
        <ul className="t-small text-muted-foreground space-y-0.5 font-mono">
          <li>{t("profile.dev.version", { version: i.version, current: i.current })}</li>
          <li>{t("profile.dev.plugin", { state: t(i.plugin ? "profile.dev.yes" : "profile.dev.no") })}</li>
          <li>
            {t("profile.dev.dartMcp")}: {i.dart ? t("profile.dev.dart", { dart: i.dart }) : "–"} ·{" "}
            {t(i.dartMcpOk ? "profile.dev.okay" : "profile.dev.notOkay")}
          </li>
          <li>{t("profile.dev.structure", { structure: i.structure })}</li>
        </ul>
      )}
      <Button
        size="xs"
        variant="outline"
        onClick={() =>
          void api
            .profileApply(project.id)
            .then(() => q.refetch())
            .catch((e) => toastError(e, t))
        }
      >
        {t("profile.dev.reapply")}
      </Button>
    </li>
  );
}

/** Verborgen ontwikkelaarsmodus: technische toestand van het projectprofiel. */
export function DevPanel() {
  const { t } = useTranslation();
  const projects = useQuery({ queryKey: qk.projects(""), queryFn: () => api.projectsList("") });
  return (
    <Card className="space-y-2 p-6">
      <h2 className="t-heading">{t("profile.dev.title")}</h2>
      <p className="t-small text-muted-foreground">{t("profile.dev.on")}</p>
      <ul className="divide-y">
        {(projects.data ?? []).map((p) => (
          <Info key={p.id} project={p} />
        ))}
      </ul>
    </Card>
  );
}

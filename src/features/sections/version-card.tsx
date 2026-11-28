import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Tag } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Project } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Card2, Field, inputCls } from "./shared";

/** Versie en buildnummer: Studio houdt ze zelf juist (ook voor de volgende upload); hier kun je ze handmatig aanpassen. */
export function VersionCard({ project }: { project: Project }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const id = project.id;
  const q = useQuery({
    queryKey: ["version", id, project.version, project.buildNumber],
    queryFn: () => api.versionInfo(id),
  });
  const info = q.data;
  const [open, setOpen] = useState(false);
  const [version, setVersion] = useState("");
  const [build, setBuild] = useState("");
  const [busy, setBusy] = useState(false);

  const minBuild = (info?.lastPublished?.build ?? 0) + 1;
  const versionOk = /^\d+\.\d+\.\d+$/.test(version);
  const buildOk = build !== "" && Number(build) >= minBuild;

  const show = () => {
    setVersion(info?.version ?? project.version);
    setBuild(String(info?.build ?? project.buildNumber));
    setOpen(true);
  };
  const save = async () => {
    setBusy(true);
    try {
      await api.versionSet(id, version, Number(build));
      toast.success(t("sec.versions.version.saved"));
      setOpen(false);
      await Promise.all(["version", "project", "checklist"].map((k) => qc.invalidateQueries({ queryKey: [k, id] })));
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card2 title={t("sec.versions.version.title")}>
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
        <div>
          <p className="t-small text-muted-foreground">{t("sec.versions.version.current")}</p>
          <p className="t-heading font-mono" data-testid="version-current">
            {info?.version ?? project.version}{" "}
            <span className="text-muted-foreground">+{info?.build ?? project.buildNumber}</span>
          </p>
        </div>
        <div>
          <p className="t-small text-muted-foreground">{t("sec.versions.version.last")}</p>
          <p className="t-body font-mono">
            {info?.lastPublished
              ? `${info.lastPublished.version}${info.lastPublished.build ? ` +${info.lastPublished.build}` : ""}`
              : t("sec.versions.version.never")}
          </p>
        </div>
        <Button variant="outline" className="ml-auto" onClick={show} disabled={!info}>
          <Tag /> {t("sec.versions.version.change")}
        </Button>
      </div>
      {info && (
        <p className={cn("t-small mt-3", info.willChange ? "text-foreground" : "text-muted-foreground")} role="status">
          {info.willChange
            ? t("sec.versions.version.nextChanges", { version: info.nextVersion, build: info.nextBuild })
            : t("sec.versions.version.nextSame")}
        </p>
      )}
      <p className="t-small text-muted-foreground mt-1">{t("sec.versions.version.auto")}</p>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("sec.versions.version.dialogTitle")}</DialogTitle>
            <DialogDescription>{t("sec.versions.version.dialogBody")}</DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label={t("sec.settings.version")} htmlFor="v-version">
              <input
                id="v-version"
                value={version}
                onChange={(e) => setVersion(e.target.value.trim())}
                aria-invalid={!versionOk}
                className={cn(inputCls, "font-mono")}
              />
              {!versionOk && (
                <p role="alert" className="text-destructive t-small">
                  {t("sec.versions.version.versionInvalid")}
                </p>
              )}
            </Field>
            <Field label={t("sec.settings.build")} htmlFor="v-build">
              <input
                id="v-build"
                value={build}
                inputMode="numeric"
                onChange={(e) => setBuild(e.target.value.replace(/\D/g, ""))}
                aria-invalid={!buildOk}
                className={cn(inputCls, "font-mono")}
              />
              {!buildOk && (
                <p role="alert" className="text-destructive t-small">
                  {t("sec.versions.version.buildInvalid", { min: minBuild })}
                </p>
              )}
            </Field>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button disabled={!versionOk || !buildOk || busy} onClick={() => void save()}>
              {t("sec.common.save")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card2>
  );
}

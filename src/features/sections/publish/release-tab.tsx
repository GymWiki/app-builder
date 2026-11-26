import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, Loader2, Rocket } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { friendlyError } from "@/lib/errors";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Evaluation, Meta, Project } from "@/lib/types";
import { Card2, Check, Field, inputCls } from "../shared";
import { NO_LINES, NO_STATUS, useJobs } from "./jobs-store";

export function ReleaseTab({
  project,
  meta,
  evaluation,
  onMeta,
  onDone,
}: {
  project: Project;
  meta: Meta;
  evaluation: Evaluation;
  onMeta: (m: Meta) => Promise<void>;
  onDone: () => Promise<unknown>;
}) {
  const { t } = useTranslation();
  const id = project.id;
  const track = meta.publishing.track || "internal";
  const status = useJobs((s) => s.status[id] ?? NO_STATUS);
  const log = useJobs((s) => s.logs[id] ?? NO_LINES);
  const vq = useQuery({
    queryKey: ["version", id, project.version, project.buildNumber],
    queryFn: () => api.versionInfo(id),
  });
  const shown = { version: vq.data?.nextVersion ?? project.version, build: vq.data?.nextBuild ?? project.buildNumber };
  const [confirm, setConfirm] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const others = evaluation.items
    .filter((i) => i.blocking && i.key !== "final_confirmation")
    .every((i) => i.status === "done");
  const running = status.build === "running" || status.upload === "running";

  const publish = async () => {
    setConfirm(false);
    setDone(false);
    setError(null);
    useJobs.getState().clear(id);
    try {
      await api.publishAndroid(id, track, true);
      setDone(true);
      toast.success(t("sec.publish.release.done"));
    } catch (e) {
      const f = friendlyError(e, t);
      setError(`${f.title} ${f.fix}`);
      toastError(e, t);
    } finally {
      await onDone();
    }
  };

  return (
    <div className="space-y-5">
      <Card2 title={t("sec.publish.release.title")}>
        <p className="t-body text-muted-foreground">{t("sec.publish.release.intro")}</p>
        <p className="t-body font-medium">{t("sec.publish.release.version", shown)}</p>
        <Field label={t("sec.publish.release.track")} htmlFor="track">
          <select
            id="track"
            value={track}
            onChange={(e) => void onMeta({ ...meta, publishing: { ...meta.publishing, track: e.target.value } })}
            className={inputCls}
          >
            {["internal", "closed"].map((k) => (
              <option key={k} value={k}>
                {t(`sec.publish.release.tracks.${k}`)}
              </option>
            ))}
          </select>
        </Field>
        <Check
          id="final"
          checked={meta.publishing.finalConfirmed}
          disabled={!others}
          onChange={(v) => void onMeta({ ...meta, publishing: { ...meta.publishing, finalConfirmed: v } })}
          label={t("sec.publish.release.confirmBox")}
        />
        {!evaluation.canPublish && <p className="text-muted-foreground t-small">{t("sec.publish.release.blocked")}</p>}
        <Button disabled={!evaluation.canPublish || running} onClick={() => setConfirm(true)}>
          {running ? <Loader2 className="animate-spin" /> : <Rocket />}{" "}
          {running
            ? status.upload === "running"
              ? t("sec.publish.release.uploading")
              : t("sec.publish.release.building")
            : t("sec.publish.release.button")}
        </Button>
        {done && (
          <p role="status" className="text-success t-body flex items-center gap-2 font-medium">
            <CheckCircle2 className="size-4" /> {t("sec.publish.release.done")}
          </p>
        )}
        {error && (
          <p role="alert" className="text-destructive t-small">
            {t("sec.publish.release.failed")} {error}
          </p>
        )}
      </Card2>

      {log.length > 0 && (
        <section>
          <h3 className="t-heading mb-2">{t("sec.publish.release.log")}</h3>
          <pre
            tabIndex={0}
            aria-label={t("sec.publish.release.log")}
            className="bg-muted t-small max-h-72 overflow-auto rounded-lg p-3 font-mono whitespace-pre-wrap"
          >
            {log.join("\n")}
          </pre>
        </section>
      )}

      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent className="rounded-2xl">
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("sec.publish.release.confirmTitle", { version: `${shown.version} (${shown.build})` })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("sec.publish.release.confirmBody", { track: t(`sec.publish.release.tracks.${track}`) })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void publish()}>{t("sec.publish.release.confirmRun")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

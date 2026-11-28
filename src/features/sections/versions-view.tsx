import { useQuery, useQueryClient } from "@tanstack/react-query";
import { GitCommitHorizontal, History } from "lucide-react";
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
import { StatusBadge, type Tone } from "@/components/ds/status-badge";
import { timeAgo } from "@/lib/format";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";
import type { Project, Snapshot } from "@/lib/types";
import { VersionCard } from "./version-card";
import { AskButton, Card2, SectionPage, inputCls } from "./shared";

const TONE: Record<Snapshot["kind"], Tone> = { auto: "accent", manual: "neutral", publish: "success" };

export function VersionsView({ project }: { project: Project }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const id = project.id;
  const list = useQuery({ queryKey: ["snapshots", id], queryFn: () => api.snapshotsList(id) });
  const changes = useQuery({ queryKey: ["changelog", id], queryFn: () => api.changelogDraft(id) });
  const [label, setLabel] = useState("");
  const [target, setTarget] = useState<Snapshot | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: ["snapshots", id] }),
      qc.invalidateQueries({ queryKey: ["changelog", id] }),
      qc.invalidateQueries({ queryKey: ["project", id] }),
      qc.invalidateQueries({ queryKey: ["screens", id] }),
    ]);

  const create = async () => {
    setBusy(true);
    try {
      const r = await api.snapshotCreate(id, label.trim() || t("sec.versions.create"));
      toast(r.created ? t("sec.versions.created") : t("sec.versions.nothing"));
      setLabel("");
      await refresh();
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
    }
  };
  const restore = async () => {
    if (!target) return;
    setBusy(true);
    try {
      await api.snapshotRestore(id, target.commit);
      toast.success(t("sec.versions.restored", { label: target.label }));
      await refresh();
    } catch (e) {
      toastError(e, t);
    } finally {
      setBusy(false);
      setTarget(null);
    }
  };
  const snaps = list.data ?? [];
  const lines = changes.data ?? [];

  return (
    <SectionPage title={t("sec.versions.title")} intro={t("sec.versions.intro")}>
      <VersionCard project={project} />
      <Card2>
        <div className="flex gap-2">
          <input
            aria-label={t("sec.versions.create")}
            value={label}
            placeholder={t("sec.versions.createPlaceholder")}
            onChange={(e) => setLabel(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void create()}
            className={inputCls}
          />
          <Button disabled={busy} onClick={() => void create()}>
            {t("sec.versions.create")}
          </Button>
        </div>
      </Card2>

      {snaps.length === 0 ? (
        <div className="text-muted-foreground flex flex-col items-center gap-2 py-10 text-center">
          <History aria-hidden className="size-8" strokeWidth={1.5} />
          <p className="t-body">{t("sec.versions.empty")}</p>
        </div>
      ) : (
        <ol className="relative space-y-2 border-l pl-5" aria-label={t("sec.versions.title")}>
          {snaps.map((s, i) => (
            <li key={s.id} className="bg-card shadow-soft relative rounded-xl border p-3">
              <GitCommitHorizontal
                aria-hidden
                className="bg-background text-muted-foreground absolute top-4 -left-[30px] size-4 rounded-full"
              />
              <div className="flex flex-wrap items-center gap-2">
                <span className="t-body min-w-0 flex-1 truncate font-medium" title={s.label}>
                  {s.label}
                </span>
                <StatusBadge tone={TONE[s.kind]}>{t(`sec.versions.kind.${s.kind}`)}</StatusBadge>
              </div>
              <div className="mt-1 flex flex-wrap items-center justify-between gap-2">
                <span className="t-small text-muted-foreground">
                  v{s.version} · {timeAgo(s.createdAt, i18n.language)}
                </span>
                {i === 0 ? (
                  <span className="t-small text-primary-text font-medium">{t("sec.versions.current")}</span>
                ) : (
                  <Button size="xs" variant="outline" disabled={busy} onClick={() => setTarget(s)}>
                    {t("sec.versions.restore")}
                  </Button>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}

      <Card2 title={t("sec.versions.changelogTitle")}>
        <p className="t-small text-muted-foreground">{t("sec.versions.changelogHelp")}</p>
        {lines.length === 0 ? (
          <p className="t-body text-muted-foreground">{t("sec.versions.changelogEmpty")}</p>
        ) : (
          <ul className="t-body list-disc space-y-1 pl-5">
            {lines.slice(0, 20).map((l, i) => (
              <li key={i}>{l}</li>
            ))}
          </ul>
        )}
        <AskButton
          projectId={id}
          purpose="helper"
          prompt={t("sec.versions.changelogPrompt", {
            version: project.version,
            changes: lines.map((l) => `- ${l}`).join("\n") || "-",
          })}
          label={t("sec.versions.changelogAsk")}
          variant="outline"
        />
      </Card2>

      <AlertDialog open={!!target} onOpenChange={(o) => !o && setTarget(null)}>
        <AlertDialogContent className="rounded-2xl">
          <AlertDialogHeader>
            <AlertDialogTitle>{t("sec.versions.restoreTitle", { label: target?.label })}</AlertDialogTitle>
            <AlertDialogDescription>{t("sec.versions.restoreBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => void restore()}>{t("sec.versions.restoreConfirm")}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SectionPage>
  );
}

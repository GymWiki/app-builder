import { useMutation, useQueryClient } from "@tanstack/react-query";
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
import type { Project } from "@/lib/types";
import { toastError } from "@/lib/toast";

/** Twee bewuste keuzes; bestanden verwijderen vraagt een extra bevestiging (harde regel 5). */
export function RemoveProjectDialog({ project, onClose }: { project: Project | null; onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [confirming, setConfirming] = useState(false);

  const remove = useMutation({
    mutationFn: (deleteFiles: boolean) => api.projectDelete(project!.id, deleteFiles),
    onSuccess: async (_d, deleteFiles) => {
      await qc.invalidateQueries({ queryKey: ["projects"] });
      toast.success(t(deleteFiles ? "removeProject.deleted" : "removeProject.removed", { name: project!.name }));
      close();
    },
    onError: (e) => toastError(e, t),
  });

  const close = () => {
    setConfirming(false);
    onClose();
  };

  return (
    <Dialog open={!!project} onOpenChange={(o) => !o && !remove.isPending && close()}>
      <DialogContent className="rounded-2xl">
        {project && (
          <>
            <DialogHeader>
              <DialogTitle>{t("removeProject.title", { name: project.name })}</DialogTitle>
              <DialogDescription>{t("removeProject.body")}</DialogDescription>
            </DialogHeader>
            <div className="space-y-3">
              <div className="rounded-xl border p-4">
                <p className="t-body font-medium">{t("removeProject.keep")}</p>
                <p className="text-muted-foreground t-small mt-0.5">{t("removeProject.keepHint")}</p>
                <Button
                  variant="outline"
                  size="sm"
                  className="mt-3"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate(false)}
                >
                  {t("removeProject.keep")}
                </Button>
              </div>
              <div className="border-destructive/40 rounded-xl border p-4">
                <p className="t-body text-destructive font-medium">{t("removeProject.delete")}</p>
                <p className="text-muted-foreground t-small mt-0.5 break-all">
                  {t("removeProject.deleteHint", { path: project.path })}
                </p>
                {confirming ? (
                  <Button
                    variant="destructive"
                    size="sm"
                    className="mt-3"
                    disabled={remove.isPending}
                    onClick={() => remove.mutate(true)}
                  >
                    {t("removeProject.confirmDelete")}
                  </Button>
                ) : (
                  <Button
                    variant="outline"
                    size="sm"
                    className="text-destructive mt-3"
                    onClick={() => setConfirming(true)}
                  >
                    {t("removeProject.delete")}
                  </Button>
                )}
              </div>
            </div>
            <DialogFooter>
              <Button variant="ghost" onClick={close} disabled={remove.isPending}>
                {t("common.cancel")}
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}

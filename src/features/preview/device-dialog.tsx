import { useQuery } from "@tanstack/react-query";
import { RefreshCw, Smartphone } from "lucide-react";
import { useTranslation } from "react-i18next";
import { StatusBadge } from "@/components/ds/status-badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { api } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import type { Device } from "@/lib/types";

/** Echte telefoon of emulator kiezen, met stap-voor-stap uitleg als er niets gevonden wordt. */
export function DeviceDialog({
  open,
  onOpenChange,
  onRun,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  onRun: (d: Device) => void;
}) {
  const { t } = useTranslation();
  const q = useQuery({ queryKey: ["devices"], queryFn: api.previewDevices, enabled: open, staleTime: 0, gcTime: 0 });
  const devices = q.data ?? [];
  const steps = (k: string) => t(`preview.device.${k}`, { returnObjects: true }) as string[];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto rounded-2xl sm:max-w-xl">
        <DialogHeader>
          <DialogTitle className="t-title">{t("preview.device.title")}</DialogTitle>
          <DialogDescription>{t("preview.device.intro")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-2" aria-live="polite">
          {q.isFetching ? (
            <p className="text-muted-foreground t-body">{t("preview.device.scanning")}</p>
          ) : q.isError ? (
            <p role="alert" className="text-destructive t-body">
              {friendlyError(q.error, t).title} — {friendlyError(q.error, t).fix}
            </p>
          ) : devices.length === 0 ? (
            <div className="bg-muted/50 rounded-xl border border-dashed p-4">
              <p className="t-body font-medium">{t("preview.device.none")}</p>
              <p className="t-small text-muted-foreground mt-1">{t("preview.device.noneBody")}</p>
            </div>
          ) : (
            devices.map((d) => (
              <div key={d.id} className="flex items-center gap-3 rounded-xl border p-3">
                <Smartphone aria-hidden className="text-muted-foreground size-5 shrink-0" />
                <div className="min-w-0 flex-1">
                  <p className="t-body truncate font-medium">{d.name}</p>
                  <div className="mt-0.5 flex flex-wrap gap-1.5">
                    <StatusBadge tone="neutral">{d.platform.startsWith("ios") ? "iOS" : "Android"}</StatusBadge>
                    {d.emulator && <StatusBadge tone="info">{t("preview.device.emulator")}</StatusBadge>}
                    {d.wireless && <StatusBadge tone="accent">{t("preview.device.wireless")}</StatusBadge>}
                  </div>
                </div>
                <Button size="sm" onClick={() => onRun(d)}>
                  {t("preview.device.run")}
                </Button>
              </div>
            ))
          )}
          <Button variant="outline" size="sm" onClick={() => void q.refetch()} disabled={q.isFetching}>
            <RefreshCw className={q.isFetching ? "animate-spin" : ""} /> {t("preview.device.refresh")}
          </Button>
        </div>

        <div className="space-y-4 border-t pt-4">
          <section>
            <h3 className="t-heading mb-2">{t("preview.device.stepsTitle")}</h3>
            <ol className="t-body text-foreground/90 list-decimal space-y-1.5 pl-5">
              {steps("usb").map((s, i) => (
                <li key={i}>{s}</li>
              ))}
            </ol>
          </section>
          <section>
            <h3 className="t-heading mb-2">{t("preview.device.wirelessTitle")}</h3>
            <ol className="t-body text-foreground/90 list-decimal space-y-1.5 pl-5">
              {steps("wireless_steps").map((s, i) => (
                <li key={i}>{s}</li>
              ))}
            </ol>
          </section>
          <p className="t-small text-muted-foreground">{t("preview.device.ios")}</p>
        </div>
      </DialogContent>
    </Dialog>
  );
}

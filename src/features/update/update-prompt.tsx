import { Download, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { log } from "@/lib/log";
import { checkForUpdate, type UpdateInfo } from "@/lib/updater";

/** Bij elke start van Studio op de computer: is er een nieuwere versie? Zo ja, dan vraagt Studio om te updaten (Later = tot de volgende start). */
export function UpdatePrompt() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [pct, setPct] = useState<number | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    checkForUpdate()
      .then((u) => {
        if (!cancelled && u) {
          setInfo(u);
          setOpen(true);
        }
      })
      // Geen internet of een onbereikbare server is geen reden voor een melding; Studio werkt gewoon door.
      .catch((e) => log("warn", `update check failed: ${String(e)}`));
    return () => {
      cancelled = true;
    };
  }, []);

  if (!info) return null;
  const install = async () => {
    setBusy(true);
    setFailed(false);
    setPct(null);
    try {
      await info.install(setPct);
    } catch (e) {
      log("warn", `update failed: ${String(e)}`);
      setFailed(true);
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !busy && setOpen(o)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("update.title", { version: info.version })}</DialogTitle>
          <DialogDescription>{t("update.body", { current: info.current, version: info.version })}</DialogDescription>
        </DialogHeader>
        {info.notes && (
          <p className="bg-muted t-small max-h-40 overflow-auto rounded-lg p-3 whitespace-pre-wrap">{info.notes}</p>
        )}
        {busy && (
          <div role="status" className="space-y-2">
            {pct === null ? <Loader2 aria-hidden className="size-4 animate-spin" /> : <Progress value={pct} className="h-2" />}
            <p className="t-small text-muted-foreground">{t("update.installing")}</p>
          </div>
        )}
        {failed && (
          <p role="alert" className="text-destructive t-small">
            {t("update.failed")}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={() => setOpen(false)}>
            {t("update.later")}
          </Button>
          <Button disabled={busy} onClick={() => void install()}>
            <Download /> {t(failed ? "update.retry" : "update.now")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

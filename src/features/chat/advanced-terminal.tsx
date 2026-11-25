import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/ipc";
import { toastError } from "@/lib/toast";

const PtyTerminal = lazy(() => import("@/components/pty-terminal"));

/** Ruwe `claude` in de projectmap, voor wie een terminal prettig vindt. */
export function AdvancedTerminal({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const [ptyId, setPtyId] = useState<string | null>(null);
  const current = useRef<string | null>(null);

  const stop = async () => {
    if (current.current) await api.ptyKill(current.current).catch(() => undefined);
    current.current = null;
    setPtyId(null);
  };
  // Terminal netjes sluiten bij het verlaten van de tab of het project.
  useEffect(
    () => () => {
      if (current.current) void api.ptyKill(current.current).catch(() => undefined);
    },
    [],
  );

  const start = async () => {
    try {
      const id = await api.ptyOpenClaude(projectId);
      current.current = id;
      setPtyId(id);
    } catch (e) {
      toastError(e, t);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-3">
      <p className="text-muted-foreground t-small">{t("chat.advanced.intro")}</p>
      {ptyId ? (
        <>
          <Suspense fallback={<div className="h-56 animate-pulse rounded-xl bg-[#0f1117]" />}>
            <div className="min-h-0 flex-1 [&>div]:h-full">
              <PtyTerminal ptyId={ptyId} label={t("chat.advanced.running")} />
            </div>
          </Suspense>
          <Button variant="outline" size="sm" className="self-start" onClick={() => void stop()}>
            {t("chat.advanced.stop")}
          </Button>
        </>
      ) : (
        <Button size="sm" className="self-start" onClick={() => void start()}>
          {t("chat.advanced.start")}
        </Button>
      )}
    </div>
  );
}

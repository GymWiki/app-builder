import { Check, Loader2, Info } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks";
import { useProfile } from "./profile-store";

/** Eén rustige regel in gewone taal; technische details alleen in de ontwikkelaarsmodus. */
export function ProfileStatusLine({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const status = useProfile((s) => s.byProject[projectId]);
  const dev = useSettings().data?.dev_mode === "true";
  const [hideReady, setHideReady] = useState(false);
  const state = status?.state;

  useEffect(() => {
    if (state !== "ready") return;
    const id = setTimeout(() => setHideReady(true), 4000);
    return () => clearTimeout(id);
  }, [state]);
  // Een nieuwe statusronde toont de regel weer (reset tijdens het renderen, geen effect nodig).
  const [lastState, setLastState] = useState(state);
  if (lastState !== state) {
    setLastState(state);
    if (state !== "ready") setHideReady(false);
  }

  if (!status || (status.state === "ready" && hideReady)) return null;
  const Icon = status.state === "working" ? Loader2 : status.state === "ready" ? Check : Info;
  return (
    <p role="status" className="text-muted-foreground t-small flex items-start gap-2 border-b px-3 py-1.5">
      <Icon
        aria-hidden
        className={
          status.state === "working"
            ? "mt-0.5 size-3.5 shrink-0 animate-spin"
            : status.state === "ready"
              ? "text-success mt-0.5 size-3.5 shrink-0"
              : "mt-0.5 size-3.5 shrink-0"
        }
      />
      <span>
        {t(`profile.${status.state}`)}
        {dev && status.reason && <span className="block font-mono opacity-70">{status.reason}</span>}
      </span>
    </p>
  );
}

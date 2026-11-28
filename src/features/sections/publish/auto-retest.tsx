import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useChat } from "@/features/chat/chat-store";
import { runTest, useJobs } from "./jobs-store";

/** Na het oplossen door Claude (en alleen als de gebruiker dat wilde) automatisch opnieuw testen. */
export function AutoRetest({ projectId }: { projectId: string }) {
  const qc = useQueryClient();
  const running = useChat((s) => s.byProject[projectId]?.running ?? false);
  const was = useRef(false);
  useEffect(() => {
    // Claude kan bestanden, vermelding, thema en instellingen hebben aangepast: alles wat daarvan afhangt opnieuw ophalen.
    if (was.current && !running) {
      for (const key of [
        "meta",
        "checklist",
        "platform",
        "theme",
        "snapshots",
        "changelog",
        "screens",
        "signing",
        "icon",
        "project",
        "testReport",
      ]) {
        void qc.invalidateQueries({ queryKey: [key, projectId] });
      }
    }
    if (was.current && !running && useJobs.getState().retestPending[projectId]) {
      useJobs.getState().setRetest(projectId, false);
      void runTest(projectId, async () => {
        await Promise.all([
          qc.invalidateQueries({ queryKey: ["testReport", projectId] }),
          qc.invalidateQueries({ queryKey: ["checklist", projectId] }),
        ]);
      }).catch(() => undefined);
    }
    was.current = running;
  }, [running, projectId, qc]);
  return null;
}

import { create } from "zustand";
import { api, onEvent } from "@/lib/ipc";
import type { BuildProgress, BuildReport } from "@/lib/types";

export const useBuild = create<{
  progress: Record<string, BuildProgress>;
  report: Record<string, BuildReport>;
}>(() => ({ progress: {}, report: {} }));

/** Houdt de voortgang van het bouwen van nieuwe apps bij (ook als het bouwscherm er niet is). Eén keer aanroepen bij het starten. */
export function initBuildListeners(): () => void {
  const a = onEvent<{ projectId: string; progress: BuildProgress }>("build-progress", (e) =>
    useBuild.setState((s) => ({ progress: { ...s.progress, [e.projectId]: e.progress } })),
  );
  const b = onEvent<{ projectId: string; report: BuildReport }>("build-done", (e) =>
    useBuild.setState((s) => ({ report: { ...s.report, [e.projectId]: e.report } })),
  );
  return () => {
    void a.then((f) => f());
    void b.then((f) => f());
  };
}

/** Een bouw die al loopt (bijvoorbeeld na het openen van het scherm): de laatste voortgang ophalen. */
export async function restoreBuild(projectId: string) {
  const p = await api.onboardingStatus(projectId).catch(() => null);
  if (p) useBuild.setState((s) => (s.progress[projectId] ? s : { progress: { ...s.progress, [projectId]: p } }));
}

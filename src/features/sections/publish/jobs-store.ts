import { create } from "zustand";
import { api, onEvent } from "@/lib/ipc";

export type JobStatus = "running" | "done" | "failed";

/** Stabiele lege waarden: een selector mag niet bij elke aanroep een nieuw object teruggeven. */
export const NO_LINES: string[] = [];
export const NO_STATUS: Partial<Record<string, JobStatus>> = {};

interface JobsState {
  logs: Record<string, string[]>;
  status: Record<string, Partial<Record<string, JobStatus>>>;
  /** Projecten waarvoor na het oplossen door Claude automatisch opnieuw getest moet worden. */
  retestPending: Record<string, boolean>;
  setRetest: (projectId: string, on: boolean) => void;
  clear: (projectId: string) => void;
}

export const useJobs = create<JobsState>((set) => ({
  logs: {},
  status: {},
  retestPending: {},
  setRetest: (projectId, on) => set((s) => ({ retestPending: { ...s.retestPending, [projectId]: on } })),
  clear: (projectId) => set((s) => ({ logs: { ...s.logs, [projectId]: [] } })),
}));

/** Houdt logregels en taakstatus (test, build, upload) bij vanuit de events van Rust. */
export function initJobListeners(): () => void {
  const un = [
    onEvent<{ projectId: string; job: string; line: string }>("job-log", ({ projectId, line }) =>
      useJobs.setState((s) => ({ logs: { ...s.logs, [projectId]: [...(s.logs[projectId] ?? []), line].slice(-300) } })),
    ),
    onEvent<{ projectId: string; job: string; status: JobStatus }>("job-state", ({ projectId, job, status }) =>
      useJobs.setState((s) => ({ status: { ...s.status, [projectId]: { ...s.status[projectId], [job]: status } } })),
    ),
  ];
  return () => un.forEach((u) => void u.then((f) => f()));
}

/** Test starten en daarna de checklist verversen. De Rust-kant laat maar één taak per project tegelijk toe. */
export async function runTest(projectId: string, refresh: () => Promise<unknown>) {
  useJobs.getState().clear(projectId);
  try {
    return await api.testrunStart(projectId);
  } finally {
    await refresh();
  }
}

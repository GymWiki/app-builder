import { create } from "zustand";
import { useChat } from "@/features/chat/chat-store";
import { api } from "@/lib/ipc";
import type { Purpose } from "@/lib/types";

export type TaskStatus = "running" | "done" | "failed" | "cancelled";

export interface Task {
  id: string;
  projectId: string;
  title: string;
  prompt: string;
  language: string;
  purpose?: Purpose;
  status: TaskStatus;
  startedAt: number;
  /** Hoeveel chat-items er waren toen de taak begon (om alleen onze eigen berichten te lezen). */
  from: number;
  /** Foutcode van de chat (`chat.errors.<code>`), als de taak mislukte. */
  errorCode: string | null;
  cancelled: boolean;
}

/** Een taak werkt zelfstandig: geen open vragen in de chat. Ontbreekt er iets, dan kiest de AI een redelijke standaard en meldt dat. */
export const AUTONOMOUS =
  "Work on this by yourself and do not ask me questions in the chat. If something is unclear or missing, choose a sensible default and say in your final summary what you assumed. Task: ";

export const useTasks = create<{
  tasks: Task[];
  start: (projectId: string, title: string, prompt: string, language: string, purpose?: Purpose) => Promise<void>;
  cancel: (id: string) => Promise<void>;
  retry: (id: string) => Promise<void>;
  dismiss: (id: string) => void;
}>((set, get) => ({
  tasks: [],

  async start(projectId, title, prompt, language, purpose) {
    const id = `t${Date.now()}${Math.floor(Math.random() * 1000)}`;
    const from = useChat.getState().byProject[projectId]?.items.length ?? 0;
    const task: Task = {
      id,
      projectId,
      title,
      prompt,
      language,
      purpose,
      status: "running",
      startedAt: Date.now(),
      from,
      errorCode: null,
      cancelled: false,
    };
    set((s) => ({ tasks: [task, ...s.tasks.filter((x) => x.projectId !== projectId || x.status === "running")].slice(0, 6) }));
    // Meting: dit is een AI-aanroep (geen chatbeurt van de gebruiker; de taak loopt op de achtergrond).
    void api.uxEvent(projectId, "assist", "ai").catch(() => undefined);
    try {
      await useChat.getState().send(projectId, AUTONOMOUS + prompt, null, language, purpose);
    } catch {
      set((s) => ({ tasks: s.tasks.map((x) => (x.id === id ? { ...x, status: "failed", errorCode: "unknown" } : x)) }));
    }
  },

  async cancel(id) {
    const t = get().tasks.find((x) => x.id === id);
    if (!t || t.status !== "running") return;
    set((s) => ({ tasks: s.tasks.map((x) => (x.id === id ? { ...x, cancelled: true } : x)) }));
    await useChat.getState().cancel(t.projectId);
  },

  async retry(id) {
    const t = get().tasks.find((x) => x.id === id);
    if (!t) return;
    set((s) => ({ tasks: s.tasks.filter((x) => x.id !== id) }));
    await get().start(t.projectId, t.title, t.prompt, t.language, t.purpose);
  },

  dismiss(id) {
    set((s) => ({ tasks: s.tasks.filter((x) => x.id !== id) }));
  },
}));

/** Zodra de chat van een project klaar is, krijgt de lopende taak een uitkomst: klaar, mislukt (met een uitleg) of geannuleerd. */
export function initTaskListeners(): () => void {
  let prev: Record<string, boolean> = {};
  return useChat.subscribe((s) => {
    for (const [projectId, chat] of Object.entries(s.byProject)) {
      const was = prev[projectId] ?? false;
      if (was && !chat.running) {
        const t = useTasks.getState().tasks.find((x) => x.projectId === projectId && x.status === "running");
        if (t) {
          const err = chat.items.slice(t.from).find((i) => i.kind === "error");
          const status: TaskStatus = t.cancelled ? "cancelled" : err ? "failed" : "done";
          useTasks.setState((st) => ({
            tasks: st.tasks.map((x) =>
              x.id === t.id ? { ...x, status, errorCode: err && err.kind === "error" ? err.code : null } : x,
            ),
          }));
        }
      }
      prev[projectId] = chat.running;
    }
    prev = { ...prev };
  });
}

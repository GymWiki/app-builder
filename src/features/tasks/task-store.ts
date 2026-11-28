import { create } from "zustand";
import { useChat } from "@/features/chat/chat-store";
import { api } from "@/lib/ipc";
import type { Purpose } from "@/lib/types";
import { classify, fanoutPlan, type Strategy } from "@/features/project/scope";

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
  /** Alleen bij een opdracht over meerdere pagina's: voortgang, gekozen strategie en wat er is gebeurd. */
  scope?: ScopeRun;
}

export interface ScopeRun {
  strategy: Strategy;
  total: number;
  done: number;
  current: string | null;
  /** Pagina's (klassenamen) die klaar zijn, zodat het raster ze kan markeren. */
  changed: string[];
  failed: string[];
  sharedDone: boolean;
  /** Commit van de momentopname vóór de opdracht (voor "Terugdraaien"). */
  snapshot: string | null;
  finished: boolean;
  undone: boolean;
  startedAt: number;
  /** Per run: wat er gebeurde (voor de ontwikkelaarsmodus). */
  log: { step: string; ok: boolean; seconds: number }[];
  refs: PageRef[];
}

export interface PageRef {
  class: string;
  file: string;
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
  /** Een opdracht over meerdere pagina's (Hele app, map, selectie): Studio kiest zelf de efficiëntste uitvoering. */
  startScope: (a: { projectId: string; title: string; text: string; language: string; pages: PageRef[]; scopeLabel: string }) => Promise<void>;
  retryFailed: (id: string, language: string) => Promise<void>;
  undo: (id: string) => Promise<void>;
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

  async startScope({ projectId, title, text, language, pages, scopeLabel }) {
    const { strategy } = classify(text, pages.length);
    const id = `t${Date.now()}${Math.floor(Math.random() * 1000)}`;
    const steps = strategy === "fanout" ? fanoutPlan(pages.map((p) => p.class)) : [{ kind: "shared" as const }];
    const run: ScopeRun = {
      strategy,
      total: strategy === "fanout" ? pages.length : 1,
      done: 0,
      current: null,
      changed: [],
      failed: [],
      sharedDone: false,
      snapshot: null,
      finished: false,
      undone: false,
      startedAt: Date.now(),
      log: [],
      refs: pages,
    };
    const from = useChat.getState().byProject[projectId]?.items.length ?? 0;
    const task: Task = { id, projectId, title, prompt: text, language, status: "running", startedAt: Date.now(), from, errorCode: null, cancelled: false, scope: run };
    set((s) => ({ tasks: [task, ...s.tasks.filter((x) => x.projectId !== projectId || x.status === "running")].slice(0, 6) }));
    const patch = (f: (r: ScopeRun) => Partial<ScopeRun>, extra?: Partial<Task>) =>
      set((s) => ({ tasks: s.tasks.map((x) => (x.id === id && x.scope ? { ...x, ...extra, scope: { ...x.scope, ...f(x.scope) } } : x)) }));
    const cancelled = () => get().tasks.find((x) => x.id === id)?.cancelled === true;
    try {
      // Vooraf een momentopname, zodat het hele resultaat met één klik terug kan.
      const snap = await api.snapshotCreate(projectId, `Before: ${text.slice(0, 60)}`).catch(() => null);
      patch(() => ({ snapshot: snap?.commit ?? null }));
      const chat = useChat.getState();
      const runStep = async (label: string, prompt: string): Promise<boolean> => {
        if (cancelled()) return false;
        const t0 = Date.now();
        await chat.newSession(projectId).catch(() => undefined);
        const start = useChat.getState().byProject[projectId]?.items.length ?? 0;
        void api.uxEvent(projectId, `scope_${strategy}`, "ai").catch(() => undefined);
        try {
          await useChat.getState().send(projectId, AUTONOMOUS + prompt, null, language);
          await waitIdle(projectId);
        } catch {
          patch((r) => ({ log: [...r.log, { step: label, ok: false, seconds: Math.round((Date.now() - t0) / 1000) }] }));
          return false;
        }
        const items = useChat.getState().byProject[projectId]?.items.slice(start) ?? [];
        const ok = !items.some((i) => i.kind === "error") && !cancelled();
        patch((r) => ({ log: [...r.log, { step: label, ok, seconds: Math.round((Date.now() - t0) / 1000) }] }));
        return ok;
      };
      if (strategy === "single") {
        await runStep("run", `${text}\n\nScope: ${scopeLabel}.`);
        patch(() => ({ done: 1 }));
      } else if (strategy === "plan") {
        patch(() => ({ current: "plan" }));
        await runStep(
          "plan+run",
          `This change is wide and complicated. First write a short plan (a few lines, no questions to me), then carry it out in this one run, using the screen index and the shared layer (lib/core/) as your map and opening only the files you need. Scope: ${scopeLabel}.\n\nRequest: ${text}`,
        );
        patch(() => ({ done: 1, changed: pages.map((p) => p.class) }));
      } else {
        for (const step of steps) {
          if (cancelled()) break;
          if (step.kind === "shared") {
            patch(() => ({ current: "shared" }));
            const ok = await runStep(
              "shared",
              `Wide but simple change across ${pages.length} screens. Do the shared part first: change ONLY shared code (lib/core/, theme tokens, shared widgets) so the screens need as little change as possible. Do not edit screen files unless unavoidable.\n\nRequest: ${text}`,
            );
            patch(() => ({ sharedDone: ok }));
          } else {
            const page = pages.find((p) => p.class === step.page)!;
            patch(() => ({ current: page.class }));
            const prompt = `Apply this request to the screen "${page.class}" (${page.file}) only: ${text}\nA shared layer was already updated; read lib/core/ only as far as you need. Keep the change to this screen minimal and do not touch other screens.`;
            let ok = await runStep(page.class, prompt);
            if (!ok && !cancelled()) ok = await runStep(`${page.class} (retry)`, prompt); // één gerichte herpoging
            patch((r) => ({ done: r.done + 1, changed: ok ? [...r.changed, page.class] : r.changed, failed: ok ? r.failed : [...r.failed, page.class] }));
          }
        }
      }
    } catch {
      /* de uitkomst staat hieronder in de taak */
    }
    const wasCancelled = cancelled();
    patch(() => ({ current: null, finished: true }), { status: wasCancelled ? "cancelled" : "done" });
    // Na een brede wijziging: automatisch een snelle rooktest (de AI-testrun); alleen problemen worden gemeld in de checklist.
    if (!wasCancelled) void import("@/features/sections/publish/jobs-store").then((m) => m.runTest(projectId, async () => undefined)).catch(() => undefined);
  },

  async retryFailed(id, language) {
    const t = get().tasks.find((x) => x.id === id);
    if (!t?.scope || t.scope.failed.length === 0) return;
    const pages = t.scope.refs.filter((r) => t.scope!.failed.includes(r.class));
    set((s) => ({ tasks: s.tasks.filter((x) => x.id !== id) }));
    await get().startScope({ projectId: t.projectId, title: t.title, text: t.prompt, language, pages, scopeLabel: pages.map((p) => p.class).join(", ") });
  },

  async undo(id) {
    const t = get().tasks.find((x) => x.id === id);
    if (!t?.scope?.snapshot) return;
    await api.snapshotRestore(t.projectId, t.scope.snapshot);
    set((s) => ({ tasks: s.tasks.map((x) => (x.id === id && x.scope ? { ...x, scope: { ...x.scope, undone: true } } : x)) }));
  },
}));

/** Wacht tot de chat van dit project klaar is met de lopende run. */
function waitIdle(projectId: string): Promise<void> {
  return new Promise((resolve) => {
    const check = () => !useChat.getState().byProject[projectId]?.running;
    // de run kan een tel nodig hebben om als lopend te verschijnen
    let seenRunning = useChat.getState().byProject[projectId]?.running === true;
    const un = useChat.subscribe((s) => {
      const running = s.byProject[projectId]?.running === true;
      if (running) seenRunning = true;
      if (seenRunning && !running) {
        un();
        resolve();
      }
    });
    if (check() && seenRunning) {
      un();
      resolve();
    }
    setTimeout(() => {
      if (!seenRunning && check()) {
        un();
        resolve();
      }
    }, 3000);
  });
}

/** Zodra de chat van een project klaar is, krijgt de lopende taak een uitkomst: klaar, mislukt (met een uitleg) of geannuleerd. */
export function initTaskListeners(): () => void {
  let prev: Record<string, boolean> = {};
  return useChat.subscribe((s) => {
    for (const [projectId, chat] of Object.entries(s.byProject)) {
      const was = prev[projectId] ?? false;
      if (was && !chat.running) {
        // Taken met een bereik (meerdere pagina's) bepalen zelf wanneer ze klaar zijn: er lopen meerdere runs achter elkaar.
        const t = useTasks.getState().tasks.find((x) => x.projectId === projectId && x.status === "running" && !x.scope);
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

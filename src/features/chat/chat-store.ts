import { create } from "zustand";
import { api, onEvent } from "@/lib/ipc";
import type { ChatEvent, PermissionDecision, PermissionRequest } from "@/lib/types";
import { applyEvent, finishRunning, replay, type Item } from "./chat-model";

interface ProjectChat {
  items: Item[];
  running: boolean;
  startedAt: number | null;
  loaded: boolean;
}

interface ChatState {
  byProject: Record<string, ProjectChat>;
  permissions: PermissionRequest[];
  load: (projectId: string) => Promise<void>;
  send: (projectId: string, text: string, context: string | null, language: string) => Promise<void>;
  cancel: (projectId: string) => Promise<void>;
  newSession: (projectId: string) => Promise<void>;
  respond: (id: string, decision: PermissionDecision) => Promise<void>;
}

const empty: ProjectChat = { items: [], running: false, startedAt: null, loaded: false };

export const useChat = create<ChatState>((set) => {
  const patch = (id: string, f: (c: ProjectChat) => Partial<ProjectChat>) =>
    set((s) => {
      const cur = s.byProject[id] ?? empty;
      return { byProject: { ...s.byProject, [id]: { ...cur, ...f(cur) } } };
    });

  return {
    byProject: {},
    permissions: [],

    async load(projectId) {
      const [history, running] = await Promise.all([api.chatHistory(projectId), api.chatRunning(projectId)]);
      patch(projectId, (c) => ({
        items: c.running && c.items.length > 0 ? c.items : replay(history),
        running,
        loaded: true,
        startedAt: running ? (c.startedAt ?? Date.now()) : null,
      }));
    },

    async send(projectId, text, context, language) {
      patch(projectId, () => ({ running: true, startedAt: Date.now() }));
      try {
        await api.chatSend(projectId, text, context, language);
      } catch (e) {
        patch(projectId, () => ({ running: false, startedAt: null }));
        throw e;
      }
    },

    async cancel(projectId) {
      await api.chatCancel(projectId);
    },

    async newSession(projectId) {
      await api.chatNewSession(projectId);
      patch(projectId, () => ({ items: [], running: false, startedAt: null }));
    },

    async respond(id, decision) {
      set((s) => ({ permissions: s.permissions.filter((p) => p.id !== id) }));
      await api.permissionRespond(id, decision);
    },
  };
});

/** Koppelt de events uit Rust aan de store. Eén keer aanroepen bij het starten van de app. */
export function initChatListeners(): () => void {
  const unsub: Promise<() => void>[] = [
    onEvent<{ projectId: string; event: ChatEvent }>("chat-event", ({ projectId, event }) => {
      useChat.setState((s) => {
        const cur = s.byProject[projectId] ?? empty;
        return {
          byProject: {
            ...s.byProject,
            [projectId]: {
              ...cur,
              items: applyEvent(cur.items, event),
              running: true,
              startedAt: cur.startedAt ?? Date.now(),
            },
          },
        };
      });
    }),
    onEvent<{ projectId: string }>("chat-finished", ({ projectId }) => {
      useChat.setState((s) => {
        const cur = s.byProject[projectId] ?? empty;
        return {
          byProject: {
            ...s.byProject,
            [projectId]: { ...cur, items: finishRunning(cur.items), running: false, startedAt: null },
          },
          permissions: s.permissions.filter((p) => p.projectId !== projectId),
        };
      });
    }),
    onEvent<PermissionRequest>("permission-request", (req) =>
      useChat.setState((s) => ({ permissions: [...s.permissions, req] })),
    ),
  ];
  return () => unsub.forEach((u) => void u.then((f) => f()));
}

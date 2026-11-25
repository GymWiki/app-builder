import { create } from "zustand";
import { api, onEvent } from "@/lib/ipc";
import type { PreviewState } from "@/lib/types";

const initial: PreviewState = {
  projectId: null,
  status: "stopped",
  target: "web",
  url: null,
  problems: [],
  reloads: 0,
};

export const usePreview = create<{ state: PreviewState; busy: boolean; refresh: () => Promise<void> }>((set) => ({
  state: initial,
  busy: false,
  refresh: async () => set({ state: await api.previewState() }),
}));

/** Houdt de store gelijk aan de preview in Rust. Eén keer aanroepen bij het starten van de app. */
export function initPreviewListener(): () => void {
  void usePreview
    .getState()
    .refresh()
    .catch(() => undefined);
  const un = onEvent<PreviewState>("preview-state", (state) => usePreview.setState({ state }));
  return () => void un.then((f) => f());
}

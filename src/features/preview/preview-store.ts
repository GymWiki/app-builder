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
  mode: "iframe",
};

export const usePreview = create<{
  state: PreviewState;
  busy: boolean;
  /** Laatste beeld uit de beeldstroom (inspect-modus), als data-URL. */
  frame: string | null;
  refresh: () => Promise<void>;
}>((set) => ({
  state: initial,
  busy: false,
  frame: null,
  refresh: async () => set({ state: await api.previewState() }),
}));

/** Houdt de store gelijk aan de preview in Rust. Eén keer aanroepen bij het starten van de app. */
export function initPreviewListener(): () => void {
  void usePreview
    .getState()
    .refresh()
    .catch(() => undefined);
  const un = onEvent<PreviewState>("preview-state", (state) =>
    usePreview.setState((s) => ({ state, frame: state.status === "stopped" ? null : s.frame })),
  );
  const frames = onEvent<{ data: string; mime?: string }>("preview-frame", (f) =>
    usePreview.setState({ frame: `data:${f.mime ?? "image/jpeg"};base64,${f.data}` }),
  );
  return () => {
    void un.then((f) => f());
    void frames.then((f) => f());
  };
}

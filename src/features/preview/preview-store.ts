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
  /** De route waar de preview nu is (door de gebruiker of door Studio gekozen). */
  route: string | null;
  /** Telt mee elke keer dat de preview zelf een route meldt (ook dezelfde), zodat de pagina-lijst kan volgen. */
  routeSeq: number;
  /** Tijdens het op de achtergrond verzamelen van schermafbeeldingen negeren we routes (de gebruiker zit er niet aan). */
  crawling: boolean;
  /** Wanneer Studio zelf voor het laatst naar een route navigeerde (ms); routes kort daarna zijn geen keuze van de gebruiker. */
  navAt: number;
  refresh: () => Promise<void>;
}>((set) => ({
  state: initial,
  busy: false,
  frame: null,
  route: null,
  routeSeq: 0,
  crawling: false,
  navAt: 0,
  refresh: async () => set({ state: await api.previewState() }),
}));

/** Houdt de store gelijk aan de preview in Rust. Eén keer aanroepen bij het starten van de app. */
export function initPreviewListener(): () => void {
  void usePreview
    .getState()
    .refresh()
    .catch(() => undefined);
  const un = onEvent<PreviewState>("preview-state", (state) =>
    usePreview.setState((s) => ({
      state,
      frame: state.status === "stopped" ? null : s.frame,
      route: state.status === "stopped" ? null : s.route,
    })),
  );
  const routes = onEvent<{ route: string }>("preview-route", (e) =>
    usePreview.setState((s) => ({ route: e.route, routeSeq: s.routeSeq + 1 })),
  );
  const frames = onEvent<{ data: string; mime?: string }>("preview-frame", (f) =>
    usePreview.setState({ frame: `data:${f.mime ?? "image/jpeg"};base64,${f.data}` }),
  );
  return () => {
    void un.then((f) => f());
    void frames.then((f) => f());
    void routes.then((f) => f());
  };
}

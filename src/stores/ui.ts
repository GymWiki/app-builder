import { create } from "zustand";

export type ThemePref = "system" | "light" | "dark";
const KEY = "studio-theme";

function read(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* opslag geblokkeerd: val terug op systeem */
  }
  return "system";
}

export function resolveDark(pref: ThemePref): boolean {
  return pref === "dark" || (pref === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
}

function apply(pref: ThemePref) {
  document.documentElement.classList.toggle("dark", resolveDark(pref));
}

interface UiState {
  theme: ThemePref;
  setTheme: (t: ThemePref) => void;
}

export const useUi = create<UiState>((set) => ({
  theme: read(),
  setTheme: (theme) => {
    try {
      localStorage.setItem(KEY, theme);
    } catch {
      /* niet erg */
    }
    apply(theme);
    set({ theme });
  },
}));

/** Volgt de systeemvoorkeur zolang de gebruiker "Systeem" heeft gekozen. */
export function initTheme() {
  apply(useUi.getState().theme);
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (useUi.getState().theme === "system") apply("system");
  });
}

import { create } from "zustand";
import { onEvent } from "@/lib/ipc";
import type { ProfileStatus } from "@/lib/types";

export const useProfile = create<{ byProject: Record<string, ProfileStatus> }>(() => ({ byProject: {} }));

/** Statusregel "Claude wordt klaargezet…" uit de events van de achtergrondstap. Eén keer aanroepen bij het starten van de app. */
export function initProfileListener(): () => void {
  const un = onEvent<ProfileStatus>("profile-status", (s) =>
    useProfile.setState((st) => ({ byProject: { ...st.byProject, [s.projectId]: s } })),
  );
  return () => void un.then((f) => f());
}

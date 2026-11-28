import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/ipc";

export const qk = {
  settings: ["settings"] as const,
  appInfo: ["appInfo"] as const,
  projects: (q: string) => ["projects", q] as const,
  project: (id: string) => ["project", id] as const,
  tool: (id: string) => ["tool", id] as const,
  login: ["claudeLogin"] as const,
};

export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: api.settingsGet, staleTime: Infinity });
export const useAppInfo = () => useQuery({ queryKey: qk.appInfo, queryFn: api.appInfo, staleTime: Infinity });

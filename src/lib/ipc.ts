import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toAppError, AppError } from "./errors";
import type { AppInfo, FlutterCheck, InstallPlan, Project, Settings, TemplateId, ToolStatus } from "./types";

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Alle backend-aanroepen lopen hierdoor: getypeerde fouten, en in dev-in-de-browser een mock. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) {
    if (import.meta.env.DEV) {
      const { mockInvoke } = await import("./mock");
      return (await mockInvoke(cmd, args)) as T;
    }
    throw new AppError("unknown", "Studio must run inside its desktop shell");
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}

export async function onEvent<T>(name: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (!inTauri()) {
    if (import.meta.env.DEV) {
      const { mockListen } = await import("./mock");
      return mockListen(name, cb as (p: unknown) => void);
    }
    return () => undefined;
  }
  return listen<T>(name, (e) => cb(e.payload));
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  settingsGet: () => call<Settings>("settings_get"),
  settingsSet: (key: keyof Settings, value: string) => call<void>("settings_set", { key, value }),

  checkGit: () => call<ToolStatus>("check_git"),
  checkClaude: () => call<ToolStatus>("check_claude"),
  checkFlutter: () => call<FlutterCheck>("check_flutter"),
  claudeLoginStatus: () => call<boolean>("claude_login_status"),
  installPlan: (component: string) => call<InstallPlan | null>("install_plan", { component }),

  ptyOpenClaudeLogin: () => call<string>("pty_open_claude_login"),
  ptyOpenInstall: (component: string) => call<string>("pty_open_install", { component }),
  ptyBacklog: (id: string) => call<{ data: string; end: number }>("pty_backlog", { id }),
  ptyWrite: (id: string, data: string) => call<void>("pty_write", { id, data }),
  ptyResize: (id: string, cols: number, rows: number) => call<void>("pty_resize", { id, cols, rows }),
  ptyKill: (id: string) => call<void>("pty_kill", { id }),

  projectsList: (query?: string) => call<Project[]>("projects_list", { query: query || null }),
  projectGet: (id: string) => call<Project>("project_get", { id }),
  projectOpen: (id: string) => call<Project>("project_open", { id }),
  projectRename: (id: string, name: string) => call<Project>("project_rename", { id, name }),
  projectCreate: (args: { name: string; template: TemplateId; description: string }) =>
    call<Project>("project_create", { args }),
  projectDelete: (id: string, deleteFiles: boolean) => call<void>("project_delete", { id, deleteFiles }),
};

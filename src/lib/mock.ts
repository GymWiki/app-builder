// Alleen voor `npm run dev` in een gewone browser (zonder Tauri): een in-memory backend,
// zodat de UI zonder desktop-shell kan worden ontwikkeld en getest. Niet in productie.
import { AppError } from "./errors";
import type { Project, TemplateId, ToolStatus } from "./types";

type Handler = (p: unknown) => void;
const listeners = new Map<string, Set<Handler>>();
export function mockListen(name: string, cb: Handler) {
  if (!listeners.has(name)) listeners.set(name, new Set());
  listeners.get(name)!.add(cb);
  return () => listeners.get(name)?.delete(cb);
}
const emit = (name: string, payload: unknown) => listeners.get(name)?.forEach((cb) => cb(payload));
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const scenario = new URLSearchParams(location.search).get("mock") ?? "ready"; // ready | fresh | empty
const settings: Record<string, string> = scenario === "fresh" ? {} : { setup_completed: "true" };
const now = () => new Date().toISOString();
const ago = (m: number) => new Date(Date.now() - m * 60000).toISOString();
const mk = (
  id: string,
  name: string,
  template: TemplateId,
  status: Project["status"],
  min: number,
  done: number,
): Project => ({
  id,
  name,
  slug: id,
  path: `/home/me/Studio/${id}`,
  template,
  description: "",
  status,
  version: "1.0.0",
  buildNumber: 1,
  bundleId: `com.example.${id}`,
  createdAt: ago(min + 60),
  updatedAt: ago(min),
  lastOpenedAt: null,
  checklistDone: done,
  checklistTotal: 8,
});
let projects: Project[] =
  scenario === "ready"
    ? [
        mk("friday_scores", "Friday Scores", "scoreboard", "testing", 12, 4),
        mk("water_tracker", "Water Tracker", "tracker", "draft", 60 * 26, 1),
        mk("groceries", "Groceries", "checklist", "published", 60 * 24 * 9, 8),
      ]
    : [];

const tool = (id: ToolStatus["id"], state: ToolStatus["state"], extra: Partial<ToolStatus> = {}): ToolStatus => ({
  id,
  state,
  version: state === "ok" ? "1.0.0" : null,
  minVersion: null,
  path: null,
  detail: null,
  issue: null,
  ...extra,
});
let loggedIn = scenario !== "fresh";
let fixed = scenario !== "fresh";

export async function mockInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "app_info":
      return {
        version: "0.1.0",
        os: "windows",
        arch: "x86_64",
        projectsDir: "C:\\Users\\me\\Documents\\Studio",
        logDir: "C:\\Users\\me\\AppData\\Local\\Studio\\logs",
        minVersions: { git: "2.20.0", flutter: "3.24.0", claude: "2.1.0" },
      };
    case "settings_get":
      return { ...settings };
    case "settings_set":
      settings[args.key as string] = args.value as string;
      return;
    case "log_event":
      return;
    case "check_git":
      await sleep(400);
      return tool("git", "ok", { version: "2.47.0", minVersion: "2.20.0" });
    case "check_claude":
      await sleep(700);
      return fixed
        ? tool("claude", "ok", { version: "2.1.331", minVersion: "2.1.0" })
        : tool("claude", "missing", { minVersion: "2.1.0" });
    case "check_flutter":
      await sleep(1500);
      return {
        flutter: tool("flutter", "ok", { version: "3.27.1", minVersion: "3.24.0" }),
        android: fixed
          ? tool("android", "ok", { version: "35.0.0" })
          : tool("android", "warning", {
              issue: "licenses",
              detail:
                "Android license status unknown.\nRun `flutter doctor --android-licenses` to accept the SDK licenses.",
            }),
      };
    case "claude_login_status":
      await sleep(300);
      return loggedIn;
    case "install_plan":
      return args.component === "claude"
        ? {
            program: "winget",
            args: ["install", "--id", "Anthropic.ClaudeCode", "-e"],
            display: "winget install --id Anthropic.ClaudeCode -e",
          }
        : args.component === "android_licenses"
          ? { program: "flutter", args: ["doctor", "--android-licenses"], display: "flutter doctor --android-licenses" }
          : null;
    case "pty_open_claude_login":
    case "pty_open_install": {
      const id = "mock-pty";
      const isLogin = cmd === "pty_open_claude_login";
      setTimeout(
        () => emit("pty-data", { id, data: "\x1b[1mRunning…\x1b[0m\r\nPaste code here if prompted > ", end: 1 }),
        200,
      );
      setTimeout(() => {
        emit("pty-data", { id, data: "\r\n\x1b[32mDone.\x1b[0m\r\n", end: 2 });
        if (isLogin) loggedIn = true;
        else fixed = true;
        emit("pty-exit", { id, code: 0 });
      }, 1500);
      return id;
    }
    case "pty_backlog":
      return { data: "", end: 0 };
    case "pty_write":
    case "pty_resize":
    case "pty_kill":
      return;
    case "projects_list": {
      const q = ((args.query as string | null) ?? "").toLowerCase();
      return projects.filter((p) => p.name.toLowerCase().includes(q));
    }
    case "project_get":
    case "project_open": {
      const p = projects.find((x) => x.id === args.id);
      if (!p) throw new AppError("not_found", `project ${String(args.id)} not found`);
      return p;
    }
    case "project_create": {
      const a = args.args as { name: string; template: TemplateId; description: string };
      if (a.name.toLowerCase().includes("fail"))
        throw new AppError("tool_missing", "required tool is missing: flutter");
      for (const step of ["prepare", "flutter_create", "template", "memory", "git", "save"]) {
        emit("project-create-progress", { step });
        await sleep(step === "flutter_create" ? 1200 : 350);
      }
      const p = {
        ...mk(a.name.toLowerCase().replace(/\W+/g, "_"), a.name, a.template, "draft", 0, 0),
        updatedAt: now(),
        description: a.description,
      };
      projects = [p, ...projects];
      return p;
    }
    case "project_delete":
      projects = projects.filter((p) => p.id !== args.id);
      return;
    default:
      throw new AppError("unknown", `mock: unhandled command ${cmd}`);
  }
}

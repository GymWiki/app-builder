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
let projects: Project[] = ["ready", "nodevices", "noscreens"].includes(scenario)
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

// ── chat (nagebootst gedrag van de echte backend) ──
const chats = new Map<string, unknown[]>();
const runningChats = new Set<string>();
const cancelled = new Set<string>();
const permissionWaiters = new Map<string, (d: string) => void>();
let seq = 0;

async function mockChat(projectId: string, text: string) {
  const push = (event: Record<string, unknown>) => {
    chats.set(projectId, [...(chats.get(projectId) ?? []), ...(event.type === "text_delta" ? [] : [event])]);
    emit("chat-event", { projectId, event });
  };
  const wait = async (ms: number) => {
    await sleep(ms);
    return !cancelled.has(projectId);
  };
  const id = ++seq;
  runningChats.add(projectId);
  push({ type: "user", text, context: null });
  push({ type: "init", session_id: "mock-session", model: "mock" });
  try {
    if (text.toLowerCase().includes("login")) {
      await wait(400);
      push({
        type: "done",
        ok: false,
        text: "Not logged in · Please run /login",
        duration_ms: 300,
        code: "not_logged_in",
      });
      return;
    }
    const reply =
      "Sure! I'll add a **settings screen** and link it from the home screen.\n\n- A new `SettingsScreen` with a theme switch\n- A button on the home screen";
    for (const word of reply.split(" ")) {
      if (!(await wait(35))) return;
      push({ type: "text_delta", message_id: `m${id}`, text: word + " " });
    }
    push({ type: "text", message_id: `m${id}`, text: reply });
    push({
      type: "tool_use",
      id: `t${id}a`,
      name: "Read",
      kind: "read",
      summary: "lib/main.dart",
      path: "lib/main.dart",
      detail: null,
    });
    await wait(300);
    push({ type: "tool_result", tool_use_id: `t${id}a`, ok: true, text: "" });
    if (text.toLowerCase().includes("internet")) {
      const permId = `perm${id}`;
      emit("permission-request", {
        id: permId,
        projectId,
        tool: "WebFetch",
        kind: "network",
        summary: "https://pub.dev/packages/shared_preferences",
        detail: "https://pub.dev/packages/shared_preferences",
        key: "WebFetch(pub.dev)",
      });
      const decision = await new Promise<string>((r) => permissionWaiters.set(permId, r));
      push({
        type: "tool_use",
        id: `t${id}n`,
        name: "WebFetch",
        kind: "network",
        summary: "https://pub.dev/packages/shared_preferences",
        path: null,
        detail: "https://pub.dev/packages/shared_preferences",
      });
      await wait(300);
      push({
        type: "tool_result",
        tool_use_id: `t${id}n`,
        ok: decision !== "deny",
        text: decision === "deny" ? "The user declined this action in Studio." : "ok",
      });
    }
    push({
      type: "tool_use",
      id: `t${id}b`,
      name: "Write",
      kind: "edit",
      summary: "lib/settings_screen.dart",
      path: "lib/settings_screen.dart",
      detail: null,
    });
    await wait(600);
    push({ type: "tool_result", tool_use_id: `t${id}b`, ok: true, text: "" });
    push({
      type: "tool_use",
      id: `t${id}c`,
      name: "Bash",
      kind: "command",
      summary: "flutter analyze",
      path: null,
      detail: "flutter analyze",
    });
    await wait(600);
    push({ type: "tool_result", tool_use_id: `t${id}c`, ok: true, text: "No issues found!" });
    push({ type: "text", message_id: `m${id}b`, text: "Done! Your app now has a settings screen." });
    push({ type: "done", ok: true, text: "Done!", duration_ms: 4000, code: null });
    push({ type: "snapshot", commit: "abc1234", label: "Claude: " + text.slice(0, 40) });
  } finally {
    runningChats.delete(projectId);
    cancelled.delete(projectId);
    emit("chat-finished", { projectId });
  }
}

// ── preview ──
type MockPreview = {
  projectId: string | null;
  status: string;
  target: string;
  url: string | null;
  problems: unknown[];
  reloads: number;
};
let preview: MockPreview = { projectId: null, status: "stopped", target: "web", url: null, problems: [], reloads: 0 };
const setPreview = (p: Partial<MockPreview>) => {
  preview = { ...preview, ...p };
  emit("preview-state", preview);
};
const MOCK_PAGE =
  "data:text/html," +
  encodeURIComponent(
    "<body style='margin:0;font:16px sans-serif'><div style='background:#4f46e5;color:#fff;padding:18px'>Friday Scores</div><div style='padding:24px;text-align:center'><div style='font-size:64px'>7</div><button>+1</button></div></body>",
  );

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
    case "chat_send":
      if (runningChats.has(args.projectId as string)) throw new AppError("invalid", "busy");
      void mockChat(args.projectId as string, args.text as string);
      return;
    case "chat_cancel":
      cancelled.add(args.projectId as string);
      for (const [k, r] of permissionWaiters) {
        r("deny");
        permissionWaiters.delete(k);
      }
      return;
    case "chat_running":
      return runningChats.has(args.projectId as string);
    case "chat_history":
      return chats.get(args.projectId as string) ?? [];
    case "chat_new_session":
      chats.delete(args.projectId as string);
      return;
    case "permission_respond": {
      const r = permissionWaiters.get(args.id as string);
      r?.(args.decision as string);
      permissionWaiters.delete(args.id as string);
      return !!r;
    }
    case "project_screens":
      return projects.some((p) => p.id === args.id) && scenario !== "noscreens"
        ? [
            { class: "HomeScreen", file: "lib/main.dart", line: 24 },
            { class: "ScoreboardScreen", file: "lib/main.dart", line: 40 },
          ]
        : [];
    case "pty_open_claude":
      setTimeout(
        () => emit("pty-data", { id: "mock-claude", data: "\x1b[1mClaude Code\x1b[0m ready.\r\n> ", end: 1 }),
        300,
      );
      return "mock-claude";
    case "preview_start": {
      const id = args.projectId as string;
      setPreview({
        projectId: id,
        status: "starting",
        target: args.target as string,
        url: null,
        problems: [],
        reloads: 0,
      });
      await sleep(900);
      if (id === "water_tracker" && args.target === "web") {
        setPreview({
          status: "error",
          problems: [
            {
              file: "lib/main.dart",
              line: 12,
              message: "Expected ';' after this.",
              excerpt:
                "lib/main.dart:12:5: Error: Expected ';' after this.\n  runApp(const MyApp())\n                       ^",
            },
          ],
        });
      } else setPreview({ status: "running", url: args.target === "web" ? MOCK_PAGE : null });
      return;
    }
    case "preview_stop":
      setPreview({ status: "stopped", url: null, problems: [] });
      return;
    case "preview_state":
      return preview;
    case "preview_log":
      return [
        "Launching lib/main.dart on Web Server in debug mode...",
        "lib/main.dart is being served at http://127.0.0.1:51234",
      ];
    case "preview_reload":
      return;
    case "preview_devices":
      await sleep(300);
      return scenario === "nodevices"
        ? []
        : [
            { id: "1A2B3C", name: "Pixel 8", platform: "android-arm64", emulator: false, wireless: false },
            {
              id: "emulator-5554",
              name: "Android SDK built for x86",
              platform: "android-x64",
              emulator: true,
              wireless: false,
            },
          ];
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

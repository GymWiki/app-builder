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
  push({ type: "status", code: "scope", detail: "small|HomeScreen" });
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
    // Wat Claude in het echt met bestanden zou doen, voor de demo van de checklist.
    const low = text.toLowerCase();
    if (low.includes("privacy")) fs.docs = true;
    if (low.includes("changelog")) fs.changelog = true;
    if (low.includes("signingconfig") || low.includes("signing")) fs.gradle = true;
    if (low.includes("set_store_listing")) {
      const m = metaOf(projectId);
      m.listing = {
        title: "Friday Scores",
        shortDescription: "Keep score on game night",
        fullDescription: "A simple scoreboard for your board game nights.",
        category: "tools",
        contactEmail: "me@example.com",
      };
    }
    push({ type: "done", ok: true, text: "Done!", duration_ms: 4000, code: null });
    push({ type: "status", code: "checking", detail: null });
    push({ type: "status", code: "checks_ok", detail: null });
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

// ── fase 4 en 5 ──
const emptyMeta = () => ({
  listing: { title: "", shortDescription: "", fullDescription: "", category: "", contactEmail: "" },
  questionnaire: {
    hasAccounts: null,
    collectsPersonalData: null,
    showsAds: null,
    inAppPurchases: null,
    userGeneratedContent: null,
    usesLocation: null,
    playConsoleDone: false,
  },
  legal: { privacyUrl: "", termsUrl: "", hostedChecked: false, accountDeletion: false },
  data: { provider: "", supabaseUrl: "", supabaseProjectRef: "", authMethods: [] as string[] },
  publishing: {
    track: "",
    playAccountReady: false,
    finalConfirmed: false,
    manualChecks: {} as Record<string, boolean>,
    deviceTested: false,
  },
  lastPublishedCommit: null as string | null,
  lastPublishedVersion: null as string | null,
});
type MockMeta = ReturnType<typeof emptyMeta>;
const metas = new Map<string, MockMeta>();
const metaOf = (id: string) => metas.get(id) ?? metas.set(id, emptyMeta()).get(id)!;
const secrets = new Set<string>();
const fs = {
  icon: false,
  shots: [] as string[],
  docs: false,
  changelog: false,
  keystore: false,
  gradle: false,
  testPassed: false,
  testRan: false,
  seed: "#4F46E5" as string | null,
};
let snaps = [
  {
    id: "s3",
    commit: "c3c3c3c",
    label: "Claude: add a settings screen",
    kind: "auto",
    version: "1.0.0",
    createdAt: new Date(Date.now() - 600000).toISOString(),
  },
  {
    id: "s2",
    commit: "b2b2b2b",
    label: "Claude: make the colors teal",
    kind: "auto",
    version: "1.0.0",
    createdAt: new Date(Date.now() - 86400000).toISOString(),
  },
  {
    id: "s1",
    commit: "a1a1a1a",
    label: "Start",
    kind: "auto",
    version: "1.0.0",
    createdAt: new Date(Date.now() - 172800000).toISOString(),
  },
];
const platform = {
  appName: "Friday Scores",
  bundleId: "com.example.friday_scores",
  minAndroid: 21,
  minIos: "12.0",
  androidApplicationId: "com.example.friday_scores",
  permissions: ["internet", "camera", "location", "microphone", "photos", "notifications"].map((key) => ({
    key,
    enabled: key === "internet",
    ios_reason: null as string | null,
  })),
  hasAndroid: true,
  hasIos: true,
};
const applied = (what: string) => ({ applied: [what], skipped: [] as { target: string; reason: string }[] });

function mockEvaluate(id: string) {
  const m = metaOf(id);
  const p = projects.find((x) => x.id === id);
  const l = m.listing;
  const q = m.questionnaire;
  const answered = [
    q.hasAccounts,
    q.collectsPersonalData,
    q.showsAds,
    q.inAppPurchases,
    q.userGeneratedContent,
    q.usesLocation,
  ].every((v) => v !== null);
  const accounts = q.hasAccounts === true || m.data.authMethods.length > 0;
  const mk = (key: string, blocking: boolean, missing: string[], next: string) => ({
    key,
    blocking,
    status: missing.length ? "todo" : "done",
    missing,
    next: missing.length ? next : null,
  });
  const items = [
    mk(
      "app_basics",
      true,
      [...(platform.bundleId.startsWith("com.example.") ? ["bundle_id"] : []), ...(fs.icon ? [] : ["icon"])],
      fs.icon ? "settings" : "assets",
    ),
    mk(
      "data_auth",
      true,
      [...(m.data.provider ? [] : ["provider"]), ...(accounts && !m.legal.accountDeletion ? ["account_deletion"] : [])],
      "data",
    ),
    mk(
      "legal",
      true,
      [
        ...(fs.docs ? [] : ["docs"]),
        ...(m.legal.privacyUrl ? [] : ["privacy_url"]),
        ...(m.legal.termsUrl ? [] : ["terms_url"]),
        ...(m.legal.privacyUrl && !m.legal.hostedChecked ? ["hosted_checked"] : []),
      ],
      "publish:legal",
    ),
    mk("ai_test_run", true, fs.testPassed ? [] : [fs.testRan ? "failed" : "never"], "publish:test"),
    mk(
      "store_listing",
      true,
      [
        ...(l.title ? [] : ["title"]),
        ...(l.shortDescription ? [] : ["short_description"]),
        ...(l.fullDescription ? [] : ["full_description"]),
        ...(l.category ? [] : ["category"]),
        ...(l.contactEmail ? [] : ["contact_email"]),
        ...(fs.shots.length >= 2 ? [] : ["screenshots"]),
        ...(!answered ? ["questionnaire"] : !q.playConsoleDone ? ["play_console"] : []),
      ],
      "publish:listing",
    ),
    mk(
      "accounts_signing",
      true,
      [
        ...(fs.keystore ? [] : ["keystore"]),
        ...(fs.gradle ? [] : ["gradle"]),
        ...(m.publishing.playAccountReady ? [] : ["play_account"]),
      ],
      "publish:signing",
    ),
    mk(
      "version_changelog",
      true,
      [...(m.lastPublishedVersion === p?.version ? ["version_bump"] : []), ...(fs.changelog ? [] : ["changelog"])],
      "versions",
    ),
  ];
  const others = items.every((i) => i.status === "done");
  items.push({
    ...mk(
      "final_confirmation",
      true,
      !others ? ["others"] : m.publishing.finalConfirmed ? [] : ["confirm"],
      "publish:confirm",
    ),
  });
  items.push(mk("device_test", false, m.publishing.deviceTested ? [] : ["device"], "preview"));
  items.push(
    mk(
      "native_manual",
      false,
      ["camera"].filter((k) => !m.publishing.manualChecks[k]),
      "publish:test",
    ),
  );
  const blocking = items.filter((i) => i.blocking);
  const done = blocking.filter((i) => i.status === "done").length;
  return { items, canPublish: done === blocking.length, done, total: blocking.length };
}

const MOCK_REPORT = {
  startedOk: true,
  tapped: 9,
  durationMs: 5200,
  screens: [
    {
      name: "Friday Scores",
      depth: 0,
      taps: 6,
      issues: [{ kind: "dead_button", severity: "warning", message: "A TextButton seems to do nothing", detail: "" }],
      screenshot: null,
    },
    { name: "Settings", depth: 1, taps: 3, issues: [], screenshot: null },
  ],
  errors: [],
};
const emitJob = (projectId: string, job: string, status: string) => emit("job-state", { projectId, job, status });
const emitLine = (projectId: string, job: string, line: string) => emit("job-log", { projectId, job, line });

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
    case "theme_get":
      return { managed: fs.seed !== null, seedColor: fs.seed };
    case "theme_set":
      fs.seed = (args.seedColor as string).toUpperCase();
      return { ok: true, wired: true, next: "" };
    case "screens_graph":
      return scenario === "noscreens"
        ? { home: null, nodes: [], unlinked: [] }
        : {
            home: "HomeScreen",
            nodes: [
              { class: "HomeScreen", file: "lib/main.dart", line: 24, children: ["SettingsScreen"] },
              { class: "SettingsScreen", file: "lib/settings_screen.dart", line: 5, children: ["AboutPage"] },
              { class: "AboutPage", file: "lib/about_page.dart", line: 3, children: [] },
              { class: "LonelyScreen", file: "lib/lonely.dart", line: 1, children: [] },
            ],
            unlinked: ["LonelyScreen"],
          };
    case "platform_get":
      return platform;
    case "platform_set_name":
      platform.appName = args.name as string;
      return applied("Android + iOS");
    case "platform_set_bundle":
      if (args.bundleId === "fail.demo")
        return { applied: [], skipped: [{ target: "iOS", reason: "project.pbxproj not found" }] };
      platform.bundleId = platform.androidApplicationId = args.bundleId as string;
      return applied("Android: applicationId, iOS: PRODUCT_BUNDLE_IDENTIFIER");
    case "platform_set_min_os":
      if (args.android) platform.minAndroid = args.android as number;
      if (args.ios) platform.minIos = args.ios as string;
      return applied("minSdk, deployment target");
    case "platform_set_permission": {
      const perm = platform.permissions.find((x) => x.key === args.key)!;
      perm.enabled = args.enabled as boolean;
      perm.ios_reason = args.enabled ? (args.iosReason as string) : null;
      return applied("AndroidManifest.xml, Info.plist");
    }
    case "version_set": {
      const p = projects.find((x) => x.id === args.projectId);
      if (p) {
        p.version = args.version as string;
        p.buildNumber = args.build as number;
      }
      return;
    }
    case "assets_generate_icon":
      fs.icon = true;
      return { written: new Array(26).fill("x"), warnings: [] };
    case "snapshots_list":
      return snaps;
    case "snapshot_create":
      snaps = [
        {
          id: `s${snaps.length + 1}`,
          commit: `d${snaps.length}d${snaps.length}d`,
          label: args.label as string,
          kind: "manual",
          version: "1.0.0",
          createdAt: now(),
        },
        ...snaps,
      ];
      return { created: true };
    case "snapshot_restore": {
      const t = snaps.find((x) => x.commit === args.commit);
      snaps = [
        {
          id: `r${snaps.length}`,
          commit: `e${snaps.length}e${snaps.length}e`,
          label: `Restored: ${t?.label}`,
          kind: "manual",
          version: "1.0.0",
          createdAt: now(),
        },
        ...snaps,
      ];
      return { restored: true };
    }
    case "changelog_draft":
      return snaps.filter((x) => x.kind === "auto" && x.label !== "Start").map((x) => x.label);
    case "meta_get":
      return metaOf(args.projectId as string);
    case "meta_save": {
      const id = args.projectId as string;
      const m = args.meta as MockMeta;
      if (m.listing.title.length > 30) throw new AppError("invalid", "store title can be at most 30 characters");
      metas.set(id, {
        ...m,
        lastPublishedCommit: metaOf(id).lastPublishedCommit,
        lastPublishedVersion: metaOf(id).lastPublishedVersion,
      });
      return;
    }
    case "secret_set":
      secrets.add(`${args.projectId}:${args.name}`);
      return;
    case "secret_has":
      return secrets.has(`${args.projectId}:${args.name}`);
    case "secret_delete":
      secrets.delete(`${args.projectId}:${args.name}`);
      return;
    case "data_setup_prompt":
      return "Set up the data layer for this app.";
    case "checklist_evaluate":
      return mockEvaluate(args.projectId as string);
    case "store_screenshots_list":
      return fs.shots;
    case "store_screenshot_capture": {
      if (preview.status !== "running") throw new AppError("invalid", "start the live preview first");
      const n = `phone-0${fs.shots.length + 1}.png`;
      fs.shots = [...fs.shots, n];
      return n;
    }
    case "store_screenshot_import":
      fs.shots = [...fs.shots, ...(args.paths as string[]).map((_, i) => `imported-0${fs.shots.length + i + 1}.png`)];
      return fs.shots;
    case "store_screenshot_delete":
      fs.shots = fs.shots.filter((x) => x !== args.name);
      return;
    case "file_data_url":
      return (
        "data:image/svg+xml," +
        encodeURIComponent(
          "<svg xmlns='http://www.w3.org/2000/svg' width='90' height='160'><rect width='90' height='160' fill='%234f46e5'/></svg>",
        )
      );
    case "testrun_start": {
      const id = args.projectId as string;
      emitJob(id, "test", "running");
      for (const l of ["00:01 +0: Studio crawl", "00:04 +1: Studio crawl"]) {
        await sleep(500);
        emitLine(id, "test", l);
      }
      fs.testRan = true;
      fs.testPassed = !(metaOf(id).listing.title === "FAIL");
      emitJob(id, "test", fs.testPassed ? "done" : "failed");
      return MOCK_REPORT;
    }
    case "testrun_report":
      return {
        errors: 0,
        warnings: fs.testRan ? 1 : 0,
        report: fs.testRan ? MOCK_REPORT : null,
        passed: fs.testPassed,
        stale: false,
        nativeFeatures: ["camera"],
        notTested: ["camera", "push", "payments", "sensors", "real_device", "performance"],
      };
    case "signing_status":
      return {
        keytool: true,
        keystore: fs.keystore,
        keystorePath: fs.keystore
          ? "C:\\Users\\me\\AppData\\Roaming\\app.studio.desktop\\keystores\\x\\upload-keystore.jks"
          : null,
        gradleConfigured: fs.gradle,
        playKey: secrets.has(`${args.projectId}:play_service_account_json`),
        fastlane: true,
      };
    case "signing_create_keystore":
      await sleep(500);
      fs.keystore = true;
      return "keystore";
    case "signing_prompt":
      return "Configure release signing from environment variables.";
    case "publish_android": {
      const id = args.projectId as string;
      if (!args.confirmed) throw new AppError("invalid", "publishing needs confirmation");
      emitJob(id, "build", "running");
      for (const l of [
        "Running Gradle task 'bundleRelease'...",
        "✓ Built build/app/outputs/bundle/release/app-release.aab",
      ]) {
        await sleep(400);
        emitLine(id, "build", l);
      }
      emitJob(id, "build", "done");
      emitJob(id, "upload", "running");
      await sleep(600);
      emitLine(id, "upload", "Uploading to internal track as draft…");
      emitJob(id, "upload", "done");
      const p = projects.find((x) => x.id === id);
      metaOf(id).lastPublishedVersion = p?.version ?? null;
      metaOf(id).publishing.finalConfirmed = false;
      return { version: p?.version, track: args.track };
    }
    case "publish_ios":
      throw new AppError("unsupported", "not supported yet: ios_publish");
    case "pty_write":
    case "pty_resize":
    case "pty_kill":
      return;
    case "projects_list": {
      const q = ((args.query as string | null) ?? "").toLowerCase();
      return projects.filter((p) => p.name.toLowerCase().includes(q));
    }
    case "profile_info":
      return { version: 1, current: 1, plugin: true, dart: "3.14.0", dartMcpOk: true, structure: "feature-first" };
    case "profile_apply":
      return;
    case "run_metrics":
      return {
        runs: 12,
        byLevel: { small: 7, screen: 3, shared: 1, project: 1 },
        expanded: 2,
        cleanFirstTime: 9,
        repaired: 2,
        rolledBack: 1,
        aiQuestions: 1,
        avgDurationMs: 41000,
        totalCostUsd: 3.42,
        avgFilesChanged: 2.3,
        deniedEdits: 3,
      };
    case "project_get":
    case "project_open": {
      const p = projects.find((x) => x.id === args.id);
      if (!p) throw new AppError("not_found", `project ${String(args.id)} not found`);
      if (cmd === "project_open") {
        emit("profile-status", { projectId: p.id, state: "working", reason: null });
        setTimeout(() => emit("profile-status", { projectId: p.id, state: "ready", reason: null }), 1500);
      }
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

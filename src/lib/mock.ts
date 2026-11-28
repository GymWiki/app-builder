// Alleen voor `npm run dev` in een gewone browser (zonder Tauri): een in-memory backend,
// zodat de UI zonder desktop-shell kan worden ontwikkeld en getest. Niet in productie.
import {
  mockLeaks,
  vaultHas,
  vaultInvoke,
  vaultMissing,
  vaultPreloadForPurchases,
  vaultSetKeystore,
} from "./mock-vault";
import { AppError } from "./errors";
import * as pur from "./mock-purchases";
import type { Project, Purchases, PurchasesView, TemplateId, ToolStatus } from "./types";

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
/** Laatst "geüploade" versie per project (de demo kent er maar een paar). */
const published: Record<string, { version: string; build: number }> = { friday_scores: { version: "1.0.0", build: 1 } };
function mockVersionInfo(p?: Project) {
  const last = p ? published[p.id] : undefined;
  const cur = (p?.version ?? "1.0.0").split(".").map(Number) as [number, number, number];
  let next: [number, number, number] = cur;
  let build = p?.buildNumber ?? 1;
  if (last) {
    const lv = last.version.split(".").map(Number) as [number, number, number];
    const le = cur[0] * 1e6 + cur[1] * 1e3 + cur[2] <= lv[0] * 1e6 + lv[1] * 1e3 + lv[2];
    if (le) next = [lv[0], lv[1], lv[2] + 1];
    if (build <= last.build) build = last.build + 1;
  }
  return {
    version: p?.version ?? "1.0.0",
    build: p?.buildNumber ?? 1,
    lastPublished: last ? { version: last.version, build: last.build } : null,
    nextVersion: next.join("."),
    nextBuild: build,
    willChange: next.join(".") !== (p?.version ?? "1.0.0") || build !== (p?.buildNumber ?? 1),
  };
}
if (scenario === "purchases") vaultPreloadForPurchases("friday_scores");
let enginesOrder = ["claude-code"];
const enginesLimited = new Map<string, number>();
const enginesView = () => ({
  order: enginesOrder,
  engines: [
    {
      id: "claude-code",
      name: "Claude Code",
      provider: "Anthropic",
      installed: true,
      version: "2.1.0 (Claude Code)",
      loggedIn: true,
      proven: true,
      installHint: "winget install Anthropic.ClaudeCode",
      docs: "",
      limitedUntil: enginesLimited.get("claude-code") ?? null,
      runs: 42,
      clean: 36,
      rolledBack: 2,
    },
    {
      id: "codex",
      name: "Codex CLI",
      provider: "OpenAI",
      installed: true,
      version: "codex-cli 0.172.0",
      loggedIn: false,
      proven: false,
      installHint: "npm install -g @openai/codex",
      docs: "",
      limitedUntil: enginesLimited.get("codex") ?? null,
      runs: 0,
      clean: 0,
      rolledBack: 0,
    },
    {
      id: "gemini",
      name: "Gemini CLI",
      provider: "Google",
      installed: false,
      version: null,
      loggedIn: false,
      proven: false,
      installHint: "npm install -g @google/gemini-cli",
      docs: "",
      limitedUntil: null,
      runs: 0,
      clean: 0,
      rolledBack: 0,
    },
  ],
});
if (scenario === "limit") enginesLimited.set("claude-code", Math.floor(Date.now() / 1000) + 7200);
const purchasesStore = new Map<string, Purchases>();
const purchasesApplying = new Set<string>();
const purchasesOf = (id: string): Purchases => {
  if (!purchasesStore.has(id)) purchasesStore.set(id, pur.emptyPurchases());
  return purchasesStore.get(id)!;
};
const purchasesView = (id: string): PurchasesView =>
  pur.view(
    purchasesOf(id),
    id,
    projects.find((p) => p.id === id)?.bundleId ?? "com.example.app",
    purchasesApplying.has(id),
  );
let projects: Project[] = ["ready", "nodevices", "noscreens", "inspect", "purchases", "iframe", "slowstart", "nopreview", "update", "licstall"].includes(scenario)
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
let fixed = scenario !== "fresh" && scenario !== "licstall";

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
  // Over aankopen begint Claude niet zelf aan code: Studio zet een concept klaar en opent de pagina Aankopen (zoals de echte backend).
  if (!text.startsWith("STUDIO ") && /abonnement|subscription|paywall|premium|betaalmuur|aankop|purchase/i.test(text)) {
    await sleep(300);
    const pu = purchasesOf(projectId);
    if (!pu.products.length) {
      const sub = /abonnement|subscription/i.test(text);
      pu.products.push({
        id: sub ? "pro_yearly" : "pro",
        kind: sub ? "subscription" : "oneTime",
        name: "Pro",
        description: "",
        basePrice: 0,
        currency: "EUR",
        period: sub ? "year" : null,
        trialDays: null,
        entitlement: "pro",
        status: { apple: "concept", google: "concept" },
      });
      pu.paywallDescription = text;
    }
    const reply = "For purchases there is a dedicated page; I have already set up a draft for you.";
    push({ type: "text", message_id: `m${id}`, text: reply });
    push({ type: "done", ok: true, text: reply, duration_ms: 0 });
    emit("open-purchases", { projectId });
    runningChats.delete(projectId);
    emit("chat-finished", { projectId });
    return;
  }
  push({ type: "status", code: "scope", detail: "small|HomeScreen" });
  push({
    type: "status",
    code: "model",
    detail: "trivial|light|haiku|trivial class|scope small = 0; kind text_change = 0",
  });
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
    const reply = text.includes("STUDIO GATING PLAN")
      ? '```json\n[{"kind":"screen","name":"Statistics","entitlement":"pro","file":"lib/stats_screen.dart","line":7},{"kind":"button","name":"Export","entitlement":"pro","file":"lib/stats_screen.dart","line":31}]\n```'
      : "Sure! I'll add a **settings screen** and link it from the home screen.\n\n- A new `SettingsScreen` with a theme switch\n- A button on the home screen";
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
  mode: string;
};
let preview: MockPreview = {
  projectId: null,
  status: "stopped",
  target: "web",
  url: null,
  problems: [],
  reloads: 0,
  mode: "iframe",
};
const setPreview = (p: Partial<MockPreview>) => {
  preview = { ...preview, ...p };
  emit("preview-state", preview);
};
/** Beeldstroom van de demo (inspect-modus): een tekening van de pagina op de huidige route, elke halve seconde opnieuw verstuurd. */
let framesTimer: ReturnType<typeof setInterval> | null = null;
let mockRoute = "/";
const ROUTE_TITLES: Record<string, string> = {
  "/": "Friday Scores",
  "/settings": "Settings",
  "/profile": "Account",
  "/item/1": "Item 1",
};
function mockFrameSvg(route: string): string {
  const title = ROUTE_TITLES[route] ?? route;
  return `<svg xmlns='http://www.w3.org/2000/svg' width='360' height='740'><rect width='360' height='740' fill='white'/><rect width='360' height='56' fill='#4f46e5'/><text x='16' y='34' fill='white' font-family='sans-serif' font-size='18'>${title}</text><text x='180' y='170' font-size='72' text-anchor='middle' font-family='sans-serif'>7</text><rect x='140' y='200' width='80' height='36' rx='18' fill='#e0e7ff'/><text x='180' y='224' font-size='16' text-anchor='middle' font-family='sans-serif'>+1</text></svg>`;
}
const sendMockFrame = () =>
  emit("preview-frame", { data: btoa(mockFrameSvg(mockRoute)), mime: "image/svg+xml", width: 360, height: 740 });
function startMockFrames() {
  if (framesTimer) clearInterval(framesTimer);
  mockRoute = "/";
  emit("preview-route", { route: "/" });
  sendMockFrame();
  framesTimer = setInterval(
    () => (preview.status === "stopped" ? framesTimer && clearInterval(framesTimer) : sendMockFrame()),
    500,
  );
}
let licenseConsent = false;
let licenseCancelled = false;
/** Gesimuleerde licentie-flow: 7 vragen, of (mock=licstall) een vastloper die met de andere methode wel lukt. */
async function mockLicenses(alternative: boolean) {
  licenseCancelled = false;
  const stall = scenario === "licstall" && !alternative;
  emit("license-progress", { accepted: 0, total: null, phase: "starting" });
  await sleep(300);
  if (stall) {
    await sleep(700);
    emit("license-progress", { accepted: 0, total: null, phase: "stalled" });
    for (let i = 0; i < 6 && !licenseCancelled; i++) await sleep(100);
    emit("license-done", {
      result: { ok: false, accepted: 0, total: null, already: false, stalled: true, cancelled: licenseCancelled, exit: null, method: "sdkmanager", log: "" },
      problems: licenseCancelled ? [] : [{ code: "java_missing" }],
      check: null,
      confirmed: false,
      seconds: 20,
      alternative,
    });
    return;
  }
  for (let i = 1; i <= 7; i++) {
    await sleep(250);
    if (licenseCancelled) {
      emit("license-done", {
        result: { ok: false, accepted: i - 1, total: 7, already: false, stalled: false, cancelled: true, exit: null, method: "sdkmanager", log: "" },
        problems: [],
        check: null,
        confirmed: false,
        seconds: 2,
        alternative,
      });
      return;
    }
    emit("license-progress", { accepted: i, total: 7, phase: i === 7 ? "done" : "asking" });
  }
  fixed = true;
  emit("license-done", {
    result: { ok: true, accepted: 7, total: 7, already: false, stalled: false, cancelled: false, exit: 0, method: alternative ? "flutter" : "sdkmanager", log: "All SDK package licenses accepted." },
    problems: [],
    check: {
      flutter: tool("flutter", "ok", { version: "3.27.1", minVersion: "3.24.0" }),
      android: tool("android", "ok", { version: "35.0.0" }),
    },
    confirmed: true,
    seconds: 3,
    alternative,
  });
}
const mockShots = new Map<string, string>();
const uxCounts: Record<string, Record<string, number>> = {};
// Alleen voor de demo-tests: een route-gebeurtenis alsof de gebruiker in de preview zelf navigeert.
(window as unknown as { __mockEmit: typeof emit }).__mockEmit = emit;
const MOCK_PAGE =
  "data:text/html," +
  encodeURIComponent(
    "<body style='margin:0;font:16px sans-serif'><div style='background:#4f46e5;color:#fff;padding:18px'>Friday Scores</div><div style='padding:24px;text-align:center'><div style='font-size:64px'>7</div><button>+1</button></div></body>",
  );

async function mockPreviewStart(id: string, target: string) {
  setPreview({
    projectId: id,
    status: "starting",
    target,
    url: null,
    problems: [],
    reloads: 0,
    mode: scenario !== "iframe" && target === "web" ? "inspect" : "iframe",
  });
  await sleep(scenario === "slowstart" ? 3000 : 900);
  if (id === "water_tracker" && target === "web") {
    setPreview({
      status: "error",
      problems: [
        {
          file: "lib/main.dart",
          line: 12,
          message: "Expected ';' after this.",
          excerpt: "lib/main.dart:12:5: Error: Expected ';' after this.\n  runApp(const MyApp())\n                       ^",
        },
      ],
    });
  } else {
    setPreview({ status: "running", url: target === "web" ? MOCK_PAGE : null });
    if (preview.mode === "inspect") startMockFrames();
  }
}

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
  font: null as string | null,
  kit: null as string | null,
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

const MOCK_FIXABLE = new Set([
  "app_basics:version",
  "store_listing:title",
  "store_listing:short_description",
  "store_listing:full_description",
  "version_changelog:changelog",
]);
function mockEvaluate(id: string) {
  const m = metaOf(id);
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
    fixable: missing.filter((m) => MOCK_FIXABLE.has(`${key}:${m}`)),
  });
  /** Punt met ontbrekende geheimen uit de (gesimuleerde) kluis: `secret:<integratie>.<veld>` en een knop naar dat kopje. */
  const secretItem = (projectId: string, key: string, other: string[], otherNext: string, secretFirst = false) => {
    const gaps = vaultMissing(projectId).filter((g) => g.items.includes(key));
    const missing = [...gaps.map((g) => `secret:${g.integration}.${g.field}`), ...other];
    const g0 = gaps[0];
    const next =
      g0 && (secretFirst || other.length === 0)
        ? `${g0.enabled ? "secrets" : "integrations"}:${g0.integration}`
        : otherNext;
    return mk(key, true, missing, next);
  };
  const leaks = mockLeaks().length;
  const items = [
    mk(
      "app_basics",
      true,
      [...(platform.bundleId.startsWith("com.example.") ? ["bundle_id"] : []), ...(fs.icon ? [] : ["icon"])],
      fs.icon ? "settings" : "assets",
    ),
    secretItem(
      id,
      "data_auth",
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
    secretItem(
      id,
      "accounts_signing",
      [...(fs.gradle ? [] : ["gradle"]), ...(m.publishing.playAccountReady ? [] : ["play_account"])],
      "publish:signing",
      true,
    ),
    mk("version_changelog", true, [...(fs.changelog ? [] : ["changelog"])], "versions"),
  ];
  items.push({ ...mk("no_leaks", true, leaks ? [`leaks:${leaks}`] : [], "leaks"), status: leaks ? "failed" : "done" });
  // Aankopen: alleen als de app producten heeft (zoals de echte checklist).
  const pu = purchasesOf(id);
  if (pu.products.length > 0) {
    const n = pu.products.length;
    const created = (s: "apple" | "google") => pu.products.filter((x) => pur.rank(x.status[s]) >= 2).length;
    const c = (k: string) => !!pu.confirmations[k];
    const legalOk = !!m.legal.privacyUrl && !!m.legal.termsUrl;
    items.push(
      mk(
        "purchases_products",
        true,
        [
          ...(pu.appliedVersion > 0 ? [] : ["not_applied"]),
          ...(pu.apple && created("apple") < n ? ["apple_products"] : []),
          ...(pu.google && created("google") < n ? ["google_products"] : []),
        ],
        "purchases",
      ),
      mk(
        "purchases_agreements",
        true,
        [
          ...(pu.apple && !c("paid_apps_agreement") ? ["paid_apps_agreement"] : []),
          ...(pu.apple && !c("tax_bank") ? ["tax_bank"] : []),
          ...(pu.google && !c("merchant_account") ? ["merchant_account"] : []),
        ],
        "purchases",
      ),
      mk(
        "purchases_paywall",
        true,
        [...(pu.appliedVersion > 0 ? [] : ["paywall"]), ...(legalOk ? [] : ["legal_links"])],
        pu.appliedVersion > 0 && !legalOk ? "publish:legal" : "purchases",
      ),
      mk("purchases_tested", true, c("sandbox_tested") ? [] : ["sandbox"], "purchases"),
      ...(pu.apple ? [mk("purchases_apple_submit", true, c("apple_submitted") ? [] : ["submit"], "purchases")] : []),
    );
  }
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
  const v = await vaultInvoke(cmd, args);
  if (v.handled) return v.value;
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
            args: ["install", "--id", "Anthropic.ClaudeCode", "-e", "--silent", "--accept-package-agreements", "--accept-source-agreements"],
            display:
              "winget install --id Anthropic.ClaudeCode -e --silent --accept-package-agreements --accept-source-agreements",
          }
        : null;
    case "android_licenses_status":
      return { consent: licenseConsent };
    case "android_licenses_accept": {
      if (!args.agree && !licenseConsent) throw new AppError("invalid", "consent required");
      licenseConsent = true;
      void mockLicenses(!!args.alternative);
      return;
    }
    case "android_licenses_cancel":
      licenseCancelled = true;
      return;
    case "install_run": {
      const component = args.component as string;
      void (async () => {
        for (const line of ["Found Anthropic Claude Code", "Downloading…", "Successfully installed"]) {
          await sleep(300);
          emit("install-line", { component, line });
        }
        fixed = true;
        emit("install-done", { component, ok: true, exit: 0, log: "" });
      })();
      return;
    }
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
    case "preview_start":
      return mockPreviewStart(args.projectId as string, args.target as string);
    case "preview_navigate": {
      const r = args.route as string;
      if (!/^\/[A-Za-z0-9_\-./:?=&%~]*$/.test(r)) throw new AppError("invalid", "invalid route");
      if (preview.mode !== "inspect") throw new AppError("unsupported", "preview navigation");
      mockRoute = r;
      emit("preview-route", { route: r });
      setTimeout(sendMockFrame, 150);
      return;
    }
    case "preview_shot_save":
      mockShots.set(`${args.projectId}:${args.screen}`, `data:image/jpeg;base64,${args.data}`);
      return;
    case "preview_shot_get":
      return mockShots.get(`${args.projectId}:${args.screen}`) ?? null;
    case "preview_shots_list":
      return [...mockShots.keys()].filter((k) => k.startsWith(`${args.projectId}:`)).map((k) => k.split(":")[1]);
    case "preview_stop":
      setPreview({ status: "stopped", url: null, problems: [] });
      return;
    case "engines_list":
      return enginesView();
    case "engines_set_order":
      enginesOrder = (args.order as string[]).filter((x, i, a) => a.indexOf(x) === i);
      if (!enginesOrder.length) enginesOrder = ["claude-code"];
      return enginesView();
    case "engine_clear_limit":
      enginesLimited.delete(args.id as string);
      return enginesView();
    case "engine_open_login": {
      setTimeout(
        () =>
          emit("pty-data", {
            id: "mock-pty",
            data: "\x1b[1mCodex login\x1b[0m\r\nOpen the browser to sign in…\r\n",
            end: 1,
          }),
        200,
      );
      return "mock-pty";
    }
    case "purchases_suggest_id":
      return pur.suggestId(args.name as string, args.kind as never, args.period as never);
    case "purchases_get":
      return purchasesView(args.projectId as string);
    case "purchases_save": {
      const id = args.projectId as string;
      const disk = purchasesOf(id);
      const next = args.purchases as Purchases;
      next.applied = disk.applied;
      next.appliedVersion = disk.appliedVersion;
      next.confirmations = disk.confirmations;
      for (const pr of next.products) {
        const old = disk.products.find((o) => o.id === pr.id);
        pr.status = old ? old.status : { apple: "concept", google: "concept" };
      }
      next.gating.approved =
        disk.gating.approved && JSON.stringify(disk.gating.points) === JSON.stringify(next.gating.points);
      purchasesStore.set(id, next);
      return purchasesView(id);
    }
    case "purchases_approve_gating":
      purchasesOf(args.projectId as string).gating.approved = true;
      return purchasesView(args.projectId as string);
    case "purchases_confirm":
      purchasesOf(args.projectId as string).confirmations[args.key as string] = args.value as boolean;
      return purchasesView(args.projectId as string);
    case "purchases_set_status": {
      const pu = purchasesOf(args.projectId as string);
      const pr = pu.products.find((x) => x.id === args.product);
      if (!pr) throw new AppError("not_found", args.product as string);
      const store = args.store as "apple" | "google";
      if (
        pur.rank(pr.status[store]) < 1 ||
        pur.rank(args.status as never) <= pur.rank(pr.status[store]) ||
        pur.rank(args.status as never) < 2
      )
        throw new AppError("invalid", "a status can only move forward");
      pr.status[store] = args.status as never;
      const ap = (pu.applied as { products: { id: string; status: unknown }[] } | null)?.products.find(
        (x) => x.id === args.product,
      );
      if (ap) ap.status = { ...pr.status };
      return purchasesView(args.projectId as string);
    }
    case "purchases_gating_prompt":
      return "STUDIO GATING PLAN. Do NOT change any file.";
    case "purchases_set_gating_from_answer": {
      const text = String(args.answer);
      const raw = text.includes("```") ? text.split("```")[1]!.replace(/^json/, "") : text;
      let pts: Purchases["gating"]["points"] = [];
      try {
        pts = (JSON.parse(raw) as Purchases["gating"]["points"]).filter((x) => x.name);
      } catch {
        /* geen bruikbaar plan */
      }
      if (!pts.length) throw new AppError("invalid", "no usable gating plan");
      const pu = purchasesOf(args.projectId as string);
      pu.gating = { points: pts, approved: false };
      return purchasesView(args.projectId as string);
    }
    case "purchases_apply": {
      const id = args.projectId as string;
      const v = purchasesView(id);
      if (v.missing[0]) throw new AppError("rejected", `purchases_${v.missing[0].code}:${v.missing[0].next}`);
      purchasesApplying.add(id);
      emit("purchases-progress", { projectId: id, step: "snapshot", detail: null });
      emit("purchases-progress", { projectId: id, step: "code", detail: null });
      setTimeout(() => {
        pur.finalize(purchasesOf(id));
        purchasesApplying.delete(id);
        emit("purchases-progress", { projectId: id, step: "done", detail: null });
      }, 1500);
      return;
    }
    case "preview_viewport":
    case "chat_set_selection":
      return;
    case "preview_select": {
      // Nagebootst: bovenin de titel van de app, daaronder het getal en de knop.
      const y = (args.bbox as number[])[1] ?? 0;
      const file = "lib/home_screen.dart";
      const t =
        y < 12
          ? { widget: "Text", text: "Friday Scores", file, startLine: 12, endLine: 12 }
          : y < 30
            ? { widget: "Text", text: "7", file, startLine: 24, endLine: 24 }
            : { widget: "PrimaryButton", text: "+1", file, startLine: 30, endLine: 33 };
      await sleep(400);
      const selection = {
        screen: "HomeScreen",
        screenFile: file,
        bbox: args.bbox,
        targets: [{ ...t, chain: [], viaShared: t.widget === "PrimaryButton" }],
      };
      return {
        selection,
        prompt: `The user marked part of the live preview and wants ONLY that part changed.\n1. ${t.widget} "${t.text}" — ${t.file} lines ${t.startLine}–${t.endLine}`,
      };
    }
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
      return { managed: fs.seed !== null, seedColor: fs.seed, font: fs.font, kit: fs.kit, deviation: false };
    case "theme_apply": {
      const patch = args.patch as { seedColor?: string; font?: string; kit?: string };
      if (patch.seedColor) fs.seed = patch.seedColor.toUpperCase();
      if (patch.font !== undefined) fs.font = patch.font || null;
      if (patch.kit !== undefined) fs.kit = patch.kit && patch.kit !== "material" ? patch.kit : null;
      uxCounts.theme = { ...(uxCounts.theme ?? {}), deterministic: (uxCounts.theme?.deterministic ?? 0) + 1 };
      return { changed: [{ area: "theme", file: "lib/core/theme/app_theme.dart" }], needsPubGet: false, wired: true, notes: [], pubGetFailed: false };
    }
    case "data_apply": {
      const m = metaOf(args.projectId as string);
      const prov = m.data.provider;
      const accounts = (prov === "supabase" || prov === "firebase") && m.data.authMethods.some((a) => a === "email" || a === "anonymous");
      const notes = [
        ...(m.data.authMethods.some((a) => a === "google" || a === "apple") ? ["auth_social_manual"] : []),
        ...(prov === "firebase" ? ["firebase_options"] : []),
        ...(prov === "supabase" && accounts ? ["supabase_delete_sql"] : []),
      ];
      if (prov === "firebase" && accounts) m.legal.accountDeletion = true;
      uxCounts.data = { ...(uxCounts.data ?? {}), deterministic: (uxCounts.data?.deterministic ?? 0) + (accounts ? 5 : 2) };
      return {
        changed: [{ area: "data", file: "lib/core/data/studio_data.dart" }, ...(accounts ? [{ area: "data", file: "lib/features/auth/login_screen.dart" }] : [])],
        pubAdd: [],
        notes,
        pubAddFailed: false,
        accountDeletionSet: prov === "firebase" && accounts,
      };
    }
    case "ux_summary": {
      const det = Object.values(uxCounts).reduce((n, m) => n + (m.deterministic ?? 0), 0);
      return { flows: uxCounts, avgTokensPerAiRun: 8000, estimatedTokensSaved: det * 8000 };
    }
    case "ux_event":
      uxCounts[args.flow as string] = {
        ...(uxCounts[args.flow as string] ?? {}),
        [args.kind as string]: (uxCounts[args.flow as string]?.[args.kind as string] ?? 0) + 1,
      };
      return;
    case "theme_set":
      fs.seed = (args.seedColor as string).toUpperCase();
      return { ok: true, wired: true, next: "" };
    case "screens_graph":
      return scenario === "noscreens"
        ? { home: null, nodes: [], unlinked: [] }
        : {
            home: "HomeScreen",
            nodes: [
              { class: "HomeScreen", file: "lib/main.dart", line: 24, children: ["SettingsScreen", "ProfileScreen", "ItemScreen"], route: "/" },
              {
                class: "SettingsScreen",
                file: "lib/settings_screen.dart",
                line: 5,
                children: ["AboutPage"],
                route: "/settings",
              },
              { class: "AboutPage", file: "lib/about_page.dart", line: 3, children: [] },
              {
                class: "ProfileScreen",
                file: "lib/profile_screen.dart",
                line: 2,
                children: [],
                route: "/profile",
              },
              {
                class: "ItemScreen",
                file: "lib/item_screen.dart",
                line: 2,
                children: [],
                route: "/item/:id",
                previewRoute: "/item/1",
              },
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
    case "version_info":
      return mockVersionInfo(projects.find((x) => x.id === args.projectId));
    case "version_set": {
      const p = projects.find((x) => x.id === args.projectId);
      if (p) {
        const last = published[p.id];
        if (last && (args.build as number) <= last.build) throw { kind: "invalid", message: "build too low" };
        p.version = args.version as string;
        p.buildNumber = args.build as number;
      }
      return mockVersionInfo(p);
    }
    case "assets_generate_icon":
      fs.icon = true;
      return { written: new Array(26).fill("x"), warnings: [] };
    case "assets_splash_apply":
      uxCounts.assets = { ...(uxCounts.assets ?? {}), deterministic: (uxCounts.assets?.deterministic ?? 0) + 1 };
      return { written: ["android/launch_background.xml", "ios/LaunchScreen.storyboard"], warnings: [] };
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
    case "preflight_fix": {
      const id = args.projectId as string;
      const item = args.item as string | null;
      const fixed: string[] = [];
      const m = metaOf(id);
      const e = mockEvaluate(id);
      for (const it of e.items) {
        if (item && it.key !== item) continue;
        for (const f of it.fixable) {
          if (f === "title") m.listing.title = projects.find((p) => p.id === id)?.name ?? "My app";
          else if (f === "short_description") m.listing.shortDescription = "Keep score of your games with friends";
          else if (f === "full_description") m.listing.fullDescription = "Keep score of your games with friends and see who wins the season.";
          else if (f === "changelog") fs.changelog = true;
          fixed.push(f);
        }
      }
      uxCounts.publish = { ...(uxCounts.publish ?? {}), deterministic: (uxCounts.publish?.deterministic ?? 0) + fixed.length };
      return fixed;
    }
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
        keystore: vaultHas(args.projectId as string, "google_play", "keystore"),
        keystorePath: null,
        gradleConfigured: fs.gradle,
        playKey: vaultHas(args.projectId as string, "google_play", "service_account"),
        fastlane: true,
      };
    case "signing_create_keystore":
      await sleep(500);
      vaultSetKeystore(args.projectId as string);
      return "vault";
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
        byClass: {
          trivial: { runs: 7, ok: 7, clean: 7, escalated: 0, costUsd: 0.31, tokens: 410000 },
          normal: { runs: 3, ok: 3, clean: 2, escalated: 1, costUsd: 1.1, tokens: 990000 },
          complex: { runs: 2, ok: 1, clean: 0, escalated: 0, costUsd: 2.01, tokens: 1700000 },
        },
        byModel: {
          haiku: { runs: 7, ok: 7, clean: 7, escalated: 0, costUsd: 0.31, tokens: 410000 },
          sonnet: { runs: 3, ok: 3, clean: 2, escalated: 1, costUsd: 1.1, tokens: 990000 },
          opus: { runs: 2, ok: 1, clean: 0, escalated: 0, costUsd: 2.01, tokens: 1700000 },
        },
      };
    case "project_get":
    case "project_open": {
      const p = projects.find((x) => x.id === args.id);
      if (!p) throw new AppError("not_found", `project ${String(args.id)} not found`);
      if (cmd === "project_open") {
        emit("profile-status", { projectId: p.id, state: "working", reason: null });
        setTimeout(() => emit("profile-status", { projectId: p.id, state: "ready", reason: null }), 1500);
        // De preview start vanzelf bij het openen van een project (net als in de echte app).
        if (scenario !== "nopreview" && !(preview.projectId === p.id && preview.status !== "stopped")) {
          void mockPreviewStart(p.id, "web");
        }
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

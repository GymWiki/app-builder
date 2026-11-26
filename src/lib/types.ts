export type ProjectStatus = "draft" | "testing" | "ready" | "published";
export type TemplateId = "blank" | "scoreboard" | "checklist" | "tracker";

export interface Project {
  id: string;
  name: string;
  slug: string;
  path: string;
  template: TemplateId;
  description: string;
  status: ProjectStatus;
  version: string;
  buildNumber: number;
  bundleId: string;
  createdAt: string;
  updatedAt: string;
  lastOpenedAt: string | null;
  checklistDone: number;
  checklistTotal: number;
}

export type ToolState = "ok" | "missing" | "outdated" | "warning";
export type ToolId = "git" | "flutter" | "android" | "claude";

export interface ToolStatus {
  id: ToolId;
  state: ToolState;
  version: string | null;
  minVersion: string | null;
  path: string | null;
  detail: string | null;
  /** no_sdk | licenses | cmdline_tools | needs_flutter | other */
  issue: string | null;
}

export interface FlutterCheck {
  flutter: ToolStatus;
  android: ToolStatus;
}

export interface InstallPlan {
  program: string;
  args: string[];
  display: string;
}

export interface AppInfo {
  version: string;
  os: "windows" | "macos" | "linux" | string;
  arch: string;
  projectsDir: string;
  logDir: string | null;
  minVersions: { git: string; flutter: string; claude: string };
}

export type Settings = Partial<
  Record<"projects_dir" | "setup_completed" | "org" | "dev_mode" | "model_override", string>
>;

export interface Screen {
  class: string;
  file: string;
  line: number;
}

export type ToolKind = "read" | "edit" | "command" | "network" | "studio" | "other";

/** Spiegel van `ChatEvent` in src-tauri/src/engine/mod.rs. */
export type ChatEvent =
  | { type: "user"; text: string; context: string | null }
  | { type: "init"; session_id: string; model: string | null; mcp?: [string, string][] }
  | { type: "text_delta"; message_id: string; text: string }
  | { type: "text"; message_id: string; text: string }
  | {
      type: "tool_use";
      id: string;
      name: string;
      kind: ToolKind;
      summary: string;
      path: string | null;
      detail: string | null;
    }
  | { type: "tool_result"; tool_use_id: string; ok: boolean; text: string }
  | { type: "snapshot"; commit: string; label: string }
  | { type: "status"; code: string; detail: string | null }
  | {
      type: "done";
      ok: boolean;
      text: string;
      duration_ms: number | null;
      code: string | null;
      cost_usd?: number | null;
      turns?: number | null;
    }
  | { type: "error"; code: string; message: string };

export interface PermissionRequest {
  id: string;
  projectId: string;
  tool: string;
  kind: ToolKind;
  summary: string;
  detail: string | null;
  key: string;
}

export type PermissionDecision = "allow_once" | "allow_always" | "deny";

export interface PreviewProblem {
  file: string | null;
  line: number | null;
  message: string;
  excerpt: string;
}

export type PreviewStatus = "stopped" | "starting" | "running" | "reloading" | "error";

export interface PreviewState {
  projectId: string | null;
  status: PreviewStatus;
  /** "web" of een apparaat-id */
  target: string;
  url: string | null;
  problems: PreviewProblem[];
  reloads: number;
  /** `inspect` = het scherm komt als beeldstroom uit de Chrome van `flutter run` (widget inspector beschikbaar); `iframe` = oude web-server. */
  mode?: "iframe" | "inspect";
}

export interface SelectionLink {
  widget: string;
  file: string;
  line: number;
  column: number;
}
export interface SelectionTarget {
  widget: string;
  text: string | null;
  file: string;
  startLine: number;
  endLine: number;
  chain: SelectionLink[];
  viaShared: boolean;
}
/** Wat er in het getekende vak staat, volgens de widget inspector. */
export interface Selection {
  screen: string | null;
  screenFile: string | null;
  targets: SelectionTarget[];
  bbox: [number, number, number, number] | null;
}

export interface Device {
  id: string;
  name: string;
  platform: string;
  emulator: boolean;
  wireless: boolean;
}

export interface ScreenNode {
  class: string;
  file: string;
  line: number;
  children: string[];
  /** go_router-pad, als het scherm er een heeft (de preview springt er dan naartoe). */
  route?: string | null;
}
export interface ScreenGraph {
  home: string | null;
  nodes: ScreenNode[];
  unlinked: string[];
}

export interface PermissionState {
  key: string;
  enabled: boolean;
  ios_reason: string | null;
}
export interface PlatformSettings {
  appName: string | null;
  bundleId: string | null;
  minAndroid: number | null;
  minIos: string | null;
  androidApplicationId: string | null;
  permissions: PermissionState[];
  hasAndroid: boolean;
  hasIos: boolean;
}
export interface Applied {
  applied: string[];
  skipped: { target: string; reason: string }[];
}
export interface IconReport {
  written: string[];
  warnings: string[];
}
export interface Snapshot {
  id: string;
  commit: string;
  label: string;
  kind: "auto" | "manual" | "publish";
  version: string;
  createdAt: string;
}

export interface Questionnaire {
  hasAccounts: boolean | null;
  collectsPersonalData: boolean | null;
  showsAds: boolean | null;
  inAppPurchases: boolean | null;
  userGeneratedContent: boolean | null;
  usesLocation: boolean | null;
  playConsoleDone: boolean;
}
export interface Meta {
  listing: { title: string; shortDescription: string; fullDescription: string; category: string; contactEmail: string };
  questionnaire: Questionnaire;
  legal: { privacyUrl: string; termsUrl: string; hostedChecked: boolean; accountDeletion: boolean };
  data: { provider: string; supabaseUrl: string; supabaseProjectRef: string; authMethods: string[] };
  publishing: {
    track: string;
    playAccountReady: boolean;
    finalConfirmed: boolean;
    manualChecks: Record<string, boolean>;
    deviceTested: boolean;
  };
  lastPublishedCommit: string | null;
  lastPublishedVersion: string | null;
}

export interface ChecklistItemState {
  key: string;
  blocking: boolean;
  status: "done" | "todo" | "failed";
  missing: string[];
  next: string | null;
}
export interface Evaluation {
  items: ChecklistItemState[];
  canPublish: boolean;
  done: number;
  total: number;
}

export interface TestIssue {
  kind: string;
  severity: "error" | "warning";
  message: string;
  detail: string;
}
export interface TestScreen {
  name: string;
  depth: number;
  taps: number;
  issues: TestIssue[];
  screenshot: string | null;
}
export interface TestReport {
  startedOk: boolean;
  tapped: number;
  durationMs: number;
  screens: TestScreen[];
  errors: TestIssue[];
}
export interface ReportView {
  errors: number;
  warnings: number;
  report: TestReport | null;
  passed: boolean;
  stale: boolean;
  nativeFeatures: string[];
  notTested: string[];
}
export interface SigningStatus {
  keytool: boolean;
  keystore: boolean;
  keystorePath: string | null;
  gradleConfigured: boolean;
  playKey: boolean;
  fastlane: boolean;
}

export interface ProfileStatus {
  projectId: string;
  state: "working" | "ready" | "limited";
  /** Alleen voor de ontwikkelaarsmodus. */
  reason: string | null;
}

export interface ProfileInfo {
  version: number;
  current: number;
  plugin: boolean;
  dart: string | null;
  dartMcpOk: boolean;
  structure: "feature-first" | "flat";
}

/** Spiegel van `RunsSummary` in src-tauri/src/db.rs (alleen voor het verborgen ontwikkelaarspaneel). */
export interface RunsSummary {
  runs: number;
  byLevel: Record<string, number>;
  expanded: number;
  cleanFirstTime: number;
  repaired: number;
  rolledBack: number;
  aiQuestions: number;
  avgDurationMs: number;
  totalCostUsd: number;
  avgFilesChanged: number;
  deniedEdits: number;
  byClass: Record<string, GroupStats>;
  byModel: Record<string, GroupStats>;
}

/** Waar een opdracht voor dient: hulptaken gebruiken altijd het lichtste model, release-werk heeft een ondergrens. */
export type Purpose = "helper" | "release";

export interface GroupStats {
  runs: number;
  ok: number;
  clean: number;
  escalated: number;
  costUsd: number;
  tokens: number;
}

export interface VersionInfo {
  version: string;
  build: number;
  lastPublished: { version: string; build: number | null } | null;
  nextVersion: string;
  nextBuild: number;
  willChange: boolean;
}

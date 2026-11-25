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

export type Settings = Partial<Record<"projects_dir" | "setup_completed" | "org", string>>;

export interface Screen {
  class: string;
  file: string;
  line: number;
}

export type ToolKind = "read" | "edit" | "command" | "network" | "studio" | "other";

/** Spiegel van `ChatEvent` in src-tauri/src/engine/mod.rs. */
export type ChatEvent =
  | { type: "user"; text: string; context: string | null }
  | { type: "init"; session_id: string; model: string | null }
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
  | { type: "done"; ok: boolean; text: string; duration_ms: number | null; code: string | null }
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
}

export interface Device {
  id: string;
  name: string;
  platform: string;
  emulator: boolean;
  wireless: boolean;
}

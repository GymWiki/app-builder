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

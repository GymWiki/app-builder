import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toAppError, AppError } from "./errors";
import type {
  EnginesView,
  ProductKind,
  ProductPeriod,
  Purchases,
  PurchasesView,
  StoreStatus,
  Selection,
  VersionInfo,
  AppInfo,
  Applied,
  ChatEvent,
  Device,
  Evaluation,
  IconReport,
  Meta,
  PlatformSettings,
  ReportView,
  ScreenGraph,
  SigningStatus,
  Snapshot,
  TestReport,
  FlutterCheck,
  InstallPlan,
  PermissionDecision,
  PreviewState,
  ProfileInfo,
  RunsSummary,
  Project,
  Screen,
  Settings,
  TemplateId,
  ToolStatus,
} from "./types";
import type { Purpose } from "./types";
import type { IntegrationOverview, IntegrationStatus, Leak, TestOutcome } from "./integrations";

export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Alle backend-aanroepen lopen hierdoor: getypeerde fouten, en in dev-in-de-browser een mock. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri()) {
    if (import.meta.env.DEV || import.meta.env.VITE_DEMO) {
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
    if (import.meta.env.DEV || import.meta.env.VITE_DEMO) {
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

  chatSend: (projectId: string, text: string, context: string | null, language: string, purpose?: Purpose) =>
    call<void>("chat_send", { projectId, text, context, language, purpose: purpose ?? null }),
  chatCancel: (projectId: string) => call<void>("chat_cancel", { projectId }),
  chatRunning: (projectId: string) => call<boolean>("chat_running", { projectId }),
  chatHistory: (projectId: string) => call<ChatEvent[]>("chat_history", { projectId }),
  chatNewSession: (projectId: string) => call<void>("chat_new_session", { projectId }),
  permissionRespond: (id: string, decision: PermissionDecision) =>
    call<boolean>("permission_respond", { id, decision }),
  projectScreens: (id: string) => call<Screen[]>("project_screens", { id }),
  ptyOpenClaude: (projectId: string) => call<string>("pty_open_claude", { projectId }),

  previewStart: (projectId: string, target: string) => call<void>("preview_start", { projectId, target }),
  previewStop: () => call<void>("preview_stop"),
  previewState: () => call<PreviewState>("preview_state"),
  enginesList: () => call<EnginesView>("engines_list"),
  enginesSetOrder: (order: string[]) => call<EnginesView>("engines_set_order", { order }),
  engineClearLimit: (id: string) => call<EnginesView>("engine_clear_limit", { id }),
  engineOpenLogin: (id: string) => call<string>("engine_open_login", { id }),
  purchasesGet: (projectId: string) => call<PurchasesView>("purchases_get", { projectId }),
  purchasesSave: (projectId: string, purchases: Purchases) =>
    call<PurchasesView>("purchases_save", { projectId, purchases }),
  purchasesApproveGating: (projectId: string) => call<PurchasesView>("purchases_approve_gating", { projectId }),
  purchasesConfirm: (projectId: string, key: string, value: boolean) =>
    call<PurchasesView>("purchases_confirm", { projectId, key, value }),
  purchasesSetStatus: (projectId: string, product: string, store: "apple" | "google", status: StoreStatus) =>
    call<PurchasesView>("purchases_set_status", { projectId, product, store, status }),
  purchasesGatingPrompt: (projectId: string) => call<string>("purchases_gating_prompt", { projectId }),
  purchasesSetGatingFromAnswer: (projectId: string, answer: string) =>
    call<PurchasesView>("purchases_set_gating_from_answer", { projectId, answer }),
  purchasesApply: (projectId: string, language: string, confirmed: boolean) =>
    call<void>("purchases_apply", { projectId, language, confirmed }),
  purchasesSuggestId: (name: string, kind: ProductKind, period: ProductPeriod | null) =>
    call<string>("purchases_suggest_id", { name, kind, period }),
  previewSelect: (projectId: string, bbox: [number, number, number, number]) =>
    call<{ selection: Selection; prompt: string }>("preview_select", { projectId, bbox }),
  previewNavigate: (route: string) => call<void>("preview_navigate", { route }),
  previewShotSave: (projectId: string, screen: string, data: string) =>
    call<void>("preview_shot_save", { projectId, screen, data }),
  previewShotGet: (projectId: string, screen: string) => call<string | null>("preview_shot_get", { projectId, screen }),
  previewShotsList: (projectId: string) => call<string[]>("preview_shots_list", { projectId }),
  previewViewport: (width: number, height: number) => call<void>("preview_viewport", { width, height }),
  chatSetSelection: (projectId: string, ranges: [string, number, number][]) =>
    call<void>("chat_set_selection", { projectId, ranges }),
  previewLog: () => call<string[]>("preview_log"),
  previewReload: () => call<void>("preview_reload"),
  previewDevices: () => call<Device[]>("preview_devices"),

  themeGet: (projectId: string) => call<{ managed: boolean; seedColor: string | null }>("theme_get", { projectId }),
  themeSet: (projectId: string, seedColor: string) =>
    call<{ ok: boolean; wired: boolean; next: string }>("theme_set", { projectId, seedColor }),
  screensGraph: (projectId: string) => call<ScreenGraph>("screens_graph", { projectId }),

  platformGet: (projectId: string) => call<PlatformSettings>("platform_get", { projectId }),
  platformSetName: (projectId: string, name: string) => call<Applied>("platform_set_name", { projectId, name }),
  platformSetBundle: (projectId: string, bundleId: string) =>
    call<Applied>("platform_set_bundle", { projectId, bundleId }),
  platformSetMinOs: (projectId: string, android: number | null, ios: string | null) =>
    call<Applied>("platform_set_min_os", { projectId, android, ios }),
  platformSetPermission: (projectId: string, key: string, enabled: boolean, iosReason: string) =>
    call<Applied>("platform_set_permission", { projectId, key, enabled, iosReason }),
  versionSet: (projectId: string, version: string, build: number) =>
    call<VersionInfo>("version_set", { projectId, version, build }),
  versionInfo: (projectId: string) => call<VersionInfo>("version_info", { projectId }),
  assetsGenerateIcon: (projectId: string, sourcePath: string) =>
    call<IconReport>("assets_generate_icon", { projectId, sourcePath }),

  snapshotsList: (projectId: string) => call<Snapshot[]>("snapshots_list", { projectId }),
  snapshotCreate: (projectId: string, label: string) =>
    call<{ created: boolean }>("snapshot_create", { projectId, label }),
  snapshotRestore: (projectId: string, commit: string) =>
    call<{ restored: boolean }>("snapshot_restore", { projectId, commit }),
  changelogDraft: (projectId: string) => call<string[]>("changelog_draft", { projectId }),

  metaGet: (projectId: string) => call<Meta>("meta_get", { projectId }),
  metaSave: (projectId: string, meta: Meta) => call<void>("meta_save", { projectId, meta }),
  secretSet: (projectId: string, name: string, value: string) => call<void>("secret_set", { projectId, name, value }),
  secretHas: (projectId: string, name: string) => call<boolean>("secret_has", { projectId, name }),
  secretDelete: (projectId: string, name: string) => call<void>("secret_delete", { projectId, name }),
  dataSetupPrompt: (projectId: string) => call<string>("data_setup_prompt", { projectId }),
  checklistEvaluate: (projectId: string) => call<Evaluation>("checklist_evaluate", { projectId }),

  storeScreenshotsList: (projectId: string) => call<string[]>("store_screenshots_list", { projectId }),
  storeScreenshotCapture: (projectId: string) => call<string>("store_screenshot_capture", { projectId }),
  storeScreenshotImport: (projectId: string, paths: string[]) =>
    call<string[]>("store_screenshot_import", { projectId, paths }),
  storeScreenshotDelete: (projectId: string, name: string) =>
    call<void>("store_screenshot_delete", { projectId, name }),
  fileDataUrl: (projectId: string, rel: string) => call<string>("file_data_url", { projectId, rel }),

  testrunStart: (projectId: string) => call<TestReport>("testrun_start", { projectId }),
  testrunReport: (projectId: string) => call<ReportView>("testrun_report", { projectId }),
  signingStatus: (projectId: string) => call<SigningStatus>("signing_status", { projectId }),
  signingCreateKeystore: (projectId: string, owner: string, organisation: string, country: string) =>
    call<string>("signing_create_keystore", { projectId, owner, organisation, country }),
  signingPrompt: () => call<string>("signing_prompt"),
  publishAndroid: (projectId: string, track: string, confirmed: boolean) =>
    call<{ version: string; track: string }>("publish_android", { projectId, track, confirmed }),
  publishIos: () => call<void>("publish_ios"),

  profileInfo: (projectId: string) => call<ProfileInfo>("profile_info", { projectId }),
  profileApply: (projectId: string) => call<void>("profile_apply", { projectId }),
  integrationList: (projectId: string) => call<IntegrationOverview>("integration_list", { projectId }),
  integrationSetEnabled: (projectId: string, id: string, enabled: boolean, deleteSecrets: boolean) =>
    call<IntegrationStatus[]>("integration_set_enabled", { projectId, id, enabled, deleteSecrets }),
  secretFieldSetText: (projectId: string, integration: string, field: string, value: string) =>
    call<void>("secret_field_set_text", { projectId, integration, field, value }),
  secretFieldSetFile: (projectId: string, integration: string, field: string, fileName: string, dataB64: string) =>
    call<{ derived: Record<string, string> }>("secret_field_set_file", {
      projectId,
      integration,
      field,
      fileName,
      dataB64,
    }),
  secretFieldDelete: (projectId: string, integration: string, field: string) =>
    call<void>("secret_field_delete", { projectId, integration, field }),
  secretFieldReveal: (projectId: string, integration: string, field: string) =>
    call<string>("secret_field_reveal", { projectId, integration, field }),
  secretTest: (projectId: string, integration: string) => call<TestOutcome>("secret_test", { projectId, integration }),
  vaultExport: (projectId: string, passphrase: string) => call<string>("vault_export", { projectId, passphrase }),
  vaultImport: (projectId: string, passphrase: string, dataB64: string) =>
    call<number>("vault_import", { projectId, passphrase, dataB64 }),
  vaultExportFile: (projectId: string, passphrase: string, path: string) =>
    call<void>("vault_export_file", { projectId, passphrase, path }),
  vaultImportFile: (projectId: string, passphrase: string, path: string) =>
    call<number>("vault_import_file", { projectId, passphrase, path }),
  leaksScan: (projectId: string) => call<Leak[]>("leaks_scan", { projectId }),
  leakMoveToVault: (projectId: string, file: string) =>
    call<[string, string]>("leak_move_to_vault", { projectId, file }),
  leakFixPrompt: (file: string, line: number, kind: string) => call<string>("leak_fix_prompt", { file, line, kind }),
  runMetrics: (projectId: string) => call<RunsSummary>("run_metrics", { projectId }),

  projectsList: (query?: string) => call<Project[]>("projects_list", { query: query || null }),
  projectGet: (id: string) => call<Project>("project_get", { id }),
  projectOpen: (id: string) => call<Project>("project_open", { id }),
  projectRename: (id: string, name: string) => call<Project>("project_rename", { id, name }),
  projectCreate: (args: { name: string; template: TemplateId; description: string }) =>
    call<Project>("project_create", { args }),
  projectDelete: (id: string, deleteFiles: boolean) => call<void>("project_delete", { id, deleteFiles }),
};

import { installedApps, pinnedProjects, recentProjects, type Project } from "./data";

export type HealthIssue = { code: string; title: string; detail: string; severity: string; remediation?: string };
export type CachedHealth = { issues: HealthIssue[]; checkedAt: string };
export type ProjectHealthOverview = { name: string; path: string; checkedAt: string | null; issues: HealthIssue[] };
export type CommitFile = { path: string; status: string };
export type EngineVersionOption = { version: string; executable: string };
export type EngineChoice = { appId: string; preferred: string | null; options: EngineVersionOption[] };
export type AppInstallation = { version: string; executable: string; state: string; runnable: boolean };
export type ManagedApp = { id: string; name: string; category: string; launchable: boolean; installations: AppInstallation[] };
export type RegisteredProject = { name: string; path: string; pinned: boolean; tags: string[]; category: string };
export type ActivityRecord = { id: number; kind: string; message: string; createdAt: string };
export type UpdateInfo = { available: boolean; currentVersion: string; version: string; notes?: string | null };
export type PathInfo = { exists: boolean; isDir: boolean; isFile: boolean };
export type LaunchProfile = { id: string; name: string; app_id: string; arguments: string[]; working_directory?: string | null; preferred_version?: string | null; fallback_version?: string | null };
export type LinkedApp = { app_id: string; preferred_version?: string | null; project_file?: string | null; folder?: string | null };
export type ProjectConfig = {
  schema_version: number;
  name: string;
  project_type: string;
  linked_apps: LinkedApp[];
  launch_profiles: LaunchProfile[];
  shortcuts: Array<{ name: string; kind: string; path: string }>;
  version_control?: { provider: string; root: string } | null;
  enabled_health_checks: string[];
  thumbnail?: string | null;
  tags?: string[];
  category?: string | null;
};
export type RecentFile = { path: string; name: string; modified: number };
export type ScanProgress = { completed: number; total: number; current: string; done: boolean };

/** Subscribes to live scan progress; returns an unsubscribe function. */
export async function onScanProgress(handler: (progress: ScanProgress) => void): Promise<() => void> {
  if (!isNativeRuntime()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<ScanProgress>("scan://progress", (event) => handler(event.payload));
}

/** Opens a native folder or file picker; returns the chosen path, or null. */
export async function browsePath(opts: { directory?: boolean; title?: string }): Promise<string | null> {
  if (!isNativeRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({ directory: opts.directory ?? false, multiple: false, title: opts.title });
  return typeof result === "string" ? result : null;
}

export const APP_CATEGORY_LABELS: Record<string, string> = {
  "game-engine": "Game Engines",
  dcc: "DCC & Animation",
  art: "Art & Texturing",
  code: "Code & IDEs",
  "version-control": "Version Control",
  utility: "Utilities",
};

/** Formats a detected version, presenting the unknown 0.0.0 sentinel clearly. */
export function formatVersion(version: string): string {
  return version === "0.0.0" ? "Unknown version" : version;
}
export type GitStatus = { branch?: string; ahead: number; behind: number; changedFiles: Array<{ path: string; status: string }> };
export type GitCommit = { hash: string; shortHash: string; author: string; date: string; subject: string };
export type ToolManifest = {
  id: string;
  name: string;
  description: string;
  sourceUrl: string;
  license: string;
  supportedHosts: string[];
  platforms: string[];
  provenance: string;
  reviewState: "submitted" | "reviewed" | "verified" | "stale" | "withdrawn";
  lastVerifiedAt: string;
  safetyNotes: string;
};

export type DashboardSnapshot = {
  networkEnabled: boolean;
  continueProject: Project | null;
  pinnedProjects: Project[];
  recentProjects: Project[];
  apps: Array<{ id: string; name: string; category: string; executable?: string | null; versions: string[] }>;
  health: HealthSummary[];
};
export type HealthSummary = { code: string; title: string; detail: string; severity: string; project: string };

declare global {
  interface Window { __TAURI_INTERNALS__?: unknown; }
}

const demoSnapshot: DashboardSnapshot = {
  networkEnabled: false,
  continueProject: { name: "Voidline", path: "D:/Dev/Projects/Voidline", engine: "Unity", version: "2022.3.18f1", branch: "main", lastOpened: "Today, 10:42 AM" },
  pinnedProjects,
  recentProjects,
  apps: installedApps.map((app) => ({ id: app.name.toLowerCase().replaceAll(" ", "-"), ...app })),
  health: [],
};

const emptySnapshot: DashboardSnapshot = { networkEnabled: false, continueProject: null, pinnedProjects: [], recentProjects: [], apps: [], health: [] };

export function isNativeRuntime() { return Boolean(window.__TAURI_INTERNALS__); }
export function isDemoMode() { return new URLSearchParams(window.location.search).get("demo") === "true"; }

export async function invokeDesktop<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isNativeRuntime()) throw new Error("This operation requires the PipelineOS desktop runtime.");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export async function loadDashboard(): Promise<DashboardSnapshot> {
  if (isNativeRuntime()) return invokeDesktop<DashboardSnapshot>("dashboard_snapshot");
  return isDemoMode() ? demoSnapshot : emptySnapshot;
}

export const desktopApi = {
  listProjects: () => invokeDesktop<RegisteredProject[]>("list_projects"),
  importProject: (root: string, name?: string) => invokeDesktop("import_project", { root, name: name || null }),
  removeProject: (root: string) => invokeDesktop<void>("remove_project", { root }),
  setProjectTags: (root: string, tags: string[]) => invokeDesktop<void>("set_project_tags", { root, tags }),
  setProjectCategory: (root: string, category: string | null) => invokeDesktop<void>("set_project_category", { root, category }),
  readWorkspace: (root: string) => invokeDesktop<string | null>("read_project_workspace", { root }),
  saveWorkspace: (root: string, contents: string) => invokeDesktop<void>("save_project_workspace", { root, contents }),
  recentActivity: (limit: number) => invokeDesktop<ActivityRecord[]>("recent_activity", { limit }),
  projectHealth: (root: string) => invokeDesktop<HealthIssue[]>("project_health", { root }),
  cachedHealth: (root: string) => invokeDesktop<CachedHealth | null>("cached_health", { root }),
  healthOverview: () => invokeDesktop<ProjectHealthOverview[]>("health_overview"),
  refreshHealth: (full: boolean) => invokeDesktop<HealthSummary[]>("refresh_health", { full }),
  recordProjectOpened: (root: string) => invokeDesktop<void>("record_project_opened", { root }),
  renameProject: (root: string, name: string) => invokeDesktop<void>("rename_project", { root, name }),
  setProjectThumbnail: (root: string, source: string) => invokeDesktop<string>("set_project_thumbnail", { root, source }),
  clearProjectThumbnail: (root: string) => invokeDesktop<void>("clear_project_thumbnail", { root }),
  gitCommitFiles: (root: string, hash: string) => invokeDesktop<CommitFile[]>("git_commit_files", { root, hash }),
  gitStatus: (root: string) => invokeDesktop<GitStatus>("git_status", { root }),
  listApps: () => invokeDesktop<ManagedApp[]>("list_apps"),
  scanApps: (roots: string[]) => invokeDesktop<unknown[]>("scan_apps", { roots }),
  cancelScan: () => invokeDesktop<void>("cancel_scan"),
  setManualOverride: (appId: string, version: string, executable: string) => invokeDesktop<void>("set_manual_override", { appId, version, executable }),
  launchApp: (appId: string, executable: string) => invokeDesktop<void>("launch_app", { appId, executable }),
  appIcon: (executable: string) => invokeDesktop<string | null>("app_icon", { executable }),
  checkForUpdate: () => invokeDesktop<UpdateInfo>("check_for_update"),
  installUpdate: () => invokeDesktop<void>("install_update"),
  pathInfo: (path: string) => invokeDesktop<PathInfo>("path_info", { path }),
  projectConfig: (root: string) => invokeDesktop<ProjectConfig>("project_config", { root }),
  recentFiles: (root: string, limit: number) => invokeDesktop<RecentFile[]>("recent_files", { root, limit }),
  openPath: (path: string) => invokeDesktop<void>("open_path", { path }),
  openUrl: (url: string) => invokeDesktop<void>("open_url", { url }),
  readImage: (path: string) => invokeDesktop<string | null>("read_image", { path }),
  readToolsFromDir: (path: string) => invokeDesktop<ToolManifest[]>("read_tools_from_dir", { path }),
  launchExecutable: (executable: string) => invokeDesktop<void>("launch_executable", { executable }),
  listTools: () => invokeDesktop<ToolManifest[]>("list_tools"),
  pinProject: (root: string, pinned: boolean) => invokeDesktop<void>("set_project_pinned", { root, pinned }),
  launchProjectProfile: (root: string, profileId: string) => invokeDesktop<{ processId?: number; executable: string }>("launch_project_profile", { root, profileId }),
  openInEngine: (root: string, appId?: string) => invokeDesktop<{ processId?: number; executable: string }>("open_in_engine", { root, appId: appId ?? null }),
  engineOptions: (root: string, appId?: string) => invokeDesktop<EngineChoice | null>("engine_options", { root, appId: appId ?? null }),
  setProjectEngineVersion: (root: string, appId: string, version: string) => invokeDesktop<void>("set_project_engine_version", { root, appId, version }),
  gitSync: (root: string, confirmed: boolean) => invokeDesktop("git_sync", { root, confirmed }),
  gitCommit: (root: string, message: string, confirmed: boolean) => invokeDesktop("git_commit", { root, message, confirmed }),
  gitPush: (root: string, confirmed: boolean) => invokeDesktop("git_push", { root, confirmed }),
  gitSwitch: (root: string, branch: string, confirmed: boolean) => invokeDesktop("git_switch", { root, branch, confirmed }),
  gitBranches: (root: string) => invokeDesktop<string[]>("git_branches", { root }),
  gitCreateBranch: (root: string, branch: string, confirmed: boolean) => invokeDesktop("git_create_branch", { root, branch, confirmed }),
  gitLog: (root: string, limit: number) => invokeDesktop<GitCommit[]>("git_log", { root, limit }),
  gitDiff: (root: string, path: string) => invokeDesktop<string>("git_diff", { root, path }),
  gitCommitPaths: (root: string, message: string, paths: string[], confirmed: boolean) => invokeDesktop("git_commit_paths", { root, message, paths, confirmed }),
  gitDiscard: (root: string, path: string, confirmed: boolean) => invokeDesktop("git_discard", { root, path, confirmed }),
  appVersion: () => invokeDesktop<string>("app_version"),
  autostartEnabled: () => invokeDesktop<boolean>("plugin:autostart|is_enabled"),
  setAutostart: (enabled: boolean) => invokeDesktop<void>(enabled ? "plugin:autostart|enable" : "plugin:autostart|disable"),
};

/// Opens an external http(s) link: via the OS opener in the desktop app (the
/// WebView's window.open is unreliable), or a new browser tab on the web.
export async function openExternal(url: string): Promise<void> {
  if (isNativeRuntime()) {
    try { await desktopApi.openUrl(url); return; } catch { /* fall through to window.open */ }
  }
  window.open(url, "_blank", "noopener");
}

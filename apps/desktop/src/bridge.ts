import { installedApps, pinnedProjects, recentProjects, type Project } from "./data";

export type HealthIssue = { code: string; title: string; detail: string; severity: string; remediation?: string };
export type AppInstallation = { version: string; executable: string; state: string; runnable: boolean };
export type ManagedApp = { id: string; name: string; category: string; launchable: boolean; installations: AppInstallation[] };
export type RegisteredProject = { name: string; path: string; pinned: boolean };
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
export type GitStatus = { branch?: string; changedFiles: Array<{ path: string; status: string }> };
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
  health: HealthIssue[];
};

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
  if (!isNativeRuntime()) throw new Error("This operation requires the Vantadeck desktop runtime.");
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
  projectHealth: (root: string) => invokeDesktop<HealthIssue[]>("project_health", { root }),
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
  launchExecutable: (executable: string) => invokeDesktop<void>("launch_executable", { executable }),
  listTools: () => invokeDesktop<ToolManifest[]>("list_tools"),
  pinProject: (root: string, pinned: boolean) => invokeDesktop<void>("set_project_pinned", { root, pinned }),
  launchProjectProfile: (root: string, profileId: string) => invokeDesktop<{ processId?: number; executable: string }>("launch_project_profile", { root, profileId }),
  gitSync: (root: string, confirmed: boolean) => invokeDesktop("git_sync", { root, confirmed }),
  gitCommit: (root: string, message: string, confirmed: boolean) => invokeDesktop("git_commit", { root, message, confirmed }),
  gitPush: (root: string, confirmed: boolean) => invokeDesktop("git_push", { root, confirmed }),
  gitSwitch: (root: string, branch: string, confirmed: boolean) => invokeDesktop("git_switch", { root, branch, confirmed }),
};

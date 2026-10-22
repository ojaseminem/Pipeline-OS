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

/** Subscribes to live project-scan progress; returns an unsubscribe function. */
export async function onProjectScanProgress(handler: (progress: ScanProgress) => void): Promise<() => void> {
  if (!isNativeRuntime()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<ScanProgress>("scan-projects://progress", (event) => handler(event.payload));
}

/** Opens a native folder or file picker; returns the chosen path, or null. */
export async function browsePath(opts: { directory?: boolean; title?: string }): Promise<string | null> {
  if (!isNativeRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({ directory: opts.directory ?? false, multiple: false, title: opts.title });
  return typeof result === "string" ? result : null;
}

/** Saves text to a file the user picks (native), or downloads it (browser). */
export async function saveTextFile(opts: { defaultName: string; contents: string; filterName: string; extensions: string[] }): Promise<string | null> {
  if (isNativeRuntime()) {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({ defaultPath: opts.defaultName, filters: [{ name: opts.filterName, extensions: opts.extensions }] });
    if (!path) return null;
    await invokeDesktop("save_text_file", { path, contents: opts.contents });
    return path;
  }
  const url = URL.createObjectURL(new Blob([opts.contents], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = opts.defaultName;
  link.click();
  URL.revokeObjectURL(url);
  return opts.defaultName;
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
export type GitStatus = { branch?: string; ahead: number; behind: number; hasUpstream: boolean; lastFetchedAt?: string | null; changedFiles: Array<{ path: string; status: string }> };
export type GitCommit = { hash: string; shortHash: string; author: string; date: string; subject: string };
export type MergeOutcome = { status: "merged"; message: string } | { status: "conflicts"; files: string[] };
export type MergeStatus = { inProgress: boolean; conflictedFiles: string[] };
export type BranchComparison = { ahead: number; behind: number };
export type ConflictResolution = "ours" | "theirs" | "resolved";
/** A branch the UI can switch to or merge. `remote` is set (e.g. "origin")
 *  for a remote-tracking branch with no local copy yet; unset for local. */
export type BranchInfo = { name: string; remote?: string | null };
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

// One shared import of the Tauri core module: every IPC call awaits the same
// promise instead of starting its own dynamic import.
let coreModule: Promise<typeof import("@tauri-apps/api/core")> | null = null;

export async function invokeDesktop<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isNativeRuntime()) throw new Error("This operation requires the PipelineOS desktop runtime.");
  coreModule ??= import("@tauri-apps/api/core");
  const { invoke } = await coreModule;
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
  addLinkedApp: (root: string, appId: string) => invokeDesktop<void>("add_linked_app", { root, appId }),
  removeLinkedApp: (root: string, appId: string) => invokeDesktop<void>("remove_linked_app", { root, appId }),
  detectProjectApps: (root: string) => invokeDesktop<string[]>("detect_project_apps", { root }),
  readWorkspace: (root: string) => invokeDesktop<string | null>("read_project_workspace", { root }),
  saveWorkspace: (root: string, contents: string) => invokeDesktop<void>("save_project_workspace", { root, contents }),
  readLocalConfig: (root: string) => invokeDesktop<string | null>("read_project_local", { root }),
  saveLocalConfig: (root: string, contents: string) => invokeDesktop<void>("save_project_local", { root, contents }),
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
  gitAvailable: () => invokeDesktop<boolean>("git_available"),
  gitInitRepo: (root: string, remote: string | null, confirmed: boolean) => invokeDesktop<{ stdout: string; stderr: string }>("git_init_repo", { root, remote, confirmed }),
  installGit: () => invokeDesktop<string>("install_git"),
  installGitLfs: () => invokeDesktop<string>("install_git_lfs"),
  gitLfsTrackLargeFiles: (root: string, confirmed: boolean) => invokeDesktop<{ stdout: string; stderr: string }>("git_lfs_track_large_files", { root, confirmed }),
  listApps: () => invokeDesktop<ManagedApp[]>("list_apps"),
  scanApps: (roots: string[]) => invokeDesktop<unknown[]>("scan_apps", { roots }),
  cancelScan: () => invokeDesktop<void>("cancel_scan"),
  listDrives: () => invokeDesktop<string[]>("list_drives"),
  scanProjects: (roots: string[]) => invokeDesktop<RegisteredProject[]>("scan_projects", { roots }),
  cancelProjectScan: () => invokeDesktop<void>("cancel_project_scan"),
  setManualOverride: (appId: string, version: string, executable: string) => invokeDesktop<void>("set_manual_override", { appId, version, executable }),
  launchApp: (appId: string, executable: string) => invokeDesktop<void>("launch_app", { appId, executable }),
  appIcon: (executable: string) => invokeDesktop<string | null>("app_icon", { executable }),
  checkForUpdate: () => invokeDesktop<UpdateInfo>("check_for_update"),
  installUpdate: () => invokeDesktop<void>("install_update"),
  pathInfo: (path: string) => invokeDesktop<PathInfo>("path_info", { path }),
  projectConfig: (root: string) => invokeDesktop<ProjectConfig>("project_config", { root }),
  repairProjectConfig: (root: string, confirmed: boolean) => invokeDesktop<ProjectConfig>("repair_project_config", { root, confirmed }),
  recentFiles: (root: string, limit: number) => invokeDesktop<RecentFile[]>("recent_files", { root, limit }),
  appProjectFiles: (root: string, appId: string) => invokeDesktop<RecentFile[]>("app_project_files", { root, appId }),
  launchAppFile: (appId: string, executable: string, file: string, root: string) => invokeDesktop<{ processId?: number; executable: string }>("launch_app_file", { appId, executable, file, root }),
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
  gitFetch: (root: string) => invokeDesktop("git_fetch", { root }),
  gitPull: (root: string, confirmed: boolean) => invokeDesktop<MergeOutcome>("git_pull", { root, confirmed }),
  gitPublishBranch: (root: string, branch: string, confirmed: boolean) => invokeDesktop("git_publish_branch", { root, branch, confirmed }),
  gitCommit: (root: string, message: string, confirmed: boolean) => invokeDesktop("git_commit", { root, message, confirmed }),
  gitPush: (root: string, confirmed: boolean) => invokeDesktop("git_push", { root, confirmed }),
  gitSwitch: (root: string, branch: string, confirmed: boolean) => invokeDesktop("git_switch", { root, branch, confirmed }),
  gitMerge: (root: string, branch: string, confirmed: boolean) => invokeDesktop<MergeOutcome>("git_merge", { root, branch, confirmed }),
  gitMergeStatus: (root: string) => invokeDesktop<MergeStatus>("git_merge_status", { root }),
  gitCompareBranch: (root: string, branch: string) => invokeDesktop<BranchComparison>("git_compare_branch", { root, branch }),
  gitResolveConflict: (root: string, path: string, resolution: ConflictResolution, confirmed: boolean) => invokeDesktop("git_resolve_conflict", { root, path, resolution, confirmed }),
  gitAbortMerge: (root: string, confirmed: boolean) => invokeDesktop("git_abort_merge", { root, confirmed }),
  gitContinueMerge: (root: string, confirmed: boolean) => invokeDesktop("git_continue_merge", { root, confirmed }),
  gitBranches: (root: string) => invokeDesktop<BranchInfo[]>("git_branches", { root }),
  gitCreateBranch: (root: string, branch: string, confirmed: boolean) => invokeDesktop("git_create_branch", { root, branch, confirmed }),
  gitLog: (root: string, limit: number) => invokeDesktop<GitCommit[]>("git_log", { root, limit }),
  gitDiff: (root: string, path: string) => invokeDesktop<string>("git_diff", { root, path }),
  gitCommitDiff: (root: string, hash: string, path: string) => invokeDesktop<string>("git_commit_diff", { root, hash, path }),
  gitHasUncommittedChanges: (root: string) => invokeDesktop<boolean>("git_has_uncommitted_changes", { root }),
  gitStashPush: (root: string, message: string, confirmed: boolean) => invokeDesktop("git_stash_push", { root, message, confirmed }),
  gitStashPop: (root: string, confirmed: boolean, stashRef?: string) => invokeDesktop("git_stash_pop", { root, stashRef: stashRef ?? null, confirmed }),
  gitStashDrop: (root: string, stashRef: string, confirmed: boolean) => invokeDesktop("git_stash_drop", { root, stashRef, confirmed }),
  gitStashList: (root: string) => invokeDesktop<string[]>("git_stash_list", { root }),
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

// ---------------------------------------------------------------------------
// Studio workflows: pipeline tasks, issue board, AI brief, asset library.
// In the browser demo these run against an in-memory fixture (demo-studio.ts)
// so every screen is explorable without the desktop runtime.

export type TaskKind = "build" | "test" | "render" | "export" | "cook" | "script";
export type TaskDefinition = { id: string; name: string; kind: TaskKind; description?: string | null; program: string; args: string[]; cwd?: string | null; env?: Record<string, string> };
export type TaskPreset = { definition: TaskDefinition; engine: string | null; group: string; recommended: boolean };
export type ProjectTasks = { tasks: TaskDefinition[]; fingerprint: string | null; trusted: boolean };
export type RunStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";
export type TaskRun = {
  id: string; projectPath: string; projectName: string; taskId: string; taskName: string; kind: TaskKind; commandLine: string;
  status: RunStatus; progress: number | null; lastLine: string | null; startedAt: string; finishedAt: string | null; exitCode: number | null; durationMs: number | null;
};
export type TaskLineBatch = { runId: string; lines: Array<{ stream: "stdout" | "stderr"; text: string }> };

export type IssueStatus = "backlog" | "todo" | "in-progress" | "review" | "done";
export type IssuePriority = "none" | "low" | "medium" | "high" | "urgent";
export type IssueKind = "task" | "bug" | "feature" | "art" | "design";
export type Issue = {
  id: string; title: string; status: IssueStatus; priority: IssuePriority; kind: IssueKind; assignee?: string | null; labels?: string[]; files?: string[];
  createdAt: string; updatedAt: string; external?: { system: string; key: string; url?: string | null } | null; body: string;
};
export type NewIssue = { title: string; status?: IssueStatus; priority?: IssuePriority; kind?: IssueKind; assignee?: string | null; labels?: string[]; files?: string[]; body?: string };
export type ProjectIssue = Issue & { projectName: string; projectPath: string };

export type AssetKind = "image" | "source" | "model" | "video" | "audio" | "document";
export type ReviewStatus = "none" | "wip" | "review" | "approved" | "rejected";
export type AssetMeta = { status: ReviewStatus; note?: string | null; owner?: string | null };
export type AssetEntry = { path: string; name: string; folder: string; kind: AssetKind; size: number; modified: number; meta: AssetMeta };
export type AssetIndex = { assets: AssetEntry[]; truncated: boolean };
export type FolderLayout = { id: string; name: string; description: string; folders: string[] };
export type LayoutResult = { created: string[]; existing: string[] };
export type TrackerKind = "github" | "gitea" | "jira" | "linear" | "plane" | "clickup";
export type TrackerConfig = { kind: TrackerKind; baseUrl?: string | null; project: string; query?: string | null };
export type TrackerState = { config: TrackerConfig | null; hasToken: boolean };
export type SyncSummary = { created: number; updated: number; unchanged: number };
export type SelfHostResult = { composePath: string; dockerAvailable: boolean; started: boolean; url: string };
export type CliInfo = { cliPath: string | null; channel: "stable" | "dev"; mcpArgs: string[] };
export type AgentId = "claude" | "codex" | "gemini" | "aider";
export type ProjectEngine = "unity" | "unreal" | "godot" | "pipeline-os";
export type DiscoveredProject = { path: string; name: string; engine: ProjectEngine; engineVersion: string | null; source: string; modified: number };
export type Discovery = { projects: DiscoveredProject[]; scannedAt: string | null };
export type AiClientStatus = { id: string; name: string; installed: boolean; configured: boolean; configPath: string; problem: string | null };
export type AiSetupState = { clients: AiClientStatus[]; agentClis: string[]; canConfigure: boolean };
export type AiConfigureResult = { id: string; ok: boolean; error: string | null };
export type EngineKind = "unreal" | "unity";
export type ActiveTimeSummary = { todayMinutes: number; weekMinutes: number; totalMinutes: number; lastActive: string | null; days: Array<{ date: string; minutes: number }> };
export type ProjectActiveTime = { path: string; weekMinutes: number; totalMinutes: number };
export type EngineActivityKind = "lighting" | "shaders" | "cook" | "compile" | "build" | "import";
export type EngineActivity = {
  id: string; engine: EngineKind; projectPath: string | null; projectName: string | null;
  kind: EngineActivityKind; status: "running" | "succeeded" | "failed"; progress: number | null;
  detail: string; startedAt: string; updatedAt: string; finishedAt: string | null;
};

async function studioCall<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isNativeRuntime()) return invokeDesktop<T>(command, args);
  if (isDemoMode()) {
    const { demoStudio } = await import("./demo-studio");
    return demoStudio(command, args) as Promise<T>;
  }
  throw new Error("This operation requires the PipelineOS desktop runtime.");
}

/** True when studio features have a backend (native app or browser demo). */
export function studioAvailable(): boolean {
  return isNativeRuntime() || isDemoMode();
}

export const studioApi = {
  tasks: (path: string) => studioCall<ProjectTasks>("tasks_list", { path }),
  templates: () => studioCall<TaskDefinition[]>("tasks_templates"),
  presets: (path: string) => studioCall<TaskPreset[]>("tasks_presets", { path }),
  installUnityBuildScript: (path: string) => studioCall<string>("tasks_install_unity_script", { path }),
  saveTasks: (path: string, tasks: TaskDefinition[]) => studioCall<void>("tasks_save", { path, tasks }),
  trustTasks: (path: string, fingerprint: string) => studioCall<void>("tasks_trust", { path, fingerprint }),
  planTask: (path: string, taskId: string) => studioCall<string>("task_plan", { path, taskId }),
  runTask: (path: string, taskId: string) => studioCall<TaskRun>("task_run", { path, taskId }),
  cancelRun: (runId: string) => studioCall<void>("task_cancel", { runId }),
  runs: (path?: string) => studioCall<TaskRun[]>("task_runs", { path: path ?? null }),
  runLog: (runId: string) => studioCall<string[]>("task_run_log", { runId }),
  clearRunHistory: () => studioCall<void>("task_clear_history"),
  issues: (path: string) => studioCall<{ issues: Issue[]; problems: string[] }>("issues_list", { path }),
  createIssue: (path: string, issue: NewIssue) => studioCall<Issue>("issue_create", { path, issue }),
  updateIssue: (path: string, issue: Issue) => studioCall<Issue>("issue_update", { path, issue }),
  deleteIssue: (path: string, id: string) => studioCall<void>("issue_delete", { path, id }),
  issuesEverywhere: () => studioCall<ProjectIssue[]>("issues_everywhere"),
  tracker: (path: string) => studioCall<TrackerState>("tracker_get", { path }),
  saveTracker: (path: string, config: TrackerConfig | null, token?: string) => studioCall<void>("tracker_save", { path, config, token: token ?? null }),
  syncTracker: (path: string) => studioCall<SyncSummary>("tracker_sync", { path }),
  selfHostForgejo: (dir: string, port: number | null, start: boolean) => studioCall<SelfHostResult>("selfhost_forgejo", { dir, port, start }),
  brief: (path: string, write = false) => studioCall<{ markdown: string; writtenTo: string | null }>("project_brief", { path, write }),
  layouts: () => studioCall<FolderLayout[]>("asset_layouts"),
  applyLayout: (path: string, layout: string) => studioCall<LayoutResult>("asset_apply_layout", { path, layout }),
  assets: (path: string, limit?: number) => studioCall<AssetIndex>("assets_index", { path, limit: limit ?? null }),
  setAssetMeta: (path: string, asset: string, meta: AssetMeta) => studioCall<void>("asset_set_meta", { path, asset, meta }),
  openTerminal: (path: string, agent?: AgentId) => studioCall<void>("open_terminal", { path, agent: agent ?? null }),
  cliInfo: () => studioCall<CliInfo>("cli_info"),
  discoverProjects: (refresh: boolean) => studioCall<Discovery>("discover_projects", { refresh }),
  dismissDiscovered: (path: string) => studioCall<void>("dismiss_discovered", { path }),
  aiClients: () => studioCall<AiSetupState>("ai_clients"),
  aiConfigure: (ids: string[]) => studioCall<AiConfigureResult[]>("ai_configure", { ids }),
  aiUnconfigure: (id: string) => studioCall<void>("ai_unconfigure", { id }),
  engineActivity: () => studioCall<EngineActivity[]>("engine_activity"),
  activeTime: (path: string) => studioCall<ActiveTimeSummary>("project_active_time", { path }),
  activeTimeAll: () => studioCall<ProjectActiveTime[]>("active_time_all"),
  clearEngineActivity: () => studioCall<void>("clear_engine_activity"),
};

type Unlisten = () => void;

/** Subscribes to task run updates and batched output lines. */
export async function onTaskEvents(onRun: (run: TaskRun) => void, onLines: (batch: TaskLineBatch) => void): Promise<Unlisten> {
  if (isNativeRuntime()) {
    try {
      const { listen } = await import("@tauri-apps/api/event");
      const offRun = await listen<TaskRun>("task://run", (event) => onRun(event.payload));
      const offLines = await listen<TaskLineBatch>("task://lines", (event) => onLines(event.payload));
      return () => { offRun(); offLines(); };
    } catch {
      // Live updates are an enhancement; the UI still works by polling runs.
      return () => undefined;
    }
  }
  if (isDemoMode()) {
    const { demoTaskEvents } = await import("./demo-studio");
    return demoTaskEvents(onRun, onLines);
  }
  return () => undefined;
}

/** Subscribes to engine activity (lighting builds, cooks, player builds…)
 *  parsed from Unity and Unreal editor logs. */
export async function onEngineActivity(handler: (activity: EngineActivity) => void): Promise<Unlisten> {
  if (isNativeRuntime()) {
    try {
      const { listen } = await import("@tauri-apps/api/event");
      return await listen<EngineActivity>("engine://activity", (event) => handler(event.payload));
    } catch {
      return () => undefined;
    }
  }
  if (isDemoMode()) {
    const { demoEngineEvents } = await import("./demo-studio");
    return demoEngineEvents(handler);
  }
  return () => undefined;
}

// Machine-local, per-project workspace data (notes, to-dos, references) and
// user-added custom apps. Kept in localStorage to stay local-first in the web
// preview; the native app persists the git-shared half in
// `.pipelineos/workspace.json` and the local-only half in
// `.pipelineos/local.json` (see readWorkspace/readLocalConfig in bridge.ts).

export type Todo = { id: string; text: string; done: boolean };
export type Reference = { id: string; label: string; url: string };
export type ProjectWorkspace = { notes: string; todos: Todo[]; references: Reference[] };
/** Local-only, machine-specific project data — never synced via git. */
export type ProjectLocal = { todos: Todo[] };
export type CustomApp = { id: string; name: string; category: string; executable: string };

const EMPTY_WORKSPACE: ProjectWorkspace = { notes: "", todos: [], references: [] };
const EMPTY_LOCAL: ProjectLocal = { todos: [] };

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? { ...fallback, ...(JSON.parse(raw) as T) } : fallback;
  } catch {
    return fallback;
  }
}

export function loadWorkspace(projectPath: string): ProjectWorkspace {
  return read(`vantadeck.workspace:${projectPath}`, EMPTY_WORKSPACE);
}

export function saveWorkspace(projectPath: string, data: ProjectWorkspace): void {
  localStorage.setItem(`vantadeck.workspace:${projectPath}`, JSON.stringify(data));
}

export function loadLocalConfig(projectPath: string): ProjectLocal {
  return read(`vantadeck.local:${projectPath}`, EMPTY_LOCAL);
}

export function saveLocalConfig(projectPath: string, data: ProjectLocal): void {
  localStorage.setItem(`vantadeck.local:${projectPath}`, JSON.stringify(data));
}

export function loadCustomApps(): CustomApp[] {
  try {
    return JSON.parse(localStorage.getItem("vantadeck.customApps") ?? "[]") as CustomApp[];
  } catch {
    return [];
  }
}

export function saveCustomApps(apps: CustomApp[]): void {
  localStorage.setItem("vantadeck.customApps", JSON.stringify(apps));
}

export function newId(): string {
  return Math.random().toString(36).slice(2, 10);
}

/**
 * A pinned Quick Launch target. `id` is the unique pin key: an app id for the
 * app's primary install, `appId::executable` for a specific version, or a custom
 * app id. `appId` is what the launcher receives.
 */
export type QuickLaunchEntry = {
  id: string;
  appId: string;
  name: string;
  executable: string | null;
  version?: string | null;
  custom: boolean;
};

/** Quick Launch pins. Migrates the legacy `string[]` (app ids) format. */
export function loadQuickLaunch(): QuickLaunchEntry[] {
  try {
    const raw = JSON.parse(localStorage.getItem("vantadeck.quickLaunch") ?? "[]") as unknown[];
    return raw.map((entry) =>
      typeof entry === "string"
        ? { id: entry, appId: entry, name: entry, executable: null, custom: false }
        : (entry as QuickLaunchEntry),
    );
  } catch {
    return [];
  }
}

export function saveQuickLaunch(entries: QuickLaunchEntry[]): void {
  localStorage.setItem("vantadeck.quickLaunch", JSON.stringify(entries));
}

/** Custom thumbnail image path for a project (empty = use default). */
export function loadThumbnail(projectPath: string): string {
  return localStorage.getItem(`vantadeck.thumb:${projectPath}`) ?? "";
}

export function saveThumbnail(projectPath: string, imagePath: string): void {
  if (imagePath) localStorage.setItem(`vantadeck.thumb:${projectPath}`, imagePath);
  else localStorage.removeItem(`vantadeck.thumb:${projectPath}`);
}

export type ToolSource = { id: string; type: "local" | "git"; value: string };

/** User-configured extra tool sources (local folders or git repo URLs). */
export function loadToolSources(): ToolSource[] {
  try {
    return JSON.parse(localStorage.getItem("vantadeck.toolSources") ?? "[]") as ToolSource[];
  } catch {
    return [];
  }
}

export function saveToolSources(sources: ToolSource[]): void {
  localStorage.setItem("vantadeck.toolSources", JSON.stringify(sources));
}

/** User-assigned tags for a project (for grouping/filtering). */
export function loadTags(projectPath: string): string[] {
  try {
    return JSON.parse(localStorage.getItem(`vantadeck.tags:${projectPath}`) ?? "[]") as string[];
  } catch {
    return [];
  }
}

export function saveTags(projectPath: string, tags: string[]): void {
  localStorage.setItem(`vantadeck.tags:${projectPath}`, JSON.stringify(tags));
}

/** Health issue codes the user has dismissed/hidden for a given project. */
export function loadDismissedHealth(projectPath: string): string[] {
  try {
    return JSON.parse(localStorage.getItem(`vantadeck.health.dismissed:${projectPath}`) ?? "[]") as string[];
  } catch {
    return [];
  }
}

export function saveDismissedHealth(projectPath: string, codes: string[]): void {
  localStorage.setItem(`vantadeck.health.dismissed:${projectPath}`, JSON.stringify(codes));
}

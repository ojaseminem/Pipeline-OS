// Machine-local, per-project workspace data (notes, to-dos, references) and
// user-added custom apps. Kept in localStorage to stay local-first; a future
// iteration can persist team-shared data into .vantadeck/.

export type Todo = { id: string; text: string; done: boolean };
export type Reference = { id: string; label: string; url: string };
export type ProjectWorkspace = { notes: string; todos: Todo[]; references: Reference[] };
export type CustomApp = { id: string; name: string; category: string; executable: string };

const EMPTY_WORKSPACE: ProjectWorkspace = { notes: "", todos: [], references: [] };

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

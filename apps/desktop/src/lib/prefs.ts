// Behavior preferences (notifications, discovery, AI defaults, extra scan
// folders). One shared store so Settings and the rest of the app stay in sync.
import { useSyncExternalStore } from "react";
import type { AgentId } from "../bridge";

export type StartScreen = "Home" | "Projects" | "Activity";

export type Prefs = {
  /** Toast when an engine lighting build, cook, or player build finishes. */
  engineToasts: boolean;
  /** Toast when a pipeline task finishes. */
  taskToasts: boolean;
  /** Watch Unity/Unreal editor logs for builds and bakes. */
  engineWatch: boolean;
  /** Look for new projects (Unity Hub, Unreal, Godot, common folders) on launch. */
  discoverOnLaunch: boolean;
  /** Extra folders to include in app scans, ";"-separated. */
  extraScanFolders: string;
  /** Agent started by "Start AI agent" in a project. */
  defaultAgent: AgentId;
  /** Include the project brief when starting an agent. */
  writeBriefOnAgentStart: boolean;
  startScreen: StartScreen;
};

export const DEFAULT_PREFS: Prefs = {
  engineToasts: true,
  taskToasts: true,
  engineWatch: true,
  discoverOnLaunch: true,
  extraScanFolders: "",
  defaultAgent: "claude",
  writeBriefOnAgentStart: true,
  startScreen: "Home",
};

const KEY = "vantadeck.prefs";
const listeners = new Set<() => void>();

function read(): Prefs {
  try {
    return { ...DEFAULT_PREFS, ...(JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<Prefs>) };
  } catch {
    return DEFAULT_PREFS;
  }
}

let current: Prefs = read();

export function getPrefs(): Prefs {
  return current;
}

export function setPrefs(patch: Partial<Prefs>) {
  current = { ...current, ...patch };
  try { localStorage.setItem(KEY, JSON.stringify(current)); } catch { /* storage unavailable */ }
  for (const listener of listeners) listener();
}

/** Test hook: re-read storage (e.g. after localStorage.clear()). */
export function reloadPrefs() {
  current = read();
  for (const listener of listeners) listener();
}

export function usePrefs(): [Prefs, (patch: Partial<Prefs>) => void] {
  const value = useSyncExternalStore(
    (listener) => { listeners.add(listener); return () => listeners.delete(listener); },
    () => current,
  );
  return [value, setPrefs];
}

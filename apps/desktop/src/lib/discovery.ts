// Projects found on this computer (Unity Hub, Unreal recents, Godot's list,
// common project folders) that aren't in Pipeline OS yet. Results are cached
// by the backend, so the list shows instantly and refreshes in the background.
import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import { desktopApi, isNativeRuntime, studioApi, studioAvailable, type DiscoveredProject } from "../bridge";

export const DISCOVERY_KEY = ["discovery"] as const;

export function useDiscovery() {
  const queryClient = useQueryClient();
  const [refreshing, setRefreshing] = useState(false);
  const query = useQuery({ queryKey: DISCOVERY_KEY, queryFn: () => studioApi.discoverProjects(false), enabled: studioAvailable(), staleTime: Infinity });
  const refresh = useCallback(async () => {
    if (!studioAvailable()) return;
    setRefreshing(true);
    try {
      queryClient.setQueryData(DISCOVERY_KEY, await studioApi.discoverProjects(true));
    } finally {
      setRefreshing(false);
    }
  }, [queryClient]);
  // One entry per folder, however its path was spelled.
  const seen = new Set<string>();
  const projects = (query.data?.projects ?? []).filter((project) => {
    const key = project.path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  return { ...query, projects, scannedAt: query.data?.scannedAt ?? null, refresh, refreshing };
}

export type DiscoverySelection = {
  selected: Set<string>;
  set: (paths: Iterable<string>) => void;
  toggle: (path: string) => void;
};

const RECENT_SECONDS = 180 * 24 * 60 * 60;

/** Pre-selected by default: what the engines' own launchers list as recent,
 *  or anything touched in the last six months. Old one-off folders aren't. */
export function isRecommendedDiscovery(project: DiscoveredProject): boolean {
  const recent = project.modified > 0 && Date.now() / 1000 - project.modified < RECENT_SECONDS;
  return recent || (project.source !== "folders" && project.modified === 0);
}

/** Adds projects one by one (so one bad folder doesn't stop the rest). */
export async function addDiscoveredProjects(targets: DiscoveredProject[], queryClient: QueryClient): Promise<number> {
  if (!targets.length) return 0;
  let added = 0;
  const failures: string[] = [];
  const progress = targets.length > 5 ? toast.loading(`Adding ${targets.length} projects…`) : undefined;
  for (const project of targets) {
    try {
      if (isNativeRuntime()) await desktopApi.importProject(project.path, project.name);
      else await studioApi.dismissDiscovered(project.path);
      added++;
    } catch (error) {
      failures.push(`${project.name}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  if (progress !== undefined) toast.dismiss(progress);
  await queryClient.invalidateQueries({ queryKey: DISCOVERY_KEY });
  if (added) toast.success(`Added ${added} project${added === 1 ? "" : "s"}`);
  if (failures.length) toast.error(`Couldn't add ${failures.length} project${failures.length === 1 ? "" : "s"}`, { description: failures.slice(0, 3).join("\n") });
  return added;
}

/** Stops suggesting these projects. */
export async function hideDiscoveredProjects(targets: DiscoveredProject[], queryClient: QueryClient) {
  await Promise.all(targets.map((project) => studioApi.dismissDiscovered(project.path).catch(() => undefined)));
  await queryClient.invalidateQueries({ queryKey: DISCOVERY_KEY });
  if (targets.length > 1) toast.message(`Won't suggest ${targets.length} projects again`);
}

/** Selection state, owned by whoever renders the list (so onboarding's own
 *  "Finish" button can add the chosen projects). */
export function useDiscoverySelection(projects: DiscoveredProject[]): DiscoverySelection {
  // `null` = not touched yet: follow the recommended defaults as results arrive.
  const [chosen, setChosen] = useState<Set<string> | null>(null);
  const defaults = useMemo(() => new Set(projects.filter((project, _, all) => all.length <= 8 || isRecommendedDiscovery(project)).map((project) => project.path)), [projects]);
  const selected = useMemo(() => {
    const base = chosen ?? defaults;
    return new Set(projects.filter((project) => base.has(project.path)).map((project) => project.path));
  }, [chosen, defaults, projects]);
  return {
    selected,
    set: (paths: Iterable<string>) => setChosen(new Set(paths)),
    toggle: (path: string) => setChosen(() => { const next = new Set(selected); if (next.has(path)) next.delete(path); else next.add(path); return next; }),
  };
}

// Typed data layer over the Tauri IPC bridge. All reads go through TanStack
// Query so caching, background refetch, and loading/error states are uniform.
// See rfcs/0001-desktop-ui-architecture.md.
import { onlineManager, QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSyncExternalStore } from "react";
import { desktopApi, isNativeRuntime, loadDashboard, type DashboardSnapshot } from "../bridge";

export const queryKeys = {
  dashboard: ["dashboard"] as const,
  apps: ["apps"] as const,
  projects: ["projects"] as const,
  tools: ["tools"] as const,
};

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      // Queries and mutations are local IPC calls, not HTTP requests. TanStack's
      // default networkMode ("online") pauses them whenever the OS reports no
      // network, which froze every local screen (assets, planning…) offline.
      // Calls that do reach the network fail fast in the backend instead.
      queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000, networkMode: "always" },
      mutations: { networkMode: "always" },
    },
  });
}

/** Whether the OS reports a network connection; drives the offline badge. */
export function useOnline(): boolean {
  return useSyncExternalStore(
    (onChange) => onlineManager.subscribe(onChange),
    () => onlineManager.isOnline(),
    () => true,
  );
}

export function useDashboard() {
  return useQuery({ queryKey: queryKeys.dashboard, queryFn: () => loadDashboard() });
}

/** Detected applications. Only queried in the native runtime and when enabled. */
export function useApps(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.apps,
    queryFn: () => desktopApi.listApps(),
    enabled: enabled && isNativeRuntime(),
  });
}

export function useProjects(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.projects,
    queryFn: () => desktopApi.listProjects(),
    enabled: enabled && isNativeRuntime(),
  });
}

export function useTools(enabled: boolean) {
  return useQuery({
    queryKey: queryKeys.tools,
    queryFn: () => desktopApi.listTools(),
    enabled: enabled && isNativeRuntime(),
  });
}

/** Invalidate helpers so mutations refresh the relevant queries. */
export function useInvalidate() {
  const client = useQueryClient();
  return {
    apps: () => client.invalidateQueries({ queryKey: queryKeys.apps }),
    projects: () => client.invalidateQueries({ queryKey: queryKeys.projects }),
    tools: () => client.invalidateQueries({ queryKey: queryKeys.tools }),
    dashboard: () => client.invalidateQueries({ queryKey: queryKeys.dashboard }),
  };
}

export type { DashboardSnapshot };
export { useMutation };

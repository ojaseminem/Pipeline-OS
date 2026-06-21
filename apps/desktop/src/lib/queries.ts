// Typed data layer over the Tauri IPC bridge. All reads go through TanStack
// Query so caching, background refetch, and loading/error states are uniform.
// See rfcs/0001-desktop-ui-architecture.md.
import { QueryClient, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
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
      queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000 },
    },
  });
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

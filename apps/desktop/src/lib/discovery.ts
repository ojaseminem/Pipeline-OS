// Projects found on this computer (Unity Hub, Unreal recents, Godot's list,
// common project folders) that aren't in Pipeline OS yet. Results are cached
// by the backend, so the list shows instantly and refreshes in the background.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { studioApi, studioAvailable } from "../bridge";

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
  return { ...query, projects: query.data?.projects ?? [], scannedAt: query.data?.scannedAt ?? null, refresh, refreshing };
}

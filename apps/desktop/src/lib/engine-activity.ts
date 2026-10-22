// Engine activity (lighting builds, cooks, shader compiles, player builds)
// read from Unity/Unreal editor logs. One subscription, mounted by the app
// shell, keeps the shared query cache current and raises finish toasts.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { toast } from "sonner";
import { onEngineActivity, studioApi, studioAvailable, type EngineActivity, type EngineActivityKind } from "../bridge";
import { getPrefs, usePrefs } from "./prefs";

export const ENGINE_ACTIVITY_KEY = ["engine-activity"] as const;

export const ACTIVITY_LABELS: Record<EngineActivityKind, string> = {
  lighting: "Lighting build",
  shaders: "Shader compile",
  cook: "Cook",
  compile: "Script compile",
  build: "Player build",
  import: "Asset import",
};

export function activityTitle(activity: EngineActivity): string {
  return `${ACTIVITY_LABELS[activity.kind]}${activity.projectName ? ` · ${activity.projectName}` : ""}`;
}

export function useEngineActivity() {
  const [prefs] = usePrefs();
  return useQuery({
    queryKey: ENGINE_ACTIVITY_KEY,
    queryFn: () => studioApi.engineActivity(),
    enabled: studioAvailable() && prefs.engineWatch,
    staleTime: 60_000,
  });
}

/** Mount once: merges live events into the cache and toasts on finish. */
export function useEngineActivityFeed() {
  const queryClient = useQueryClient();
  useEffect(() => {
    if (!studioAvailable()) return;
    let off: (() => void) | undefined;
    let disposed = false;
    void onEngineActivity((activity) => {
      const prefs = getPrefs();
      if (!prefs.engineWatch) return;
      queryClient.setQueryData<EngineActivity[]>(ENGINE_ACTIVITY_KEY, (list = []) => [activity, ...list.filter((item) => item.id !== activity.id)].slice(0, 40));
      if (activity.status !== "running" && prefs.engineToasts) {
        const title = `${activityTitle(activity)} ${activity.status === "succeeded" ? "finished" : "failed"}`;
        if (activity.status === "succeeded") toast.success(title, { description: activity.detail });
        else toast.error(title, { description: activity.detail });
      }
    }).then((unlisten) => { if (disposed) unlisten(); else off = unlisten; });
    return () => { disposed = true; off?.(); };
  }, [queryClient]);
}

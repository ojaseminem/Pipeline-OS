import { Box, CheckCircle2, Cpu, Loader2, XCircle } from "lucide-react";
import { Progress } from "@/components/ui/progress";
import type { EngineActivity } from "../bridge";
import { formatLastOpened } from "../lib/format";
import { activityTitle } from "../lib/engine-activity";

export function EngineActivityRow({ activity }: { activity: EngineActivity }) {
  const Icon = activity.status === "running" ? Loader2 : activity.status === "succeeded" ? CheckCircle2 : XCircle;
  return (
    <div className="flex items-start gap-3 rounded-md px-2 py-2 hover:bg-secondary/40">
      <span className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-secondary text-muted-foreground" title={activity.engine === "unreal" ? "Unreal Engine" : "Unity"}>
        {activity.engine === "unreal" ? <Cpu size={14} /> : <Box size={14} />}
      </span>
      <span className="min-w-0 flex-1 space-y-1">
        <span className="flex items-center gap-2 text-sm">
          <span className="truncate font-medium">{activityTitle(activity)}</span>
          <Icon size={14} className={activity.status === "running" ? "animate-spin text-primary" : activity.status === "succeeded" ? "text-success" : "text-destructive"} aria-label={activity.status} />
          <span className="ml-auto shrink-0 text-xs text-muted-foreground">{formatLastOpened(activity.finishedAt ?? activity.updatedAt)}</span>
        </span>
        {activity.status === "running" && activity.progress != null ? <Progress value={activity.progress} className="h-1.5" /> : null}
        <span className="block truncate font-mono text-[11px] text-muted-foreground" title={activity.detail}>{activity.detail}</span>
      </span>
    </div>
  );
}

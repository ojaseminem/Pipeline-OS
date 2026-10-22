// "Active time": minutes the project was actually being worked on, counted
// from editor log activity and project file changes (never keystrokes or
// screen time). Shown on the project overview for reference.
import { useQuery } from "@tanstack/react-query";
import { Timer } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { studioApi, studioAvailable } from "../bridge";
import { formatMinutes } from "../lib/active-time";
import { formatLastOpened } from "../lib/format";

export function ActiveTimeCard({ projectPath }: { projectPath: string }) {
  const summary = useQuery({ queryKey: ["active-time", projectPath], queryFn: () => studioApi.activeTime(projectPath), enabled: studioAvailable(), refetchInterval: 60_000 });
  const data = summary.data;
  const max = Math.max(1, ...(data?.days ?? []).map((day) => day.minutes));
  return (
    <Card><CardContent className="space-y-3 p-5">
      <div className="flex items-center gap-2">
        <Timer size={16} className="text-primary" aria-hidden />
        <h2 className="text-base font-semibold">Active time</h2>
        {data?.lastActive ? <span className="ml-auto text-xs text-muted-foreground">Last active {formatLastOpened(data.lastActive)}</span> : null}
      </div>
      {data ? (
        <>
          <dl className="grid grid-cols-3 gap-2 text-center">
            {([["Today", data.todayMinutes], ["Last 7 days", data.weekMinutes], ["All time", data.totalMinutes]] as const).map(([label, value]) => (
              <div key={label} className="rounded-lg bg-secondary/50 px-2 py-2"><dt className="text-xs text-muted-foreground">{label}</dt><dd className="text-lg font-semibold tabular-nums">{formatMinutes(value)}</dd></div>
            ))}
          </dl>
          <div className="flex h-14 items-end gap-1" role="img" aria-label="Active time over the last 14 days">
            {data.days.map((day) => (
              <span key={day.date} title={`${day.date}: ${formatMinutes(day.minutes)}`} className="flex-1 rounded-sm bg-primary/70" style={{ height: `${Math.max(day.minutes ? 8 : 2, (day.minutes / max) * 100)}%`, opacity: day.minutes ? 1 : 0.25 }} />
            ))}
          </div>
          <p className="text-xs text-muted-foreground">Counted while the engine editor is working on this project or its files change. Stays on this machine.</p>
        </>
      ) : <p className="text-sm text-muted-foreground">{studioAvailable() ? "Measuring… Time appears once you work on the project with Pipeline OS running." : "Active time is tracked by the desktop app."}</p>}
    </CardContent></Card>
  );
}

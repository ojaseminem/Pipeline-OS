// Activity: every pipeline task run across projects (live), plus the local
// activity log (imports, scans, launches, VCS operations).
import { useQuery } from "@tanstack/react-query";
import { Activity, History, Trash2, Workflow } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { desktopApi, isNativeRuntime, studioApi, type RunStatus } from "../bridge";
import { formatLastOpened } from "../lib/format";
import { confirmAction } from "../lib/confirm";
import { EmptyState } from "./empty-state";
import { PageHeader } from "./page-header";
import { RunRow } from "./task-runs";
import { useTaskRuns } from "../lib/task-runs-context";

const FILTERS: Array<{ id: RunStatus | "all"; label: string }> = [
  { id: "all", label: "All" },
  { id: "running", label: "Running" },
  { id: "failed", label: "Failed" },
  { id: "succeeded", label: "Succeeded" },
];

export function ActivityScreen() {
  const { runs, active, refresh } = useTaskRuns();
  const [filter, setFilter] = useState<RunStatus | "all">("all");
  const log = useQuery({ queryKey: ["activity-log"], queryFn: () => desktopApi.recentActivity(40), enabled: isNativeRuntime(), refetchInterval: 15_000 });
  const shown = runs.filter((run) => filter === "all" || run.status === filter);

  async function clearHistory() {
    if (!(await confirmAction({ title: "Clear task history?", description: "Finished runs are removed from this list. Running tasks keep running.", confirmLabel: "Clear history" }))) return;
    await studioApi.clearRunHistory().catch(() => undefined);
    refresh();
  }

  return (
    <section className="pos-enter space-y-5">
      <PageHeader title="Activity" description={active.length ? `${active.length} task${active.length === 1 ? "" : "s"} running now.` : "Builds, renders, and exports across all projects, plus a log of what happened on this machine."}
        actions={runs.length ? <Button variant="outline" size="sm" onClick={() => void clearHistory()}><Trash2 size={14} /> Clear history</Button> : null} />
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.6fr)_minmax(0,1fr)]">
        <Card><CardContent className="space-y-2 p-4">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="mr-auto flex items-center gap-2 text-sm font-semibold"><Workflow size={15} className="text-primary" /> Pipeline runs</h2>
            {FILTERS.map((item) => (
              <button key={item.id} onClick={() => setFilter(item.id)} aria-pressed={filter === item.id} className={cn("rounded-full px-2.5 py-0.5 text-xs", filter === item.id ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>{item.label}</button>
            ))}
          </div>
          {shown.length ? <div className="space-y-0.5">{shown.map((run) => <RunRow key={run.id} run={run} />)}</div>
            : <EmptyState compact icon={Workflow} title={runs.length ? "No runs match this filter" : "No pipeline runs yet"} description={runs.length ? undefined : "Add a build, render, or export to a project's Pipeline tab and run it from there, the CLI, or an AI agent."} />}
        </CardContent></Card>
        <Card><CardContent className="space-y-2 p-4">
          <h2 className="flex items-center gap-2 text-sm font-semibold"><History size={15} className="text-primary" /> Activity log</h2>
          {log.data?.length ? (
            <ul className="space-y-1.5">{log.data.map((entry) => (
              <li key={entry.id} className="flex items-start gap-2 text-sm"><Activity size={14} className="mt-0.5 shrink-0 text-muted-foreground" /><span className="min-w-0 flex-1"><span className="block">{entry.message}</span><span className="block text-xs text-muted-foreground">{formatLastOpened(entry.createdAt)}</span></span></li>
            ))}</ul>
          ) : <EmptyState compact icon={History} title="Nothing logged yet" description={isNativeRuntime() ? "Scans, imports, and launches show up here." : "The activity log is available in the desktop app."} />}
        </CardContent></Card>
      </div>
    </section>
  );
}

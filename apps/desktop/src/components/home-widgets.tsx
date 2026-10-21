// Home dashboard widgets: first-run checklist, in-progress pipeline tasks,
// and open issues across every project ("My work").
import { useQuery } from "@tanstack/react-query";
import { Check, FolderPlus, KanbanSquare, Rocket, ScanSearch, Star, Workflow } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { studioApi, studioAvailable } from "../bridge";
import { Avatar } from "./project-board";
import { KIND_META, PRIORITY_META, STATUS_COLUMNS } from "../lib/issue-meta";
import { RunRow } from "./task-runs";
import { useTaskRuns } from "../lib/task-runs-context";

type Step = { id: string; title: string; description: string; done: boolean; action: string; onAction: () => void; icon: typeof Rocket };

/** Shown until the basics are done: apps found, a project added, a quick-launch pin. */
export function GettingStarted({ appsFound, projectsAdded, pinned, onScan, onAddProject, onPin, onDismiss }: {
  appsFound: boolean; projectsAdded: boolean; pinned: boolean;
  onScan: () => void; onAddProject: () => void; onPin: () => void; onDismiss: () => void;
}) {
  const steps: Step[] = [
    { id: "scan", title: "Find your creative apps", description: "Detect installed engines, DCCs, and editors, with every version side by side.", done: appsFound, action: "Scan now", onAction: onScan, icon: ScanSearch },
    { id: "project", title: "Add your first project", description: "Import a project folder, or drag one onto this window.", done: projectsAdded, action: "Add project", onAction: onAddProject, icon: FolderPlus },
    { id: "pin", title: "Pin apps to Quick Launch", description: "Star the versions you use most for one-click launching.", done: pinned, action: "Choose apps", onAction: onPin, icon: Star },
  ];
  const completed = steps.filter((step) => step.done).length;
  return (
    <Card className="overflow-hidden"><CardContent className="p-0">
      <div className="flex flex-wrap items-center gap-3 border-b border-border px-5 py-4">
        <span className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/15 text-primary"><Rocket size={18} aria-hidden /></span>
        <div className="min-w-0 flex-1">
          <h2 className="font-semibold">Set up Pipeline OS</h2>
          <p className="text-sm text-muted-foreground">{completed} of {steps.length} done. Takes about two minutes.</p>
        </div>
        <div className="h-1.5 w-32 overflow-hidden rounded-full bg-secondary" aria-hidden><div className="h-full rounded-full bg-primary transition-[width]" style={{ width: `${(completed / steps.length) * 100}%` }} /></div>
        <Button variant="ghost" size="sm" onClick={onDismiss}>Dismiss</Button>
      </div>
      <ol className="grid divide-y divide-border md:grid-cols-3 md:divide-x md:divide-y-0">
        {steps.map((step) => (
          <li key={step.id} className="flex gap-3 p-5">
            <span className={cn("mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full border text-xs", step.done ? "border-success bg-success/15 text-success" : "border-border text-muted-foreground")}>{step.done ? <Check size={13} /> : <step.icon size={13} />}</span>
            <div className="min-w-0 space-y-1.5">
              <p className={cn("text-sm font-medium", step.done && "text-muted-foreground line-through")}>{step.title}</p>
              <p className="text-xs text-muted-foreground">{step.description}</p>
              {!step.done ? <Button size="sm" variant="outline" className="mt-1" onClick={step.onAction}>{step.action}</Button> : null}
            </div>
          </li>
        ))}
      </ol>
    </CardContent></Card>
  );
}

/** Running and recently finished pipeline tasks across projects. */
export function ActiveTasksCard({ onOpenProject }: { onOpenProject: (target: { path: string; name: string }) => void }) {
  const { runs, active } = useTaskRuns();
  const recent = runs.filter((run) => run.status !== "running").slice(0, Math.max(0, 4 - active.length));
  if (!runs.length) return null;
  return (
    <Card><CardContent className="p-4">
      <div className="mb-1 flex items-center justify-between text-sm font-semibold">
        <span className="flex items-center gap-2"><Workflow size={15} className="text-primary" aria-hidden /> Pipeline</span>
        <span className="text-xs font-normal text-muted-foreground">{active.length ? `${active.length} running` : "Idle"}</span>
      </div>
      <div className="space-y-0.5">
        {[...active, ...recent].map((run) => <RunRow key={run.id} run={run} />)}
      </div>
      {active[0] ? <Button variant="link" size="sm" className="h-auto px-2 text-xs" onClick={() => onOpenProject({ path: active[0].projectPath, name: active[0].projectName })}>Open {active[0].projectName}</Button> : null}
    </CardContent></Card>
  );
}

/** Open issues across all projects, most urgent first. */
export function MyWorkCard({ onOpenProject }: { onOpenProject: (target: { path: string; name: string }, tab?: string) => void }) {
  const issues = useQuery({ queryKey: ["issues-everywhere"], queryFn: () => studioApi.issuesEverywhere(), enabled: studioAvailable(), refetchOnWindowFocus: true });
  const open = (issues.data ?? []).filter((issue) => issue.status !== "backlog").slice(0, 6);
  if (!issues.data?.length) return null;
  return (
    <Card><CardContent className="p-4">
      <div className="mb-2 flex items-center justify-between text-sm font-semibold">
        <span className="flex items-center gap-2"><KanbanSquare size={15} className="text-primary" aria-hidden /> Open work</span>
        <span className="text-xs font-normal text-muted-foreground">{issues.data.length} open across projects</span>
      </div>
      <ul className="space-y-0.5">
        {open.map((issue) => {
          const Kind = KIND_META[issue.kind].icon;
          const Priority = PRIORITY_META[issue.priority].icon;
          return (
            <li key={`${issue.projectPath}:${issue.id}`}>
              <button onClick={() => onOpenProject({ path: issue.projectPath, name: issue.projectName }, "board")} className="flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-sm hover:bg-muted/50">
                <Kind size={14} className={cn("shrink-0", KIND_META[issue.kind].className)} aria-label={KIND_META[issue.kind].label} />
                <span className="min-w-0 flex-1 truncate">{issue.title}</span>
                {Priority ? <Priority size={13} className={cn("shrink-0", PRIORITY_META[issue.priority].className)} aria-label={PRIORITY_META[issue.priority].label} /> : null}
                <span className="shrink-0 rounded bg-secondary px-1.5 py-px text-[10px] text-muted-foreground">{STATUS_COLUMNS.find((column) => column.id === issue.status)?.label}</span>
                <Avatar name={issue.assignee} />
              </button>
            </li>
          );
        })}
      </ul>
    </CardContent></Card>
  );
}

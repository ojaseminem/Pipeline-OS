// Planning: project management across every project. A dashboard (progress,
// workload, what moved this week, active time), one board for all projects,
// and a sortable list. Issues stay git-native in each project's .pipelineos.
import { useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, BarChart3, CheckCircle2, CircleDot, Clock, Flame, KanbanSquare, List, Search, Timer } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { studioApi, studioAvailable, type Issue, type IssueStatus } from "../bridge";
import { KIND_META, PRIORITY_META, STATUS_COLUMNS } from "../lib/issue-meta";
import { formatLastOpened } from "../lib/format";
import { formatMinutes } from "../lib/active-time";
import { Avatar } from "./project-board";
import { EmptyState } from "./empty-state";
import { PageHeader } from "./page-header";

type Row = Issue & { projectName: string; projectPath: string };
type View = "dashboard" | "board" | "list";
const PRIORITY_RANK = { urgent: 0, high: 1, medium: 2, low: 3, none: 4 } as const;
const WEEK_MS = 7 * 24 * 60 * 60 * 1000;

function Stat({ icon: Icon, label, value, hint, tone }: { icon: typeof Clock; label: string; value: string | number; hint?: string; tone?: string }) {
  return (
    <Card><CardContent className="flex items-center gap-3 p-4">
      <span className={cn("flex h-9 w-9 items-center justify-center rounded-lg bg-secondary", tone)}><Icon size={17} aria-hidden /></span>
      <span className="min-w-0"><span className="block text-2xl font-semibold tabular-nums leading-none">{value}</span><span className="mt-1 block text-xs text-muted-foreground">{label}{hint ? ` · ${hint}` : ""}</span></span>
    </CardContent></Card>
  );
}

function Bar({ parts, total }: { parts: Array<{ value: number; className: string; label: string }>; total: number }) {
  return (
    <div className="flex h-2 w-full overflow-hidden rounded-full bg-secondary" role="img" aria-label={parts.map((part) => `${part.label} ${part.value}`).join(", ")}>
      {total ? parts.map((part) => part.value ? <span key={part.label} className={part.className} style={{ width: `${(part.value / total) * 100}%` }} title={`${part.label}: ${part.value}`} /> : null) : null}
    </div>
  );
}

const STATUS_COLORS: Record<IssueStatus, string> = {
  backlog: "bg-muted-foreground/30",
  todo: "bg-muted-foreground/60",
  "in-progress": "bg-primary",
  review: "bg-warning",
  done: "bg-success",
};

export function PlanningScreen({ projects, onOpenProject }: { projects: Array<{ name: string; path: string }>; onOpenProject: (project: { name: string; path: string }, tab?: string) => void }) {
  const available = studioAvailable();
  const queryClient = useQueryClient();
  const [view, setView] = useState<View>(() => { try { return (localStorage.getItem("vantadeck.planningView") as View) || "dashboard"; } catch { return "dashboard"; } });
  const [query, setQuery] = useState("");
  const [projectFilter, setProjectFilter] = useState<string | null>(null);
  const [assigneeFilter, setAssigneeFilter] = useState<string | null>(null);
  const [sort, setSort] = useState<"priority" | "updated" | "project">("priority");

  const results = useQueries({
    queries: projects.map((project) => ({
      queryKey: ["issues", project.path],
      queryFn: () => studioApi.issues(project.path),
      enabled: available,
    })),
  });
  const activeTime = useQuery({ queryKey: ["active-time-all"], queryFn: () => studioApi.activeTimeAll(), enabled: available, staleTime: 60_000 });

  const rows: Row[] = useMemo(() => results.flatMap((result, index) => (result.data?.issues ?? []).map((issue) => ({ ...issue, projectName: projects[index].name, projectPath: projects[index].path }))), [results, projects]);
  const loading = results.some((result) => result.isLoading);
  const assignees = useMemo(() => [...new Set(rows.map((row) => row.assignee).filter((name): name is string => Boolean(name)))].sort(), [rows]);
  const visible = rows.filter((row) =>
    (!projectFilter || row.projectPath === projectFilter)
    && (!assigneeFilter || row.assignee === assigneeFilter)
    && (!query || `${row.title} ${row.id} ${row.projectName} ${(row.labels ?? []).join(" ")}`.toLowerCase().includes(query.toLowerCase())));

  function changeView(next: View) {
    setView(next);
    try { localStorage.setItem("vantadeck.planningView", next); } catch { /* storage unavailable */ }
  }

  async function move(row: Row, status: IssueStatus) {
    const key = ["issues", row.projectPath];
    queryClient.setQueryData(key, (data: { issues: Issue[]; problems: string[] } | undefined) => data && { ...data, issues: data.issues.map((item) => (item.id === row.id ? { ...item, status } : item)) });
    try {
      const issue: Issue & Partial<Pick<Row, "projectName" | "projectPath">> = { ...row, status };
      delete issue.projectName;
      delete issue.projectPath;
      await studioApi.updateIssue(row.projectPath, issue);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
    void queryClient.invalidateQueries({ queryKey: key });
  }

  const now = Date.now();
  const open = visible.filter((row) => row.status !== "done");
  const doneThisWeek = visible.filter((row) => row.status === "done" && now - Date.parse(row.updatedAt) < WEEK_MS).length;
  const createdThisWeek = visible.filter((row) => now - Date.parse(row.createdAt) < WEEK_MS).length;
  const hot = open.filter((row) => row.priority === "urgent" || row.priority === "high").length;
  const weekMinutes = (activeTime.data ?? []).filter((entry) => !projectFilter || entry.path === projectFilter).reduce((sum, entry) => sum + entry.weekMinutes, 0);

  const filters = (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative min-w-52 flex-1">
        <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" aria-hidden />
        <Input aria-label="Filter issues" placeholder="Filter issues…" value={query} onChange={(event) => setQuery(event.target.value)} className="h-9 pl-8" />
      </div>
      <select aria-label="Project" value={projectFilter ?? ""} onChange={(event) => setProjectFilter(event.target.value || null)} className="h-9 rounded-md border border-input bg-transparent px-2 text-sm">
        <option value="">All projects</option>
        {projects.map((project) => <option key={project.path} value={project.path}>{project.name}</option>)}
      </select>
      {assignees.length ? (
        <select aria-label="Assignee" value={assigneeFilter ?? ""} onChange={(event) => setAssigneeFilter(event.target.value || null)} className="h-9 rounded-md border border-input bg-transparent px-2 text-sm">
          <option value="">Everyone</option>
          {assignees.map((name) => <option key={name} value={name}>{name}</option>)}
        </select>
      ) : null}
      <div role="tablist" aria-label="Planning view" className="inline-flex rounded-lg border border-border bg-secondary/40 p-0.5">
        {([["dashboard", "Dashboard", BarChart3], ["board", "Board", KanbanSquare], ["list", "List", List]] as const).map(([id, label, Icon]) => (
          <button key={id} role="tab" aria-selected={view === id} onClick={() => changeView(id)} className={cn("flex items-center gap-1.5 rounded-md px-3 py-1 text-sm", view === id ? "bg-background font-medium shadow-sm" : "text-muted-foreground hover:text-foreground")}><Icon size={14} /> {label}</button>
        ))}
      </div>
    </div>
  );

  const issueCard = (row: Row, compact = false) => {
    const kind = KIND_META[row.kind];
    const priority = PRIORITY_META[row.priority];
    const KindIcon = kind.icon;
    const PriorityIcon = priority.icon;
    return (
      <button key={`${row.projectPath}:${row.id}`} onClick={() => onOpenProject({ name: row.projectName, path: row.projectPath }, "board")} className="w-full rounded-lg border border-border bg-card p-2.5 text-left text-sm transition-colors hover:border-primary/40">
        <span className="flex items-start gap-2">
          <KindIcon size={14} className={cn("mt-0.5 shrink-0", kind.className)} aria-label={kind.label} />
          <span className="min-w-0 flex-1 font-medium leading-snug">{row.title}</span>
          {PriorityIcon ? <PriorityIcon size={14} className={cn("shrink-0", priority.className)} aria-label={priority.label} /> : null}
        </span>
        {!compact ? <span className="mt-1.5 flex items-center gap-2 text-xs text-muted-foreground"><span className="truncate">{row.projectName}</span><span className="ml-auto shrink-0">{formatLastOpened(row.updatedAt)}</span><Avatar name={row.assignee} /></span> : null}
      </button>
    );
  };

  let content;
  if (!available) content = <EmptyState icon={KanbanSquare} title="Planning lives in the desktop app" description="Open Pipeline OS on your machine to plan across projects." />;
  else if (!projects.length) content = <EmptyState icon={KanbanSquare} title="No projects yet" description="Add a project, then plan its work on the board. Everything shows up here across projects." />;
  else if (view === "dashboard") {
    const byProject = projects.filter((project) => !projectFilter || project.path === projectFilter).map((project) => {
      const items = visible.filter((row) => row.projectPath === project.path);
      const time = activeTime.data?.find((entry) => entry.path === project.path);
      return { project, items, time };
    }).sort((a, b) => b.items.filter((row) => row.status !== "done").length - a.items.filter((row) => row.status !== "done").length);
    const workload = assignees.map((name) => ({ name, open: open.filter((row) => row.assignee === name).length, inProgress: open.filter((row) => row.assignee === name && row.status === "in-progress").length })).sort((a, b) => b.open - a.open);
    const unassigned = open.filter((row) => !row.assignee).length;
    const recent = [...visible].sort((a, b) => Date.parse(b.updatedAt) - Date.parse(a.updatedAt)).slice(0, 8);
    content = (
      <div className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-5">
          <Stat icon={CircleDot} label="Open issues" value={open.length} hint={`${open.filter((row) => row.status === "in-progress").length} in progress`} tone="text-primary" />
          <Stat icon={Flame} label="High priority" value={hot} tone="text-warning" />
          <Stat icon={CheckCircle2} label="Done this week" value={doneThisWeek} tone="text-success" />
          <Stat icon={ArrowRight} label="Created this week" value={createdThisWeek} />
          <Stat icon={Timer} label="Active time this week" value={formatMinutes(weekMinutes)} tone="text-primary" />
        </div>
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.5fr)_minmax(0,1fr)]">
          <Card><CardContent className="space-y-3 p-4">
            <h2 className="text-sm font-semibold">Progress by project</h2>
            <ul className="space-y-3">
              {byProject.map(({ project, items, time }) => {
                const done = items.filter((row) => row.status === "done").length;
                return (
                  <li key={project.path} className="space-y-1.5">
                    <div className="flex items-center gap-2 text-sm">
                      <button className="truncate font-medium hover:text-primary" onClick={() => onOpenProject(project, "board")}>{project.name}</button>
                      <span className="ml-auto shrink-0 text-xs text-muted-foreground tabular-nums">{items.length ? `${done}/${items.length} done` : "No issues"}{time?.weekMinutes ? ` · ${formatMinutes(time.weekMinutes)} this week` : ""}</span>
                    </div>
                    <Bar total={items.length} parts={STATUS_COLUMNS.map((column) => ({ label: column.label, value: items.filter((row) => row.status === column.id).length, className: STATUS_COLORS[column.id] }))} />
                  </li>
                );
              })}
            </ul>
            <div className="flex flex-wrap gap-3 pt-1 text-xs text-muted-foreground">{STATUS_COLUMNS.map((column) => <span key={column.id} className="flex items-center gap-1.5"><span className={cn("size-2 rounded-full", STATUS_COLORS[column.id])} />{column.label}</span>)}</div>
          </CardContent></Card>
          <div className="space-y-4">
            <Card><CardContent className="space-y-2 p-4">
              <h2 className="text-sm font-semibold">Workload</h2>
              {workload.length || unassigned ? (
                <ul className="space-y-1.5 text-sm">
                  {workload.map((entry) => (
                    <li key={entry.name} className="flex items-center gap-2">
                      <Avatar name={entry.name} /><span className="min-w-0 flex-1 truncate">{entry.name}</span>
                      <span className="text-xs text-muted-foreground tabular-nums">{entry.inProgress} active · {entry.open} open</span>
                    </li>
                  ))}
                  {unassigned ? <li className="flex items-center gap-2 text-muted-foreground"><span className="size-5" /><span className="flex-1">Unassigned</span><span className="text-xs tabular-nums">{unassigned} open</span></li> : null}
                </ul>
              ) : <p className="text-sm text-muted-foreground">Assign issues to see who's working on what.</p>}
            </CardContent></Card>
            <Card><CardContent className="space-y-2 p-4">
              <h2 className="text-sm font-semibold">Recently updated</h2>
              {recent.length ? <div className="space-y-1.5">{recent.map((row) => issueCard(row))}</div> : <p className="text-sm text-muted-foreground">{loading ? "Loading…" : "Nothing yet."}</p>}
            </CardContent></Card>
          </div>
        </div>
      </div>
    );
  } else if (view === "board") {
    content = (
      <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-5">
        {STATUS_COLUMNS.map((column) => {
          const items = visible.filter((row) => row.status === column.id).sort((a, b) => PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority]);
          return (
            <section key={column.id} aria-label={`${column.label} column`} className="flex min-h-40 flex-col gap-2 rounded-xl border border-border bg-secondary/20 p-2"
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => { const id = event.dataTransfer.getData("text/plain"); const row = rows.find((item) => `${item.projectPath}:${item.id}` === id); if (row) void move(row, column.id); }}>
              <h2 className="flex items-center justify-between px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground"><span className="flex items-center gap-1.5"><span className={cn("size-2 rounded-full", STATUS_COLORS[column.id])} />{column.label}</span><span className="tabular-nums">{items.length}</span></h2>
              {items.map((row) => <div key={`${row.projectPath}:${row.id}`} draggable onDragStart={(event) => event.dataTransfer.setData("text/plain", `${row.projectPath}:${row.id}`)}>{issueCard(row)}</div>)}
            </section>
          );
        })}
      </div>
    );
  } else {
    const sorted = [...visible].sort((a, b) =>
      sort === "priority" ? PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority] || Date.parse(b.updatedAt) - Date.parse(a.updatedAt)
        : sort === "updated" ? Date.parse(b.updatedAt) - Date.parse(a.updatedAt)
          : a.projectName.localeCompare(b.projectName));
    content = (
      <Card><CardContent className="overflow-x-auto p-0">
        <table className="w-full text-sm">
          <thead><tr className="border-b border-border text-left text-xs text-muted-foreground">
            <th className="px-3 py-2 font-medium">Issue</th>
            <th className="px-3 py-2 font-medium"><button onClick={() => setSort("project")} className={cn(sort === "project" && "text-foreground")}>Project</button></th>
            <th className="px-3 py-2 font-medium">Status</th>
            <th className="px-3 py-2 font-medium"><button onClick={() => setSort("priority")} className={cn(sort === "priority" && "text-foreground")}>Priority</button></th>
            <th className="px-3 py-2 font-medium">Assignee</th>
            <th className="px-3 py-2 font-medium"><button onClick={() => setSort("updated")} className={cn(sort === "updated" && "text-foreground")}>Updated</button></th>
          </tr></thead>
          <tbody>
            {sorted.map((row) => (
              <tr key={`${row.projectPath}:${row.id}`} className="border-b border-border/60 hover:bg-secondary/30">
                <td className="px-3 py-2"><button className="text-left font-medium hover:text-primary" onClick={() => onOpenProject({ name: row.projectName, path: row.projectPath }, "board")}>{row.title}</button></td>
                <td className="px-3 py-2 text-muted-foreground">{row.projectName}</td>
                <td className="px-3 py-2">
                  <select aria-label={`Status of ${row.title}`} value={row.status} onChange={(event) => void move(row, event.target.value as IssueStatus)} className="rounded-md border border-input bg-transparent px-1.5 py-0.5 text-xs">
                    {STATUS_COLUMNS.map((column) => <option key={column.id} value={column.id}>{column.label}</option>)}
                  </select>
                </td>
                <td className={cn("px-3 py-2", PRIORITY_META[row.priority].className)}>{PRIORITY_META[row.priority].label}</td>
                <td className="px-3 py-2"><span className="flex items-center gap-1.5"><Avatar name={row.assignee} />{row.assignee ?? <span className="text-muted-foreground">—</span>}</span></td>
                <td className="px-3 py-2 text-muted-foreground">{formatLastOpened(row.updatedAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {!sorted.length ? <EmptyState compact icon={Search} title={loading ? "Loading issues…" : "No issues match"} /> : null}
      </CardContent></Card>
    );
  }

  return (
    <section className="pos-enter space-y-5">
      <PageHeader title="Planning" description="Every project's work in one place: progress, workload, and a shared board. Issues stay in each project's repository." />
      {available && projects.length ? filters : null}
      {content}
    </section>
  );
}

// Project "Pipeline" tab: repeatable build/cook/render/export/test tasks with
// live progress, history, and a trust review for commands that arrived from
// the repository.
import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Copy, MoreHorizontal, Pencil, Play, Plus, ShieldAlert, ShieldCheck, Trash2, Workflow } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { studioApi, studioAvailable, type TaskDefinition, type TaskKind } from "../bridge";
import { confirmAction } from "../lib/confirm";
import { EmptyState } from "./empty-state";
import { RunProgress, RunRow } from "./task-runs";
import { KIND_ICONS, cliRunCommand } from "../lib/task-meta";
import { useTaskRuns } from "../lib/task-runs-context";

const KIND_LABELS: Record<TaskKind, string> = { build: "Build", test: "Test", render: "Render", export: "Export", cook: "Cook / package", script: "Script" };

function slugify(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40) || "task";
}

function uniqueId(base: string, taken: Set<string>): string {
  let id = slugify(base);
  for (let n = 2; taken.has(id); n++) id = `${slugify(base)}-${n}`;
  return id;
}

type Draft = { original: string | null; id: string; name: string; kind: TaskKind; description: string; program: string; args: string; cwd: string };

function toDraft(task: TaskDefinition | null, original: string | null): Draft {
  return {
    original,
    id: task?.id ?? "",
    name: task?.name ?? "",
    kind: task?.kind ?? "script",
    description: task?.description ?? "",
    program: task?.program ?? "",
    args: (task?.args ?? []).join("\n"),
    cwd: task?.cwd ?? "",
  };
}

export function ProjectPipeline({ projectPath }: { projectPath: string }) {
  const available = studioAvailable();
  const queryClient = useQueryClient();
  const { runs, start } = useTaskRuns();
  const tasksQuery = useQuery({ queryKey: ["tasks", projectPath], queryFn: () => studioApi.tasks(projectPath), enabled: available });
  const templates = useQuery({ queryKey: ["task-templates"], queryFn: () => studioApi.templates(), enabled: available, staleTime: Infinity });
  const [draft, setDraft] = useState<Draft | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const tasks = useMemo(() => tasksQuery.data?.tasks ?? [], [tasksQuery.data]);
  const trusted = tasksQuery.data?.trusted ?? false;
  const projectRuns = runs.filter((run) => run.projectPath === projectPath);
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["tasks", projectPath] });

  async function save(next: TaskDefinition[], message: string) {
    try {
      await studioApi.saveTasks(projectPath, next);
      await refresh();
      toast.success(message);
      return true;
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
      return false;
    }
  }

  async function run(task: TaskDefinition) {
    if (!trusted) { setReviewing(true); return; }
    try { await start(projectPath, task.id); }
    catch { setReviewing(true); }
  }

  function addFromTemplate(template: TaskDefinition | null) {
    const taken = new Set(tasks.map((task) => task.id));
    const base = template ?? { id: "task", name: "New task", kind: "script" as TaskKind, program: "", args: [] };
    setDraft(toDraft({ ...base, id: uniqueId(base.id, taken) }, null));
  }

  async function submitDraft() {
    if (!draft) return;
    const name = draft.name.trim();
    const program = draft.program.trim();
    if (!name || !program) { toast.error("A task needs a name and a program."); return; }
    const taken = new Set(tasks.filter((task) => task.id !== draft.original).map((task) => task.id));
    const id = draft.id.trim() ? slugify(draft.id) : uniqueId(name, taken);
    if (taken.has(id)) { toast.error(`Another task already uses the id "${id}".`); return; }
    const task: TaskDefinition = {
      id,
      name,
      kind: draft.kind,
      description: draft.description.trim() || null,
      program,
      args: draft.args.split("\n").map((arg) => arg.trim()).filter(Boolean),
      cwd: draft.cwd.trim() || null,
    };
    const next = draft.original ? tasks.map((existing) => (existing.id === draft.original ? task : existing)) : [...tasks, task];
    if (await save(next, draft.original ? `Saved "${name}"` : `Added "${name}"`)) setDraft(null);
  }

  async function remove(task: TaskDefinition) {
    if (!(await confirmAction({ title: `Delete "${task.name}"?`, description: "It's removed from .pipelineos/tasks.toml for everyone once you commit the change.", confirmLabel: "Delete task", destructive: true }))) return;
    await save(tasks.filter((existing) => existing.id !== task.id), `Deleted "${task.name}"`);
  }

  if (!available) {
    return <EmptyState icon={Workflow} title="Pipeline tasks run in the desktop app" description="Open Pipeline OS on your machine to define and run builds, cooks, renders, and exports." />;
  }

  const addMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild><Button size="sm"><Plus size={14} /> Add task <ChevronDown size={13} className="opacity-70" /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-72">
        <DropdownMenuLabel>Start from a template</DropdownMenuLabel>
        {(templates.data ?? []).map((template) => {
          const Icon = KIND_ICONS[template.kind];
          return <DropdownMenuItem key={template.id} onClick={() => addFromTemplate(template)}><Icon size={14} /> {template.name}</DropdownMenuItem>;
        })}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => addFromTemplate(null)}><Plus size={14} /> Blank task</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold">Pipeline tasks</h2>
          <p className="max-w-2xl text-sm text-muted-foreground">One-click builds, cooks, renders, exports, and tests. Tasks live in <code>.pipelineos/tasks.toml</code> so the whole team (and CI, and AI agents) run them the same way.</p>
        </div>
        {addMenu}
      </div>

      {tasks.length && !trusted ? (
        <div className="flex flex-wrap items-center gap-3 rounded-lg border border-warning/40 bg-warning/10 px-4 py-3 text-sm">
          <ShieldAlert size={18} className="shrink-0 text-warning" aria-hidden />
          <span className="min-w-0 flex-1">These tasks came from the project files. Review the exact commands before running them on this machine.</span>
          <Button size="sm" variant="outline" onClick={() => setReviewing(true)}>Review &amp; trust</Button>
        </div>
      ) : null}

      {tasksQuery.isLoading ? <p className="text-sm text-muted-foreground">Loading tasks…</p>
        : tasks.length === 0 ? (
          <Card><CardContent className="p-0">
            <EmptyState icon={Workflow} title="No pipeline tasks yet" description="Add a build, render, or export once and run it with one click from here, the command line, or an AI agent." action={addMenu} />
          </CardContent></Card>
        ) : (
          <div className="grid gap-3 md:grid-cols-2">
            {tasks.map((task) => {
              const Icon = KIND_ICONS[task.kind] ?? Workflow;
              const active = projectRuns.find((item) => item.taskId === task.id && item.status === "running");
              const last = projectRuns.find((item) => item.taskId === task.id && item.status !== "running");
              return (
                <Card key={task.id} className="transition-colors hover:border-primary/40"><CardContent className="space-y-3 p-4">
                  <div className="flex items-start gap-3">
                    <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-secondary text-primary"><Icon size={18} aria-hidden /></span>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2"><h3 className="truncate font-medium">{task.name}</h3><span className="shrink-0 rounded-full bg-secondary px-2 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">{KIND_LABELS[task.kind]}</span></div>
                      {task.description ? <p className="line-clamp-2 text-sm text-muted-foreground">{task.description}</p> : null}
                      <code className="mt-1 block truncate text-[11px] text-muted-foreground" title={[task.program, ...task.args].join(" ")}>{[task.program, ...task.args].join(" ")}</code>
                    </div>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-8 w-8" aria-label={`More actions for ${task.name}`}><MoreHorizontal size={16} /></Button></DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-56">
                        <DropdownMenuItem onClick={() => setDraft(toDraft(task, task.id))}><Pencil size={14} /> Edit</DropdownMenuItem>
                        <DropdownMenuItem onClick={() => setDraft(toDraft({ ...task, id: uniqueId(task.id, new Set(tasks.map((t) => t.id))), name: `${task.name} (copy)` }, null))}><Copy size={14} /> Duplicate</DropdownMenuItem>
                        <DropdownMenuItem onClick={() => { void navigator.clipboard?.writeText(cliRunCommand(projectPath, task.id)); toast.success("CLI command copied."); }}><Copy size={14} /> Copy CLI command</DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem variant="destructive" onClick={() => void remove(task)}><Trash2 size={14} /> Delete</DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </div>
                  {active ? <RunProgress run={active} /> : null}
                  <div className="flex items-center gap-2">
                    <Button size="sm" disabled={!!active} onClick={() => void run(task)}><Play size={13} /> {active ? "Running…" : "Run"}</Button>
                    {last ? <span className="truncate text-xs text-muted-foreground">Last run {last.status}{last.finishedAt ? ` · ${new Date(last.finishedAt).toLocaleString()}` : ""}</span> : <span className="text-xs text-muted-foreground">Not run yet</span>}
                  </div>
                </CardContent></Card>
              );
            })}
          </div>
        )}

      {projectRuns.length ? (
        <Card><CardContent className="p-4">
          <div className="mb-1 flex items-center justify-between"><h3 className="text-sm font-semibold">Run history</h3><span className="text-xs text-muted-foreground">Select a run to see its log</span></div>
          <div className="space-y-0.5">{projectRuns.slice(0, 12).map((item) => <RunRow key={item.id} run={item} showProject={false} />)}</div>
        </CardContent></Card>
      ) : null}

      <TaskEditor draft={draft} onChange={setDraft} onSubmit={() => void submitDraft()} />
      <TrustReview open={reviewing} projectPath={projectPath} tasks={tasks} fingerprint={tasksQuery.data?.fingerprint ?? null} onClose={() => setReviewing(false)} onTrusted={() => { setReviewing(false); void refresh(); }} />
    </div>
  );
}

function TaskEditor({ draft, onChange, onSubmit }: { draft: Draft | null; onChange: (draft: Draft | null) => void; onSubmit: () => void }) {
  const set = (patch: Partial<Draft>) => draft && onChange({ ...draft, ...patch });
  return (
    <Dialog open={!!draft} onOpenChange={(open) => { if (!open) onChange(null); }}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{draft?.original ? "Edit task" : "New task"}</DialogTitle>
          <DialogDescription>Tasks run a program with arguments directly, never through a shell, so paths with spaces are safe. Use <code>{"${engine}"}</code> for the project's engine at its pinned version, <code>{"${project}"}</code> for the project folder, and <code>{"${date}"}</code> for today.</DialogDescription>
        </DialogHeader>
        {draft ? (
          <form className="grid gap-3 sm:grid-cols-2" onSubmit={(event) => { event.preventDefault(); onSubmit(); }}>
            <label className="space-y-1 text-sm sm:col-span-2"><span className="font-medium">Name</span><Input autoFocus required value={draft.name} onChange={(event) => set({ name: event.target.value })} placeholder="Build Windows (Shipping)" /></label>
            <label className="space-y-1 text-sm"><span className="font-medium">Kind</span>
              <select value={draft.kind} onChange={(event) => set({ kind: event.target.value as TaskKind })} className="h-9 w-full rounded-md border border-input bg-transparent px-2 text-sm">
                {(Object.keys(KIND_LABELS) as TaskKind[]).map((kind) => <option key={kind} value={kind}>{KIND_LABELS[kind]}</option>)}
              </select>
            </label>
            <label className="space-y-1 text-sm"><span className="font-medium">Id</span><Input value={draft.id} onChange={(event) => set({ id: event.target.value })} placeholder="build-win64" /><span className="text-xs text-muted-foreground">Used by the CLI and AI agents.</span></label>
            <label className="space-y-1 text-sm sm:col-span-2"><span className="font-medium">Program</span><Input required value={draft.program} onChange={(event) => set({ program: event.target.value })} placeholder="${engine}  or  python  or  C:/Epic/UE_5.4/Engine/Build/BatchFiles/RunUAT.bat" className="font-mono" /></label>
            <label className="space-y-1 text-sm sm:col-span-2"><span className="font-medium">Arguments <span className="font-normal text-muted-foreground">(one per line)</span></span><Textarea value={draft.args} onChange={(event) => set({ args: event.target.value })} rows={6} className="font-mono text-xs" placeholder={"-batchmode\n-projectPath\n${project}"} /></label>
            <label className="space-y-1 text-sm"><span className="font-medium">Working folder</span><Input value={draft.cwd} onChange={(event) => set({ cwd: event.target.value })} placeholder="(project folder)" /></label>
            <label className="space-y-1 text-sm"><span className="font-medium">Description</span><Input value={draft.description} onChange={(event) => set({ description: event.target.value })} placeholder="What it produces and where" /></label>
            <DialogFooter className="sm:col-span-2">
              <Button type="button" variant="ghost" onClick={() => onChange(null)}>Cancel</Button>
              <Button type="submit">{draft.original ? "Save task" : "Add task"}</Button>
            </DialogFooter>
          </form>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function TrustReview({ open, projectPath, tasks, fingerprint, onClose, onTrusted }: { open: boolean; projectPath: string; tasks: TaskDefinition[]; fingerprint: string | null; onClose: () => void; onTrusted: () => void }) {
  const plans = useQuery({
    queryKey: ["task-plans", projectPath, fingerprint],
    queryFn: async () => Promise.all(tasks.map(async (task) => {
      try { return { task, line: await studioApi.planTask(projectPath, task.id), error: null as string | null }; }
      catch (error) { return { task, line: [task.program, ...task.args].join(" "), error: error instanceof Error ? error.message : String(error) }; }
    })),
    enabled: open && tasks.length > 0,
  });
  async function trust() {
    if (!fingerprint) return;
    try { await studioApi.trustTasks(projectPath, fingerprint); toast.success("Tasks trusted on this machine."); onTrusted(); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }
  return (
    <Dialog open={open} onOpenChange={(value) => { if (!value) onClose(); }}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2"><ShieldCheck size={18} className="text-primary" /> Review project tasks</DialogTitle>
          <DialogDescription>Running a task runs these exact commands on your machine. Only trust tasks from people you trust. If the tasks file changes (for example after a pull), Pipeline OS asks again.</DialogDescription>
        </DialogHeader>
        <div className="max-h-[50vh] space-y-2 overflow-y-auto">
          {(plans.data ?? tasks.map((task) => ({ task, line: [task.program, ...task.args].join(" "), error: null }))).map(({ task, line, error }) => (
            <div key={task.id} className="rounded-lg border border-border p-3">
              <div className="text-sm font-medium">{task.name}</div>
              <code className="mt-1 block whitespace-pre-wrap break-all text-xs text-muted-foreground">{line}</code>
              {error ? <p className="mt-1 text-xs text-warning">{error}</p> : null}
            </div>
          ))}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Not now</Button>
          <Button onClick={() => void trust()} disabled={!fingerprint}><ShieldCheck size={15} /> Trust and allow running</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// Project "Board" tab: a git-native Kanban. Each issue is a small TOML file in
// .pipelineos/issues, so the board syncs through version control, works
// offline, and is readable by the CLI and AI agents.
import { useMemo, useState, type DragEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { KanbanSquare, MoreHorizontal, Plus, Search, Trash2 } from "lucide-react";
import { KIND_META, PRIORITY_META, STATUS_COLUMNS } from "../lib/issue-meta";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { studioApi, studioAvailable, type Issue, type IssueKind, type IssuePriority, type IssueStatus } from "../bridge";
import { confirmAction } from "../lib/confirm";
import { EmptyState } from "./empty-state";
import { TrackerButton } from "./tracker-dialog";
import { ExternalLink } from "lucide-react";
import { openExternal } from "../bridge";

export function Avatar({ name }: { name?: string | null }) {
  if (!name) return null;
  let hue = 0;
  for (const char of name) hue = (hue * 31 + char.charCodeAt(0)) % 360;
  return <span title={name} className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[10px] font-semibold uppercase text-white" style={{ background: `oklch(0.55 0.12 ${hue})` }}>{name.slice(0, 2)}</span>;
}

export function ProjectBoard({ projectPath }: { projectPath: string }) {
  const available = studioAvailable();
  const queryClient = useQueryClient();
  const key = ["issues", projectPath];
  const issuesQuery = useQuery({ queryKey: key, queryFn: () => studioApi.issues(projectPath), enabled: available });
  const [editing, setEditing] = useState<Issue | null>(null);
  const [filter, setFilter] = useState("");
  const [kindFilter, setKindFilter] = useState<IssueKind | null>(null);
  const [assigneeFilter, setAssigneeFilter] = useState<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<IssueStatus | null>(null);
  const [adding, setAdding] = useState<IssueStatus | null>(null);
  const [newTitle, setNewTitle] = useState("");

  const issues = useMemo(() => issuesQuery.data?.issues ?? [], [issuesQuery.data]);
  const assignees = useMemo(() => [...new Set(issues.map((issue) => issue.assignee).filter((a): a is string => !!a))].sort(), [issues]);
  const visible = issues.filter((issue) =>
    (!filter || `${issue.title} ${issue.id} ${issue.body} ${(issue.labels ?? []).join(" ")}`.toLowerCase().includes(filter.toLowerCase()))
    && (!kindFilter || issue.kind === kindFilter)
    && (!assigneeFilter || issue.assignee === assigneeFilter));

  const invalidate = () => Promise.all([
    queryClient.invalidateQueries({ queryKey: key }),
    queryClient.invalidateQueries({ queryKey: ["issues-everywhere"] }),
  ]);

  async function move(issue: Issue, status: IssueStatus) {
    if (issue.status === status) return;
    // Optimistic: the card moves immediately; a failure snaps it back.
    queryClient.setQueryData(key, (data: typeof issuesQuery.data) => data && { ...data, issues: data.issues.map((item) => (item.id === issue.id ? { ...item, status } : item)) });
    try { await studioApi.updateIssue(projectPath, { ...issue, status }); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
    await invalidate();
  }

  async function quickAdd(status: IssueStatus) {
    const title = newTitle.trim();
    if (!title) { setAdding(null); return; }
    try {
      await studioApi.createIssue(projectPath, { title, status });
      setNewTitle("");
      await invalidate();
    } catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }

  async function saveEdit(issue: Issue) {
    if (!issue.title.trim()) { toast.error("An issue needs a title."); return; }
    try {
      if (issue.id) await studioApi.updateIssue(projectPath, issue);
      else await studioApi.createIssue(projectPath, { ...issue });
      setEditing(null);
      await invalidate();
    } catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }

  async function remove(issue: Issue) {
    if (!(await confirmAction({ title: `Delete "${issue.title}"?`, description: "The issue file is removed from .pipelineos/issues. Your version control history keeps a copy.", confirmLabel: "Delete issue", destructive: true }))) return;
    try { await studioApi.deleteIssue(projectPath, issue.id); setEditing(null); await invalidate(); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }

  function onDrop(event: DragEvent, status: IssueStatus) {
    event.preventDefault();
    setDropTarget(null);
    const id = event.dataTransfer.getData("text/plain") || dragging;
    const issue = issues.find((item) => item.id === id);
    setDragging(null);
    if (issue) void move(issue, status);
  }

  if (!available) return <EmptyState icon={KanbanSquare} title="The board lives in the desktop app" description="Open Pipeline OS on your machine to track tasks, bugs, and art requests per project." />;

  const blank = (status: IssueStatus): Issue => ({ id: "", title: "", status, priority: "none", kind: "task", assignee: null, labels: [], files: [], createdAt: "", updatedAt: "", body: "" });

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-48 flex-1">
          <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" aria-hidden />
          <Input aria-label="Filter issues" placeholder="Filter by title, id, label…" value={filter} onChange={(event) => setFilter(event.target.value)} className="h-8 pl-8" />
        </div>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="outline" size="sm">{kindFilter ? KIND_META[kindFilter].label : "All kinds"}</Button></DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuItem onClick={() => setKindFilter(null)}>All kinds</DropdownMenuItem>
            {(Object.keys(KIND_META) as IssueKind[]).map((kind) => { const Icon = KIND_META[kind].icon; return <DropdownMenuItem key={kind} onClick={() => setKindFilter(kind)}><Icon size={14} /> {KIND_META[kind].label}</DropdownMenuItem>; })}
          </DropdownMenuContent>
        </DropdownMenu>
        {assignees.length ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild><Button variant="outline" size="sm">{assigneeFilter ? `@${assigneeFilter}` : "Anyone"}</Button></DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuItem onClick={() => setAssigneeFilter(null)}>Anyone</DropdownMenuItem>
              {assignees.map((name) => <DropdownMenuItem key={name} onClick={() => setAssigneeFilter(name)}><Avatar name={name} /> {name}</DropdownMenuItem>)}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
        <TrackerButton projectPath={projectPath} />
        <Button size="sm" onClick={() => setEditing(blank("todo"))}><Plus size={14} /> New issue</Button>
      </div>
      {issuesQuery.data?.problems.length ? <p className="text-xs text-warning">{issuesQuery.data.problems.length} issue file(s) couldn't be read: {issuesQuery.data.problems[0]}</p> : null}

      <div className="grid gap-3 overflow-x-auto pb-2 [grid-template-columns:repeat(5,minmax(220px,1fr))]">
        {STATUS_COLUMNS.map((column) => {
          const items = visible.filter((issue) => issue.status === column.id);
          return (
            <section
              key={column.id}
              aria-label={`${column.label} column`}
              onDragOver={(event) => { event.preventDefault(); setDropTarget(column.id); }}
              onDragLeave={() => setDropTarget((current) => (current === column.id ? null : current))}
              onDrop={(event) => onDrop(event, column.id)}
              className={cn("flex min-h-64 flex-col rounded-xl border border-border bg-muted/20 p-2 transition-colors", dropTarget === column.id && "border-primary/60 bg-primary/5")}
            >
              <header className="mb-2 flex items-center justify-between px-1.5 pt-1">
                <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{column.label} <span className="ml-1 font-normal">{items.length}</span></span>
                <Button variant="ghost" size="icon" className="h-6 w-6" aria-label={`Add issue to ${column.label}`} onClick={() => { setAdding(column.id); setNewTitle(""); }}><Plus size={14} /></Button>
              </header>
              {adding === column.id ? (
                <form className="mb-2" onSubmit={(event) => { event.preventDefault(); void quickAdd(column.id); }}>
                  <Input autoFocus aria-label="New issue title" placeholder="Title, then Enter" value={newTitle} onChange={(event) => setNewTitle(event.target.value)} onBlur={() => { if (!newTitle.trim()) setAdding(null); }} onKeyDown={(event) => { if (event.key === "Escape") setAdding(null); }} className="h-8" />
                </form>
              ) : null}
              <div className="flex flex-1 flex-col gap-2">
                {items.map((issue) => {
                  const kind = KIND_META[issue.kind];
                  const priority = PRIORITY_META[issue.priority];
                  const KindIcon = kind.icon;
                  const PriorityIcon = priority.icon;
                  return (
                    <article
                      key={issue.id}
                      draggable
                      onDragStart={(event) => { event.dataTransfer.setData("text/plain", issue.id); event.dataTransfer.effectAllowed = "move"; setDragging(issue.id); }}
                      onDragEnd={() => { setDragging(null); setDropTarget(null); }}
                      className={cn("group cursor-grab rounded-lg border border-border bg-card p-2.5 text-sm shadow-xs transition-[opacity,border-color] hover:border-primary/40 active:cursor-grabbing", dragging === issue.id && "opacity-40")}
                    >
                      <div className="flex items-start gap-2">
                        <KindIcon size={14} className={cn("mt-0.5 shrink-0", kind.className)} aria-label={kind.label} />
                        <button className="min-w-0 flex-1 text-left font-medium leading-snug hover:underline" onClick={() => setEditing(issue)}>{issue.title}</button>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-6 w-6 shrink-0 opacity-60 group-hover:opacity-100" aria-label={`Actions for ${issue.title}`}><MoreHorizontal size={14} /></Button></DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            <DropdownMenuLabel>Move to</DropdownMenuLabel>
                            {STATUS_COLUMNS.filter((target) => target.id !== issue.status).map((target) => <DropdownMenuItem key={target.id} onClick={() => void move(issue, target.id)}>{target.label}</DropdownMenuItem>)}
                            <DropdownMenuSeparator />
                            {issue.external?.url ? <DropdownMenuItem onClick={() => void openExternal(issue.external!.url!)}><ExternalLink size={14} /> Open in {issue.external.system}</DropdownMenuItem> : null}
                            <DropdownMenuItem onClick={() => { void navigator.clipboard?.writeText(`#${issue.id}`); toast.success(`Copied #${issue.id}. Mention it in commit messages.`); }}>Copy reference</DropdownMenuItem>
                            <DropdownMenuItem variant="destructive" onClick={() => void remove(issue)}><Trash2 size={14} /> Delete</DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </div>
                      <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
                        {issue.external ? <span className="font-mono" title={`Synced from ${issue.external.system}`}>{issue.external.key}</span> : <span className="font-mono">#{issue.id}</span>}
                        {PriorityIcon ? <PriorityIcon size={13} className={priority.className} aria-label={`${priority.label} priority`} /> : null}
                        {(issue.labels ?? []).slice(0, 2).map((label) => <span key={label} className="rounded bg-secondary px-1.5 py-px">{label}</span>)}
                        <span className="ml-auto"><Avatar name={issue.assignee} /></span>
                      </div>
                    </article>
                  );
                })}
                {!items.length && adding !== column.id ? <p className="px-2 py-4 text-center text-xs text-muted-foreground">{dropTarget === column.id ? "Drop here" : "Nothing here"}</p> : null}
              </div>
            </section>
          );
        })}
      </div>
      <p className="text-xs text-muted-foreground">Issues are files in <code>.pipelineos/issues/</code>: commit them to share the board with your team. Reference them in commits as <code>#id</code>. AI agents can read and update the board through the Pipeline OS MCP server.</p>
      <IssueEditor issue={editing} onChange={setEditing} onSave={(issue) => void saveEdit(issue)} onDelete={(issue) => void remove(issue)} />
    </div>
  );
}

function IssueEditor({ issue, onChange, onSave, onDelete }: { issue: Issue | null; onChange: (issue: Issue | null) => void; onSave: (issue: Issue) => void; onDelete: (issue: Issue) => void }) {
  const [labelsText, setLabelsText] = useState("");
  const [lastId, setLastId] = useState<string | null>(null);
  if (issue && (issue.id || "new") !== lastId) {
    setLastId(issue.id || "new");
    setLabelsText((issue.labels ?? []).join(", "));
  }
  if (!issue && lastId !== null) setLastId(null);
  const set = (patch: Partial<Issue>) => issue && onChange({ ...issue, ...patch });
  const select = "h-9 w-full rounded-md border border-input bg-transparent px-2 text-sm";
  return (
    <Dialog open={!!issue} onOpenChange={(open) => { if (!open) onChange(null); }}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{issue?.id ? <span className="flex items-center gap-2"><span className="font-mono text-sm text-muted-foreground">#{issue.id}</span> Edit issue</span> : "New issue"}</DialogTitle>
          <DialogDescription>{issue?.id && issue.updatedAt ? `Updated ${new Date(issue.updatedAt).toLocaleString()}` : "Saved as a file in the project so it syncs with version control."}</DialogDescription>
        </DialogHeader>
        {issue ? (
          <form className="grid gap-3 sm:grid-cols-4" onSubmit={(event) => { event.preventDefault(); onSave({ ...issue, labels: labelsText.split(",").map((l) => l.trim()).filter(Boolean) }); }}>
            <label className="space-y-1 text-sm sm:col-span-4"><span className="font-medium">Title</span><Input autoFocus required value={issue.title} onChange={(event) => set({ title: event.target.value })} placeholder="What needs doing?" /></label>
            <label className="space-y-1 text-sm"><span className="font-medium">Status</span><select className={select} value={issue.status} onChange={(event) => set({ status: event.target.value as IssueStatus })}>{STATUS_COLUMNS.map((column) => <option key={column.id} value={column.id}>{column.label}</option>)}</select></label>
            <label className="space-y-1 text-sm"><span className="font-medium">Kind</span><select className={select} value={issue.kind} onChange={(event) => set({ kind: event.target.value as IssueKind })}>{(Object.keys(KIND_META) as IssueKind[]).map((kind) => <option key={kind} value={kind}>{KIND_META[kind].label}</option>)}</select></label>
            <label className="space-y-1 text-sm"><span className="font-medium">Priority</span><select className={select} value={issue.priority} onChange={(event) => set({ priority: event.target.value as IssuePriority })}>{(Object.keys(PRIORITY_META) as IssuePriority[]).map((priority) => <option key={priority} value={priority}>{PRIORITY_META[priority].label}</option>)}</select></label>
            <label className="space-y-1 text-sm"><span className="font-medium">Assignee</span><Input value={issue.assignee ?? ""} onChange={(event) => set({ assignee: event.target.value || null })} placeholder="name" /></label>
            <label className="space-y-1 text-sm sm:col-span-4"><span className="font-medium">Labels</span><Input value={labelsText} onChange={(event) => setLabelsText(event.target.value)} placeholder="level-2, lighting" /></label>
            <label className="space-y-1 text-sm sm:col-span-4"><span className="font-medium">Description <span className="font-normal text-muted-foreground">(Markdown)</span></span><Textarea rows={8} value={issue.body} onChange={(event) => set({ body: event.target.value })} placeholder={"Context, steps to reproduce, acceptance criteria, links to concept art…"} /></label>
            <DialogFooter className="sm:col-span-4 sm:justify-between">
              {issue.id ? <Button type="button" variant="ghost" className="text-destructive" onClick={() => onDelete(issue)}><Trash2 size={14} /> Delete</Button> : <span />}
              <span className="flex gap-2"><Button type="button" variant="ghost" onClick={() => onChange(null)}>Cancel</Button><Button type="submit">{issue.id ? "Save" : "Create issue"}</Button></span>
            </DialogFooter>
          </form>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

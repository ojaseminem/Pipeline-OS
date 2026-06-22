import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft, Box, CircleAlert, ExternalLink, FileCode2, FolderOpen, GitBranch,
  ListTodo, Notebook, Play, Plus, RefreshCw, Rocket, Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { toast } from "sonner";
import { desktopApi, isNativeRuntime, type HealthIssue } from "../bridge";
import { loadWorkspace, newId, saveWorkspace, type ProjectWorkspace } from "../lib/local-store";

function timeAgo(epochSeconds: number): string {
  if (!epochSeconds) return "";
  const diff = Math.max(0, Date.now() / 1000 - epochSeconds);
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export function ProjectDetail({ project, onBack }: { project: { path: string; name: string }; onBack: () => void }) {
  const native = isNativeRuntime();
  const queryClient = useQueryClient();
  const cfg = useQuery({ queryKey: ["project-config", project.path], queryFn: () => desktopApi.projectConfig(project.path), enabled: native, retry: false });
  const git = useQuery({ queryKey: ["git-status", project.path], queryFn: () => desktopApi.gitStatus(project.path), enabled: native, retry: false });
  const files = useQuery({ queryKey: ["recent-files", project.path], queryFn: () => desktopApi.recentFiles(project.path, 25), enabled: native });
  const [health, setHealth] = useState<HealthIssue[]>([]);
  const [ws, setWs] = useState<ProjectWorkspace>(() => loadWorkspace(project.path));
  const [todoText, setTodoText] = useState("");
  const [refLabel, setRefLabel] = useState("");
  const [refUrl, setRefUrl] = useState("");
  const [branch, setBranch] = useState("");
  const [commit, setCommit] = useState("");

  useEffect(() => saveWorkspace(project.path, ws), [project.path, ws]);

  async function run(label: string, action: () => Promise<unknown>) {
    try { await action(); toast.success(`${label} complete.`); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }
  const refreshGit = () => queryClient.invalidateQueries({ queryKey: ["git-status", project.path] });

  const profiles = cfg.data?.launch_profiles ?? [];
  const engine = cfg.data?.project_type ?? "project";

  return (
    <section className="space-y-5">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" aria-label="Back" onClick={onBack}><ArrowLeft size={18} /></Button>
        <span className="flex h-11 w-11 items-center justify-center rounded-xl bg-secondary text-primary"><Box size={24} /></span>
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-2xl font-semibold">{project.name}</h1>
          <p className="truncate text-sm text-muted-foreground">{project.path}</p>
        </div>
        <Badge variant="secondary" className="capitalize">{engine.replaceAll("-", " ")}</Badge>
        <Button variant="outline" onClick={() => native ? void run("Opening folder", () => desktopApi.openPath(project.path)) : undefined}><FolderOpen size={15} /> Open folder</Button>
        {profiles[0] ? <Button onClick={() => void run(`Opening ${project.name}`, () => desktopApi.launchProjectProfile(project.path, profiles[0].id))}><Rocket size={15} /> Open in {engine.replaceAll("-", " ")}</Button> : null}
      </div>

      <Tabs defaultValue="overview">
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="source">Source Control</TabsTrigger>
          <TabsTrigger value="notes">Notes &amp; To-dos</TabsTrigger>
          <TabsTrigger value="files">Recent Files</TabsTrigger>
          <TabsTrigger value="references">References</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="mt-4 grid gap-4 lg:grid-cols-2">
          <Card><CardContent className="space-y-3 p-5">
            <h2 className="text-base font-semibold">Launch</h2>
            {profiles.length ? <div className="flex flex-wrap gap-2">{profiles.map((profile) => <Button key={profile.id} variant="outline" size="sm" onClick={() => void run(`Launching ${profile.name}`, () => desktopApi.launchProjectProfile(project.path, profile.id))}><Play size={14} /> {profile.name}</Button>)}</div>
              : <p className="text-sm text-muted-foreground">No launch profiles defined in this project's <code>.vantadeck/project.toml</code> yet.</p>}
            {cfg.data?.linked_apps.length ? <div><h3 className="mb-1 mt-2 text-sm font-medium">Linked applications</h3><div className="flex flex-wrap gap-1">{cfg.data.linked_apps.map((linked) => <Badge key={linked.app_id} variant="outline">{linked.app_id}{linked.preferred_version ? ` ${linked.preferred_version}` : ""}</Badge>)}</div></div> : null}
          </CardContent></Card>
          <Card><CardContent className="space-y-3 p-5">
            <div className="flex items-center justify-between"><h2 className="text-base font-semibold">Health</h2><Button variant="outline" size="sm" disabled={!native} onClick={() => void run("Running health checks", async () => setHealth(await desktopApi.projectHealth(project.path)))}>Run checks</Button></div>
            {health.length ? <div className="space-y-2">{health.map((issue) => <div key={issue.code} className="flex items-start gap-2 text-sm"><CircleAlert size={16} className={issue.severity === "error" ? "shrink-0 text-destructive" : "shrink-0 text-primary"} /><span className="min-w-0"><strong className="block truncate">{issue.title}</strong><small className="line-clamp-2 text-muted-foreground">{issue.detail}</small></span></div>)}</div>
              : <p className="text-sm text-muted-foreground">Run checks to validate engine versions, launch profiles, and source control.</p>}
          </CardContent></Card>
        </TabsContent>

        <TabsContent value="source" className="mt-4 space-y-4">
          <Card><CardContent className="space-y-3 p-5">
            <div className="flex items-center justify-between">
              <h2 className="flex items-center gap-2 text-base font-semibold"><GitBranch size={17} /> {git.data?.branch ? `On ${git.data.branch}` : "Version control"}</h2>
              <Button variant="ghost" size="sm" disabled={!native} onClick={() => void refreshGit()}><RefreshCw size={14} /> Refresh</Button>
            </div>
            {git.isError ? <p className="text-sm text-muted-foreground">This project is not a Git repository (or Git is unavailable).</p> : (
              <>
                <p className="text-sm text-muted-foreground">{git.data ? `${git.data.changedFiles.length} changed file(s).` : "Reading status…"}</p>
                {git.data?.changedFiles.length ? <div className="max-h-40 overflow-y-auto rounded-lg border border-border text-sm">{git.data.changedFiles.slice(0, 50).map((file) => <div key={file.path} className="flex items-center justify-between border-b border-border px-3 py-1.5 last:border-0"><span className="truncate">{file.path}</span><Badge variant="outline">{file.status}</Badge></div>)}</div> : null}
                <div className="flex flex-wrap gap-2">
                  <Input aria-label="Branch" placeholder="branch" value={branch} onChange={(e) => setBranch(e.target.value)} className="w-40" />
                  <Button variant="outline" disabled={!native || !branch} onClick={() => { if (window.confirm(`Switch ${project.name} to ${branch}?`)) void run("Switching branch", async () => { await desktopApi.gitSwitch(project.path, branch, true); await refreshGit(); }); }}>Switch</Button>
                  <Button variant="outline" disabled={!native} onClick={() => { if (window.confirm(`Pull changes into ${project.name}?`)) void run("Syncing", async () => { await desktopApi.gitSync(project.path, true); await refreshGit(); }); }}>Sync</Button>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Input aria-label="Commit message" placeholder="Commit message" value={commit} onChange={(e) => setCommit(e.target.value)} className="flex-1 min-w-48" />
                  <Button variant="outline" disabled={!native || !commit} onClick={() => { if (window.confirm(`Commit all changes in ${project.name}?`)) void run("Committing", async () => { await desktopApi.gitCommit(project.path, commit, true); setCommit(""); await refreshGit(); }); }}>Commit</Button>
                  <Button variant="outline" disabled={!native} onClick={() => { if (window.confirm(`Push ${project.name} to its remote?`)) void run("Pushing", () => desktopApi.gitPush(project.path, true)); }}>Push</Button>
                </div>
              </>
            )}
          </CardContent></Card>
        </TabsContent>

        <TabsContent value="notes" className="mt-4 grid gap-4 lg:grid-cols-2">
          <Card><CardContent className="space-y-2 p-5">
            <h2 className="flex items-center gap-2 text-base font-semibold"><Notebook size={17} /> Notes</h2>
            <textarea value={ws.notes} onChange={(e) => setWs({ ...ws, notes: e.target.value })} placeholder="Project notes, decisions, reminders…" className="min-h-48 w-full resize-y rounded-md border border-border bg-secondary/40 p-3 text-sm outline-none focus:ring-2 focus:ring-ring" />
          </CardContent></Card>
          <Card><CardContent className="space-y-3 p-5">
            <h2 className="flex items-center gap-2 text-base font-semibold"><ListTodo size={17} /> To-dos</h2>
            <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); if (!todoText.trim()) return; setWs({ ...ws, todos: [...ws.todos, { id: newId(), text: todoText.trim(), done: false }] }); setTodoText(""); }}>
              <Input aria-label="New to-do" placeholder="Add a task…" value={todoText} onChange={(e) => setTodoText(e.target.value)} className="flex-1" />
              <Button type="submit" size="icon" aria-label="Add task"><Plus size={16} /></Button>
            </form>
            <div className="space-y-1">{ws.todos.length ? ws.todos.map((todo) => (
              <div key={todo.id} className="flex items-center gap-2 rounded-md px-1.5 py-1 hover:bg-muted/40">
                <input type="checkbox" checked={todo.done} onChange={() => setWs({ ...ws, todos: ws.todos.map((t) => t.id === todo.id ? { ...t, done: !t.done } : t) })} className="size-4 accent-[var(--primary)]" />
                <span className={`flex-1 text-sm ${todo.done ? "text-muted-foreground line-through" : ""}`}>{todo.text}</span>
                <Button variant="ghost" size="icon" aria-label="Delete task" onClick={() => setWs({ ...ws, todos: ws.todos.filter((t) => t.id !== todo.id) })}><Trash2 size={14} /></Button>
              </div>
            )) : <p className="text-sm text-muted-foreground">No tasks yet.</p>}</div>
          </CardContent></Card>
        </TabsContent>

        <TabsContent value="files" className="mt-4">
          <Card><CardContent className="p-5">
            <div className="mb-2 flex items-center justify-between"><h2 className="text-base font-semibold">Recently modified</h2><Button variant="ghost" size="sm" disabled={!native} onClick={() => void files.refetch()}><RefreshCw size={14} /> Refresh</Button></div>
            {files.data?.length ? <div className="divide-y divide-border rounded-lg border border-border">{files.data.map((file) => (
              <button key={file.path} onClick={() => void run("Opening file", () => desktopApi.openPath(file.path))} className="flex w-full items-center gap-3 px-3 py-2 text-left text-sm hover:bg-muted/40">
                <FileCode2 size={16} className="shrink-0 text-muted-foreground" />
                <span className="min-w-0 flex-1"><strong className="block truncate font-normal">{file.name}</strong><small className="block truncate text-xs text-muted-foreground">{file.path}</small></span>
                <span className="shrink-0 text-xs text-muted-foreground">{timeAgo(file.modified)}</span>
              </button>
            ))}</div> : <p className="text-sm text-muted-foreground">{native ? "No recent files found." : "Recent files are available in the desktop app."}</p>}
          </CardContent></Card>
        </TabsContent>

        <TabsContent value="references" className="mt-4">
          <Card><CardContent className="space-y-3 p-5">
            <h2 className="text-base font-semibold">References</h2>
            <form className="flex flex-wrap gap-2" onSubmit={(e) => { e.preventDefault(); if (!refUrl.trim()) return; setWs({ ...ws, references: [...ws.references, { id: newId(), label: refLabel.trim() || refUrl.trim(), url: refUrl.trim() }] }); setRefLabel(""); setRefUrl(""); }}>
              <Input aria-label="Reference label" placeholder="Label (e.g. Art bible)" value={refLabel} onChange={(e) => setRefLabel(e.target.value)} className="w-48" />
              <Input aria-label="Reference URL" placeholder="https://… or a file path" value={refUrl} onChange={(e) => setRefUrl(e.target.value)} className="flex-1 min-w-56" />
              <Button type="submit"><Plus size={15} /> Add</Button>
            </form>
            <div className="space-y-1">{ws.references.length ? ws.references.map((reference) => (
              <div key={reference.id} className="flex items-center gap-2 rounded-md px-1.5 py-1 hover:bg-muted/40">
                <ExternalLink size={15} className="shrink-0 text-muted-foreground" />
                <button className="min-w-0 flex-1 truncate text-left text-sm text-primary hover:underline" onClick={() => native ? void run("Opening reference", () => desktopApi.openPath(reference.url)) : window.open(reference.url, "_blank")}>{reference.label}</button>
                <Button variant="ghost" size="icon" aria-label="Remove reference" onClick={() => setWs({ ...ws, references: ws.references.filter((r) => r.id !== reference.id) })}><Trash2 size={14} /></Button>
              </div>
            )) : <p className="text-sm text-muted-foreground">No references yet. Add design docs, art bibles, tickets, or links.</p>}</div>
          </CardContent></Card>
        </TabsContent>
      </Tabs>
    </section>
  );
}

import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft, Box, Check, ChevronDown, ChevronRight, Download, ExternalLink, FileCode2, FolderOpen, GitBranch, GitBranchPlus,
  GitCommitHorizontal, ListTodo, MoreHorizontal, Notebook, Pencil, Play, Plus, RefreshCw, Rocket, Trash2, Upload,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { toast } from "sonner";
import { Copy, ImageIcon, Link2, Tag, X } from "lucide-react";
import { HealthPanel } from "./health-panel";
import { browsePath, desktopApi, isNativeRuntime, openExternal, type HealthIssue } from "../bridge";
import { formatLastOpened } from "../lib/format";
import { PROJECT_CATEGORIES } from "../lib/categories";
import { loadTags, loadWorkspace, newId, saveTags, saveWorkspace, type ProjectWorkspace } from "../lib/local-store";

// Maps a Git status to a friendly label + an explanatory tooltip. Codes come
// from `git status --porcelain=v2`: a two-char XY field where X is the staged
// (index) state and Y is the working-tree state ("." means unchanged).
function statusLabel(code: string): { label: string; title: string } {
  if (code === "untracked") {
    return { label: "Untracked", title: "Untracked — a new file Git isn't following yet. It won't be committed unless you include it." };
  }
  const names: Record<string, string> = { M: "Modified", A: "Added", D: "Deleted", R: "Renamed", C: "Copied", T: "Type changed", U: "Conflicted" };
  const staged = code.charAt(0) !== "." ? code.charAt(0) : "";
  const worktree = code.charAt(1) !== "." ? code.charAt(1) : "";
  const letter = worktree || staged || "M";
  const label = (names[letter] ?? "Changed") + (staged && !worktree ? " (staged)" : "");
  return { label, title: `Git status "${code}" — ${staged ? "staged: " + (names[staged] ?? staged) + "; " : ""}working tree: ${worktree ? names[worktree] ?? worktree : "unchanged"}.` };
}

function diffLineClass(line: string): string {
  if (line.startsWith("@@")) return "text-primary";
  if (line.startsWith("+") && !line.startsWith("+++")) return "text-[color:oklch(0.72_0.15_150)]";
  if (line.startsWith("-") && !line.startsWith("---")) return "text-destructive";
  return "text-muted-foreground";
}

const EMPTY_WORKSPACE: ProjectWorkspace = { notes: "", todos: [], references: [] };

function timeAgo(epochSeconds: number): string {
  if (!epochSeconds) return "";
  const diff = Math.max(0, Date.now() / 1000 - epochSeconds);
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

/// Per-app list of the project's files (recent-first) with a launch button that
/// opens each in the app (Maya opens with the project set).
function AppFiles({ projectPath, appId, appName, executable, native, run }: { projectPath: string; appId: string; appName: string; executable?: string; native: boolean; run: (label: string, action: () => Promise<unknown>) => Promise<void> }) {
  const files = useQuery({ queryKey: ["app-files", projectPath, appId], queryFn: () => desktopApi.appProjectFiles(projectPath, appId), enabled: native, retry: false });
  const [expanded, setExpanded] = useState(false);
  const list = files.data ?? [];
  if (!list.length) return null;
  const shown = expanded ? list : list.slice(0, 5);
  return (
    <div className="mt-2 space-y-1 border-t border-border pt-2">
      <div className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Files ({list.length})</div>
      {shown.map((file) => (
        <div key={file.path} className="flex items-center gap-2 text-sm">
          <FileCode2 size={13} className="shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate" title={file.path}>{file.name}</span>
          <span className="shrink-0 text-xs text-muted-foreground">{timeAgo(file.modified)}</span>
          <Button variant="ghost" size="sm" className="h-7 shrink-0 px-2" disabled={!native || !executable} onClick={() => { if (executable) void run(`Opening ${file.name}`, () => desktopApi.launchAppFile(appId, executable, file.path, projectPath)); }}><Play size={12} /> Open</Button>
        </div>
      ))}
      {list.length > 5 ? <button onClick={() => setExpanded(!expanded)} className="text-xs text-muted-foreground hover:text-foreground">{expanded ? "Show fewer" : `Show all ${list.length} ${appName} files`}</button> : null}
    </div>
  );
}

export function ProjectDetail({ project, onBack, onRenamed, onOpenInEngine }: { project: { path: string; name: string }; onBack: () => void; onRenamed?: (name: string) => void; onOpenInEngine?: (target: { path: string; name: string }) => void }) {
  const native = isNativeRuntime();
  const queryClient = useQueryClient();
  const cfg = useQuery({ queryKey: ["project-config", project.path], queryFn: () => desktopApi.projectConfig(project.path), enabled: native, retry: false });
  const apps = useQuery({ queryKey: ["apps"], queryFn: () => desktopApi.listApps(), enabled: native, retry: false });
  const git = useQuery({ queryKey: ["git-status", project.path], queryFn: () => desktopApi.gitStatus(project.path), enabled: native, retry: false });
  const files = useQuery({ queryKey: ["recent-files", project.path], queryFn: () => desktopApi.recentFiles(project.path, 25), enabled: native });
  const branches = useQuery({ queryKey: ["git-branches", project.path], queryFn: () => desktopApi.gitBranches(project.path), enabled: native, retry: false });
  const gitInstalled = useQuery({ queryKey: ["git-available"], queryFn: () => desktopApi.gitAvailable(), enabled: native && git.isError, retry: false });
  const [setupMode, setSetupMode] = useState<"local" | "online">("local");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [settingUp, setSettingUp] = useState(false);
  const [sourceView, setSourceView] = useState<"changes" | "history">("changes");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [marked, setMarked] = useState<Set<string>>(new Set());
  const anchorRef = useRef<number | null>(null);
  const [diffPath, setDiffPath] = useState<string | null>(null);
  const commits = useQuery({ queryKey: ["git-log", project.path], queryFn: () => desktopApi.gitLog(project.path, 30), enabled: native && sourceView === "history", retry: false });
  const diff = useQuery({ queryKey: ["git-diff", project.path, diffPath], queryFn: () => desktopApi.gitDiff(project.path, diffPath as string), enabled: native && !!diffPath, retry: false });
  const [expandedCommit, setExpandedCommit] = useState<string | null>(null);
  const commitFiles = useQuery({ queryKey: ["git-commit-files", project.path, expandedCommit], queryFn: () => desktopApi.gitCommitFiles(project.path, expandedCommit as string), enabled: native && !!expandedCommit, retry: false });
  const [historySelection, setHistorySelection] = useState<{ hash: string; path: string } | null>(null);
  const historyDiff = useQuery({
    queryKey: ["git-commit-diff", project.path, historySelection?.hash, historySelection?.path],
    queryFn: () => desktopApi.gitCommitDiff(project.path, historySelection!.hash, historySelection!.path),
    enabled: native && !!historySelection,
    retry: false,
  });
  const [pendingSwitch, setPendingSwitch] = useState<string | null>(null);
  const [switching, setSwitching] = useState(false);
  const stashes = useQuery({ queryKey: ["git-stash-list", project.path], queryFn: () => desktopApi.gitStashList(project.path), enabled: native, retry: false });
  const notifiedStashRef = useRef<string | null>(null);
  const [health, setHealth] = useState<HealthIssue[]>([]);
  const [healthCheckedAt, setHealthCheckedAt] = useState<string | null>(null);
  const [ws, setWs] = useState<ProjectWorkspace>(() => (native ? EMPTY_WORKSPACE : loadWorkspace(project.path)));
  const wsLoaded = useRef(false);
  const thumbnail = cfg.data?.thumbnail ?? null;
  const [thumbUrl, setThumbUrl] = useState<string | null>(null);
  const [name, setName] = useState(project.name);
  const [renaming, setRenaming] = useState(false);
  const [nameDraft, setNameDraft] = useState(project.name);
  const [tags, setTags] = useState<string[]>(() => (native ? [] : loadTags(project.path)));
  const [tagInput, setTagInput] = useState("");
  const [todoText, setTodoText] = useState("");
  const [refLabel, setRefLabel] = useState("");
  const [refUrl, setRefUrl] = useState("");
  const [commit, setCommit] = useState("");

  // Portable workspace (notes/to-dos/references) lives in .vantadeck/workspace.json
  // so it travels with the repo. Load on mount; in the web demo, fall back to
  // localStorage. Migrate any prior localStorage workspace into the file once.
  useEffect(() => {
    wsLoaded.current = false;
    if (!native) { setWs(loadWorkspace(project.path)); wsLoaded.current = true; return; }
    let active = true;
    desktopApi.readWorkspace(project.path).then((json) => {
      if (!active) return;
      if (json) {
        try { setWs({ ...EMPTY_WORKSPACE, ...(JSON.parse(json) as ProjectWorkspace) }); } catch { setWs(EMPTY_WORKSPACE); }
      } else {
        setWs(loadWorkspace(project.path)); // migrate localStorage → file on first save
      }
      wsLoaded.current = true;
    }).catch(() => { wsLoaded.current = true; });
    return () => { active = false; };
  }, [native, project.path]);

  // Persist workspace edits (debounced for the native file write).
  useEffect(() => {
    if (!wsLoaded.current) return;
    if (!native) { saveWorkspace(project.path, ws); return; }
    const timer = setTimeout(() => { void desktopApi.saveWorkspace(project.path, JSON.stringify(ws)).catch(() => undefined); }, 500);
    return () => clearTimeout(timer);
  }, [ws, native, project.path]);

  // Tags are portable (in project.toml); reflect the loaded config.
  useEffect(() => { if (native) setTags(cfg.data?.tags ?? []); }, [native, cfg.data?.tags]);

  function applyTags(next: string[]) {
    setTags(next);
    if (native) void run("Saving tags", async () => { await desktopApi.setProjectTags(project.path, next); await queryClient.invalidateQueries({ queryKey: ["project-config", project.path] }); onRenamed?.(name); });
    else saveTags(project.path, next);
  }
  useEffect(() => {
    let active = true;
    if (thumbnail && native) {
      desktopApi.readImage(`${project.path}/${thumbnail}`).then((url) => { if (active) setThumbUrl(url); }).catch(() => undefined);
    } else {
      setThumbUrl(null);
    }
    return () => { active = false; };
  }, [thumbnail, project.path, native]);

  // Show the last cached health result immediately so the panel is never blank.
  useEffect(() => {
    if (!native) return;
    let active = true;
    desktopApi.cachedHealth(project.path).then((cached) => {
      if (active && cached) { setHealth(cached.issues); setHealthCheckedAt(cached.checkedAt); }
    }).catch(() => undefined);
    return () => { active = false; };
  }, [native, project.path]);

  async function changeThumbnail() {
    const source = await browsePath({ directory: false, title: "Choose a thumbnail image" });
    if (!source) return;
    await run("Setting thumbnail", async () => {
      await desktopApi.setProjectThumbnail(project.path, source);
      await queryClient.invalidateQueries({ queryKey: ["project-config", project.path] });
    });
  }
  function clearThumbnail() {
    void run("Removing thumbnail", async () => {
      await desktopApi.clearProjectThumbnail(project.path);
      await queryClient.invalidateQueries({ queryKey: ["project-config", project.path] });
    });
  }
  function saveName() {
    const next = nameDraft.trim();
    if (!next || next === name) { setRenaming(false); return; }
    void run("Renaming project", async () => {
      await desktopApi.renameProject(project.path, next);
      setName(next);
      setRenaming(false);
      onRenamed?.(next);
    });
  }
  async function installGitNow() {
    setSettingUp(true);
    try { toast.message(await desktopApi.installGit()); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
    finally { setSettingUp(false); }
  }
  async function setupGit() {
    const remote = setupMode === "online" ? remoteUrl.trim() : null;
    if (setupMode === "online" && !remote) return;
    const message = remote
      ? `Initialize Git here, make an initial commit, add ${remote} as origin, and push? If sign-in is required it opens in your browser.`
      : "Initialize a local Git repository here and make an initial commit?";
    if (!window.confirm(message)) return;
    setSettingUp(true);
    try {
      await desktopApi.gitInitRepo(project.path, remote, true);
      toast.success("Git is set up.");
      await refreshGit();
    } catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
    finally { setSettingUp(false); }
  }
  function saveCategory(category: string | null) {
    void run("Saving category", async () => {
      await desktopApi.setProjectCategory(project.path, category);
      await queryClient.invalidateQueries({ queryKey: ["project-config", project.path] });
      onRenamed?.(name);
    });
  }
  function saveEngineVersion(appId: string, version: string) {
    void run("Saving engine version", async () => {
      await desktopApi.setProjectEngineVersion(project.path, appId, version);
      await queryClient.invalidateQueries({ queryKey: ["project-config", project.path] });
    });
  }
  function runHealth() {
    void run("Running health checks", async () => {
      const issues = await desktopApi.projectHealth(project.path);
      setHealth(issues);
      setHealthCheckedAt(new Date().toISOString());
    });
  }

  function addTag(value: string) {
    const tag = value.trim().toLowerCase();
    if (tag && !tags.includes(tag)) applyTags([...tags, tag]);
    setTagInput("");
  }

  useEffect(() => {
    const paths = git.data?.changedFiles.map((file) => file.path) ?? [];
    setSelected(new Set(paths));
    setMarked(new Set());
    anchorRef.current = null;
    setDiffPath((current) => (current && paths.includes(current) ? current : null));
  }, [git.data]);

  // Ctrl/⌘-click toggles a file, Shift-click selects a range, plain click selects
  // one and shows its diff — drives bulk actions like "Discard selected".
  function selectFile(index: number, path: string, event: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }) {
    const changed = git.data?.changedFiles ?? [];
    setHistorySelection(null);
    if (event.shiftKey && anchorRef.current != null) {
      const [from, to] = [anchorRef.current, index].sort((a, b) => a - b);
      setMarked(new Set(changed.slice(from, to + 1).map((file) => file.path)));
      setDiffPath(path);
    } else if (event.ctrlKey || event.metaKey) {
      setMarked((current) => { const next = new Set(current); if (next.has(path)) next.delete(path); else next.add(path); return next; });
      anchorRef.current = index;
    } else {
      setMarked(new Set([path]));
      setDiffPath(path);
      anchorRef.current = index;
    }
  }
  function selectHistoryFile(hash: string, path: string) {
    setDiffPath(null);
    setHistorySelection({ hash, path });
  }
  function discardMarked() {
    const paths = [...marked];
    if (!paths.length) return;
    const untracked = (git.data?.changedFiles ?? []).filter((file) => marked.has(file.path) && file.status === "untracked").length;
    const note = untracked ? ` ${untracked} untracked file(s) will be deleted from disk.` : "";
    if (!window.confirm(`Discard local changes to ${paths.length} file(s)?${note} This can't be undone.`)) return;
    void run("Discarding changes", async () => {
      for (const path of paths) await desktopApi.gitDiscard(project.path, path, true);
      setMarked(new Set());
      setDiffPath(null);
      await refreshGit();
    });
  }

  function toggleSelected(path: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path); else next.add(path);
      return next;
    });
  }
  function commitSelected() {
    if (!commit.trim() || selected.size === 0) return;
    if (window.confirm(`Commit ${selected.size} file(s) in ${project.name}?`)) {
      void run("Committing", async () => { await desktopApi.gitCommitPaths(project.path, commit, [...selected], true); setCommit(""); setDiffPath(null); await refreshGit(); });
    }
  }
  function discardFile(file: { path: string; status: string }) {
    const message = file.status === "untracked"
      ? `Delete the untracked file "${file.path}"? It will be removed from disk — this can't be undone.`
      : `Discard all local changes to "${file.path}"? It will be reverted to the last commit — this can't be undone.`;
    if (!window.confirm(message)) return;
    void run("Discarding changes", async () => { await desktopApi.gitDiscard(project.path, file.path, true); if (diffPath === file.path) setDiffPath(null); await refreshGit(); });
  }
  function fileActions(path: string) {
    const abs = `${project.path}/${path}`;
    return { abs, dir: abs.replace(/[\\/][^\\/]*$/, "") };
  }

  async function run(label: string, action: () => Promise<unknown>) {
    try { await action(); toast.success(`${label} complete.`); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }
  const refreshGit = () => Promise.all([
    queryClient.invalidateQueries({ queryKey: ["git-status", project.path] }),
    queryClient.invalidateQueries({ queryKey: ["git-branches", project.path] }),
    queryClient.invalidateQueries({ queryKey: ["git-stash-list", project.path] }),
  ]);
  const STASH_PREFIX = "vantadeck-autostash: from ";
  // Mirrors GitHub Desktop: switching with a clean tree is instant; a dirty
  // tree prompts to bring changes along or leave them stashed on this branch.
  function switchBranch(name: string) {
    if (name === git.data?.branch) return;
    const dirty = (git.data?.changedFiles.length ?? 0) > 0;
    if (!dirty) {
      void run(`Switching to ${name}`, async () => { await desktopApi.gitSwitch(project.path, name, true); await refreshGit(); });
      return;
    }
    setPendingSwitch(name);
  }
  async function resolveSwitch(strategy: "bring" | "leave") {
    const target = pendingSwitch;
    setPendingSwitch(null);
    if (!target) return;
    const from = git.data?.branch ?? "the current branch";
    setSwitching(true);
    await run(`Switching to ${target}`, async () => {
      await desktopApi.gitStashPush(project.path, `${STASH_PREFIX}${from}`, true);
      try {
        await desktopApi.gitSwitch(project.path, target, true);
      } catch (error) {
        await desktopApi.gitStashPop(project.path, true).catch(() => undefined);
        throw error;
      }
      if (strategy === "bring") {
        try { await desktopApi.gitStashPop(project.path, true); }
        catch { toast.error("Switched branches, but your changes couldn't be reapplied automatically — find them in the stash list and resolve manually."); }
      }
      await refreshGit();
    });
    setSwitching(false);
  }
  function newBranch() {
    const name = window.prompt("New branch name");
    if (name && name.trim()) {
      void run(`Creating ${name.trim()}`, async () => { await desktopApi.gitCreateBranch(project.path, name.trim(), true); await refreshGit(); });
    }
  }

  // If changes were left stashed here on a previous branch switch, offer to
  // restore them once (per branch landing) rather than leaving them buried in
  // the stash list.
  useEffect(() => {
    const branch = git.data?.branch;
    if (!branch || !stashes.data) return;
    const marker = `${STASH_PREFIX}${branch}`;
    const match = stashes.data.find((entry) => entry.includes(marker));
    if (!match || notifiedStashRef.current === `${branch}:${match}`) return;
    notifiedStashRef.current = `${branch}:${match}`;
    toast(`You left changes on "${branch}" during an earlier branch switch.`, {
      action: { label: "Restore", onClick: () => void run("Restoring changes", async () => { await desktopApi.gitStashPop(project.path, true); await refreshGit(); }) },
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [git.data?.branch, stashes.data]);

  const profiles = cfg.data?.launch_profiles ?? [];
  const engine = cfg.data?.project_type ?? "project";
  const managedApps = apps.data ?? [];
  const linkedApps = cfg.data?.linked_apps ?? [];
  const knownEngines = ["unity", "unreal-engine", "godot", "blender", "maya"];
  const engineAppId = linkedApps.map((app) => app.app_id).find((id) => knownEngines.includes(id)) ?? linkedApps[0]?.app_id;
  const engineName = managedApps.find((app) => app.id === engineAppId)?.name ?? engineAppId;
  const canOpenInEngine = Boolean(engineAppId);

  // "Project apps" only surfaces apps actually installed on this machine —
  // linked-but-uninstalled apps would just be clutter with nothing to open.
  const installedLinkedApps = linkedApps.map((linked) => {
    const managed = managedApps.find((app) => app.id === linked.app_id);
    const runnable = (managed?.installations.filter((item) => item.runnable) ?? []).slice().sort((left, right) => right.version.localeCompare(left.version, undefined, { numeric: true }));
    return { linked, managed, runnable };
  }).filter((entry) => entry.runnable.length > 0);
  // Apps installed on this machine but not yet linked — manual "Add app".
  const addableApps = managedApps.filter((app) => app.installations.some((item) => item.runnable) && !linkedApps.some((linked) => linked.app_id === app.id));
  const detectedApps = useQuery({ queryKey: ["detect-apps", project.path], queryFn: () => desktopApi.detectProjectApps(project.path), enabled: native, retry: false });
  // Auto-detected candidates (by file extension), narrowed to ones actually
  // installed — an uninstalled suggestion would have nothing to open with.
  const detectedSuggestions = (detectedApps.data ?? [])
    .map((id) => managedApps.find((app) => app.id === id))
    .filter((app): app is NonNullable<typeof app> => !!app && app.installations.some((item) => item.runnable));
  function addApp(appId: string) {
    void run("Adding app", async () => { await desktopApi.addLinkedApp(project.path, appId); await queryClient.invalidateQueries({ queryKey: ["project-config", project.path] }); await queryClient.invalidateQueries({ queryKey: ["detect-apps", project.path] }); });
  }
  function removeApp(appId: string) {
    if (!window.confirm("Unlink this app from the project? Its files won't be affected.")) return;
    void run("Unlinking app", async () => { await desktopApi.removeLinkedApp(project.path, appId); await queryClient.invalidateQueries({ queryKey: ["project-config", project.path] }); await queryClient.invalidateQueries({ queryKey: ["detect-apps", project.path] }); });
  }

  return (
    <section className="space-y-5">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" aria-label="Back" onClick={onBack}><ArrowLeft size={18} /></Button>
        <div className="group relative h-14 w-24 shrink-0 overflow-hidden rounded-lg border border-border bg-secondary">
          {thumbUrl ? <img src={thumbUrl} alt={`${project.name} thumbnail`} className="h-full w-full object-cover" /> : <span className="flex h-full w-full items-center justify-center text-primary"><Box size={24} /></span>}
          <button onClick={() => void changeThumbnail()} disabled={!native} aria-label="Change thumbnail" className="absolute inset-0 flex items-center justify-center bg-background/70 text-xs opacity-0 transition-opacity group-hover:opacity-100"><ImageIcon size={14} className="mr-1" /> Change</button>
          {thumbnail ? <button onClick={clearThumbnail} aria-label="Remove thumbnail" className="absolute right-0.5 top-0.5 rounded bg-background/80 p-0.5 text-muted-foreground opacity-0 hover:text-foreground group-hover:opacity-100"><X size={12} /></button> : null}
        </div>
        <div className="min-w-0 flex-1">
          {renaming ? (
            <form className="flex items-center gap-2" onSubmit={(event) => { event.preventDefault(); saveName(); }}>
              <Input autoFocus aria-label="Project name" value={nameDraft} onChange={(event) => setNameDraft(event.target.value)} onKeyDown={(event) => { if (event.key === "Escape") setRenaming(false); }} className="h-9 max-w-sm text-lg font-semibold" />
              <Button type="submit" size="sm"><Check size={15} /> Save</Button>
              <Button type="button" variant="ghost" size="sm" onClick={() => setRenaming(false)}>Cancel</Button>
            </form>
          ) : (
            <div className="flex items-center gap-1.5">
              <h1 className="truncate text-2xl font-semibold">{name}</h1>
              <Button variant="ghost" size="icon" aria-label="Rename project" disabled={!native} onClick={() => { setNameDraft(name); setRenaming(true); }}><Pencil size={15} /></Button>
            </div>
          )}
          <p className="truncate text-sm text-muted-foreground">{project.path}</p>
        </div>
        <Badge variant="secondary" className="capitalize">{engine.replaceAll("-", " ")}</Badge>
        <Button variant="outline" onClick={() => native ? void run("Opening folder", () => desktopApi.openPath(project.path)) : undefined}><FolderOpen size={15} /> Open folder</Button>
        <Button disabled={!native || !canOpenInEngine} title={canOpenInEngine ? undefined : "This project isn't connected to an engine."} onClick={() => onOpenInEngine?.({ path: project.path, name })}><Rocket size={15} /> Open in {engineName ?? "Engine"}</Button>
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
          </CardContent></Card>
          <Card><CardContent className="space-y-3 p-5">
            <div className="flex items-center justify-between gap-2">
              <div><h2 className="text-base font-semibold">Health</h2>{healthCheckedAt ? <p className="text-xs text-muted-foreground">Last checked {formatLastOpened(healthCheckedAt)}</p> : null}</div>
              <Button variant="outline" size="sm" disabled={!native} onClick={runHealth}><RefreshCw size={14} /> {healthCheckedAt ? "Re-check" : "Run checks"}</Button>
            </div>
            {health.length ? <HealthPanel projectPath={project.path} issues={health} />
              : healthCheckedAt ? <p className="text-sm text-muted-foreground">No health issues found. Engine versions, launch profiles, and source control all look good.</p>
              : <p className="text-sm text-muted-foreground">Run checks to validate engine versions, launch profiles, and source control. You can dismiss issues you don't care about and unhide them here later.</p>}
          </CardContent></Card>
          {installedLinkedApps.length || addableApps.length || detectedSuggestions.length ? <Card className="lg:col-span-2"><CardContent className="space-y-3 p-5">
            <div className="flex items-center justify-between gap-2">
              <div>
                <h2 className="text-base font-semibold">Project apps</h2>
                <p className="text-sm text-muted-foreground">Engines and creative tools linked to this project — each with its own project files. Choose the version to open in; it's saved to the project.</p>
              </div>
              {addableApps.length ? <DropdownMenu>
                <DropdownMenuTrigger asChild><Button variant="outline" size="sm" disabled={!native}><Plus size={14} /> Add app</Button></DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="max-h-72 w-56 overflow-y-auto">
                  {addableApps.map((app) => <DropdownMenuItem key={app.id} onClick={() => void addApp(app.id)}>{app.name}</DropdownMenuItem>)}
                </DropdownMenuContent>
              </DropdownMenu> : null}
            </div>
            {detectedSuggestions.length ? <div className="flex flex-wrap items-center gap-2 rounded-lg border border-dashed border-border bg-muted/30 px-3 py-2 text-sm">
              <span className="text-muted-foreground">Detected in this project's files:</span>
              {detectedSuggestions.map((app) => (
                <Button key={app.id} variant="secondary" size="sm" className="h-7" onClick={() => void addApp(app.id)}><Plus size={12} /> {app.name}</Button>
              ))}
            </div> : null}
            {installedLinkedApps.length ? <div className="grid gap-2 sm:grid-cols-2">{installedLinkedApps.map(({ linked, managed, runnable }) => {
              const selectedVersion = linked.preferred_version && runnable.some((item) => item.version === linked.preferred_version) ? linked.preferred_version : runnable[0]?.version ?? "";
              const selectedExe = runnable.find((item) => item.version === selectedVersion)?.executable ?? runnable[0]?.executable;
              return (
                <div key={linked.app_id} className="space-y-2 rounded-lg border border-border p-3">
                  <div className="flex items-center gap-2">
                    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-secondary text-primary"><Box size={16} /></span>
                    <span className="min-w-0 flex-1 truncate text-sm font-medium">{managed?.name ?? linked.app_id}</span>
                    <Button variant="ghost" size="icon" className="h-6 w-6 shrink-0" aria-label={`Unlink ${managed?.name ?? linked.app_id}`} title="Unlink from project" disabled={!native} onClick={() => void removeApp(linked.app_id)}><X size={13} /></Button>
                  </div>
                  <div className="flex items-center gap-1.5">
                    <select aria-label={`Version for ${managed?.name ?? linked.app_id}`} className="h-8 min-w-0 flex-1 rounded-md border border-border bg-secondary px-2 text-sm" value={selectedVersion} onChange={(event) => saveEngineVersion(linked.app_id, event.target.value)}>
                      {runnable.map((item) => <option key={item.executable} value={item.version}>{item.version}</option>)}
                    </select>
                    <Button variant="outline" size="sm" className="shrink-0" disabled={!native} onClick={() => void run(`Opening ${managed?.name ?? linked.app_id}`, () => desktopApi.openInEngine(project.path, linked.app_id))}><Play size={13} /> Open</Button>
                  </div>
                  <AppFiles projectPath={project.path} appId={linked.app_id} appName={managed?.name ?? linked.app_id} executable={selectedExe} native={native} run={run} />
                </div>
              );
            })}</div> : <p className="text-sm text-muted-foreground">No linked apps are installed on this machine yet.</p>}
          </CardContent></Card> : null}
          <Card className="lg:col-span-2"><CardContent className="space-y-3 p-5">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-base font-semibold">Category</h2>
              <select aria-label="Project category" disabled={!native} value={cfg.data?.category ?? ""} onChange={(event) => saveCategory(event.target.value || null)} className="h-8 rounded-md border border-border bg-secondary px-2 text-sm">
                <option value="">Auto (from apps)</option>
                {PROJECT_CATEGORIES.map((category) => <option key={category} value={category}>{category}</option>)}
              </select>
              <p className="text-xs text-muted-foreground">Sets the colored category tag and the Home category filter.</p>
            </div>
            <h2 className="flex items-center gap-2 text-base font-semibold"><Tag size={16} /> Tags</h2>
            <div className="flex flex-wrap items-center gap-2">
              {tags.map((tag) => <span key={tag} className="flex items-center gap-1 rounded-full bg-secondary px-2.5 py-1 text-xs">{tag}<button aria-label={`Remove tag ${tag}`} onClick={() => applyTags(tags.filter((value) => value !== tag))} className="text-muted-foreground hover:text-foreground"><X size={12} /></button></span>)}
              <form onSubmit={(event) => { event.preventDefault(); addTag(tagInput); }} className="flex">
                <Input aria-label="Add tag" placeholder="Add tag…" value={tagInput} onChange={(event) => setTagInput(event.target.value)} className="h-8 w-32" />
              </form>
            </div>
            <p className="text-xs text-muted-foreground">Group projects by client, game, or status. Filter by tag on the Projects screen.</p>
          </CardContent></Card>
        </TabsContent>

        <TabsContent value="source" className="mt-4">
          {git.isError ? (
            <Card><CardContent className="max-w-xl space-y-4 p-6">
              <div><h2 className="text-base font-semibold">Set up version control</h2><p className="mt-0.5 text-sm text-muted-foreground">This folder isn't a Git repository yet. Pipeline OS can initialize one so you can track changes, branch, and sync.</p></div>
              {gitInstalled.data === false ? (
                <div className="space-y-2">
                  <p className="text-sm">Git isn't installed on this machine.</p>
                  <Button disabled={!native || settingUp} onClick={installGitNow}><Download size={15} /> {settingUp ? "Installing…" : "Install Git"}</Button>
                  <p className="text-xs text-muted-foreground">Installs Git via winget — you may see a Windows permission prompt. Restart Pipeline OS afterward, then return here.</p>
                </div>
              ) : (
                <div className="space-y-3">
                  <div className="flex flex-wrap gap-2">
                    <button onClick={() => setSetupMode("local")} className={`rounded-lg border px-3 py-1.5 text-sm ${setupMode === "local" ? "border-primary bg-primary/10 text-foreground" : "border-border text-muted-foreground hover:text-foreground"}`}>Local repository</button>
                    <button onClick={() => setSetupMode("online")} className={`rounded-lg border px-3 py-1.5 text-sm ${setupMode === "online" ? "border-primary bg-primary/10 text-foreground" : "border-border text-muted-foreground hover:text-foreground"}`}>Connect to a remote</button>
                  </div>
                  {setupMode === "online" ? (
                    <label className="block text-sm">Remote URL<Input aria-label="Remote URL" placeholder="https://github.com/you/repo.git" value={remoteUrl} onChange={(event) => setRemoteUrl(event.target.value)} className="mt-1" /><span className="mt-1 block text-xs text-muted-foreground">Create the empty repository on your host first. If sign-in is needed it opens in your browser — Pipeline OS never sees your password.</span></label>
                  ) : (
                    <p className="text-xs text-muted-foreground">Creates a local repository with an initial commit and a starter .gitignore. You can add a remote later.</p>
                  )}
                  <Button disabled={!native || settingUp || (setupMode === "online" && !remoteUrl.trim())} onClick={setupGit}><GitBranchPlus size={15} /> {settingUp ? "Setting up…" : "Set up Git"}</Button>
                </div>
              )}
            </CardContent></Card>
          ) : (
            <div className="grid gap-4 lg:grid-cols-[1.5fr_1fr]">
              <Card><CardContent className="p-0">
                <div className="flex items-center justify-between border-b border-border px-3 py-2">
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button variant="ghost" size="sm" className="min-w-0 gap-1.5 font-semibold" disabled={!native}><GitBranch size={15} className="shrink-0" /><span className="truncate">{git.data?.branch ?? "—"}</span><ChevronDown size={13} className="shrink-0 text-muted-foreground" /></Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="start" className="max-h-80 w-60 overflow-y-auto">
                      {(branches.data ?? []).map((name) => (
                        <DropdownMenuItem key={name} onClick={() => switchBranch(name)}>
                          <Check size={14} className={name === git.data?.branch ? "opacity-100" : "opacity-0"} /> <span className="truncate">{name}</span>
                        </DropdownMenuItem>
                      ))}
                      {branches.data && branches.data.length ? <DropdownMenuSeparator /> : null}
                      <DropdownMenuItem onClick={newBranch}><GitBranchPlus size={14} /> New branch…</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button variant="ghost" size="sm" disabled={!native} onClick={() => { if (window.confirm(`Pull changes into ${project.name}?`)) void run("Pulling", async () => { await desktopApi.gitSync(project.path, true); await refreshGit(); }); }}><Download size={14} /> Pull{git.data?.behind ? ` ${git.data.behind}` : ""}</Button>
                    <Button variant="ghost" size="sm" disabled={!native} onClick={() => { if (window.confirm(`Push ${project.name} to its remote?`)) void run("Pushing", () => desktopApi.gitPush(project.path, true)); }}><Upload size={14} /> Push{git.data?.ahead ? ` ${git.data.ahead}` : ""}</Button>
                    <Button variant="ghost" size="icon" disabled={!native} aria-label="Refresh status" onClick={() => void refreshGit()}><RefreshCw size={14} /></Button>
                  </div>
                </div>
                <div className="flex items-center gap-3 border-t border-border px-4 py-1.5 text-sm">
                  <button onClick={() => setSourceView("changes")} className={sourceView === "changes" ? "font-medium" : "text-muted-foreground hover:text-foreground"}>Changes{git.data ? ` (${git.data.changedFiles.length})` : ""}</button>
                  <button onClick={() => setSourceView("history")} className={sourceView === "history" ? "font-medium" : "text-muted-foreground hover:text-foreground"}>History</button>
                </div>
                {sourceView === "changes" ? (
                  <>
                    {marked.size > 0 ? <div className="flex items-center justify-between gap-2 border-t border-border bg-muted/40 px-3 py-1.5 text-sm">
                      <span className="text-muted-foreground">{marked.size} selected</span>
                      <span className="flex items-center gap-1"><Button variant="ghost" size="sm" className="h-7" onClick={() => setMarked(new Set())}>Clear</Button><Button variant="ghost" size="sm" className="h-7 text-destructive hover:text-destructive" disabled={!native} onClick={discardMarked}><Trash2 size={14} /> Discard selected</Button></span>
                    </div> : null}
                    <div className="max-h-[300px] overflow-y-auto">
                      {git.data && git.data.changedFiles.length ? git.data.changedFiles.map((file, index) => {
                        const info = statusLabel(file.status);
                        return (
                          <div key={file.path} className={`flex items-center gap-2 border-t border-border px-3 py-1.5 text-sm ${marked.has(file.path) ? "bg-primary/10" : diffPath === file.path ? "bg-muted/50" : ""}`}>
                            <input type="checkbox" aria-label={`Include ${file.path}`} checked={selected.has(file.path)} onChange={() => toggleSelected(file.path)} className="size-4 accent-[var(--primary)]" />
                            <button className="flex min-w-0 flex-1 items-center gap-2 text-left" title="Click to view diff · Ctrl/⌘ or Shift-click to select multiple" onClick={(event) => selectFile(index, file.path, event)}><FileCode2 size={14} className="shrink-0 text-muted-foreground" /><span className="min-w-0 flex-1 truncate">{file.path}</span></button>
                            <Badge variant="outline" className="shrink-0 text-[10px]" title={info.title}>{info.label}</Badge>
                            <DropdownMenu>
                              <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-6 w-6 shrink-0" aria-label={`Actions for ${file.path}`}><MoreHorizontal size={15} /></Button></DropdownMenuTrigger>
                              <DropdownMenuContent align="end" className="w-56">
                                <DropdownMenuItem onClick={() => setDiffPath(file.path)}><FileCode2 size={14} /> View diff</DropdownMenuItem>
                                <DropdownMenuItem disabled={!native} onClick={() => void run("Opening file", () => desktopApi.openPath(fileActions(file.path).abs))}><ExternalLink size={14} /> Open file</DropdownMenuItem>
                                <DropdownMenuItem disabled={!native} onClick={() => { const dir = fileActions(file.path).dir; if (dir) void run("Opening folder", () => desktopApi.openPath(dir)); }}><FolderOpen size={14} /> Open containing folder</DropdownMenuItem>
                                <DropdownMenuItem onClick={() => { void navigator.clipboard?.writeText(file.path); toast.success("Path copied."); }}><Copy size={14} /> Copy relative path</DropdownMenuItem>
                                <DropdownMenuItem onClick={() => { void navigator.clipboard?.writeText(fileActions(file.path).abs); toast.success("Path copied."); }}><Copy size={14} /> Copy full path</DropdownMenuItem>
                                <DropdownMenuSeparator />
                                <DropdownMenuItem variant="destructive" disabled={!native} onClick={() => discardFile(file)}><Trash2 size={14} /> Discard changes</DropdownMenuItem>
                              </DropdownMenuContent>
                            </DropdownMenu>
                          </div>
                        );
                      }) : <div className="border-t border-border px-4 py-8 text-center text-sm text-muted-foreground">No local changes — working tree clean.</div>}
                    </div>
                    <div className="space-y-2 border-t border-border p-3">
                      <Input aria-label="Commit message" placeholder="Summary of changes" value={commit} onChange={(e) => setCommit(e.target.value)} />
                      <Button className="w-full" disabled={!native || !commit.trim() || selected.size === 0} onClick={commitSelected}><GitCommitHorizontal size={15} /> Commit {selected.size} to {git.data?.branch ?? "branch"}</Button>
                    </div>
                  </>
                ) : (
                  <div className="max-h-[420px] overflow-y-auto">
                    {commits.data && commits.data.length ? commits.data.map((entry) => {
                      const isOpen = expandedCommit === entry.hash;
                      return (
                        <div key={entry.hash} className="border-t border-border">
                          <button className="flex w-full items-start gap-2 px-3 py-2 text-left hover:bg-muted/40" onClick={() => setExpandedCommit(isOpen ? null : entry.hash)}>
                            {isOpen ? <ChevronDown size={15} className="mt-0.5 shrink-0 text-muted-foreground" /> : <ChevronRight size={15} className="mt-0.5 shrink-0 text-muted-foreground" />}
                            <span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium">{entry.subject}</span><span className="block text-xs text-muted-foreground">{entry.shortHash} · {entry.author} · {entry.date}</span></span>
                          </button>
                          {isOpen ? <div className="border-t border-border bg-muted/20 px-3 py-1.5">
                            {commitFiles.isLoading ? <p className="py-1 text-xs text-muted-foreground">Loading files…</p>
                              : commitFiles.data && commitFiles.data.length ? commitFiles.data.map((file) => (
                                <button key={file.path} onClick={() => selectHistoryFile(entry.hash, file.path)} className={`flex w-full items-center gap-2 rounded py-1 text-left text-sm hover:bg-muted/40 ${historySelection?.hash === entry.hash && historySelection?.path === file.path ? "bg-muted/50" : ""}`} title="Click to view this file's changes in the commit">
                                  <FileCode2 size={13} className="shrink-0 text-muted-foreground" />
                                  <span className="min-w-0 flex-1 truncate" title={file.path}>{file.path}</span>
                                  <Badge variant="outline" className="shrink-0 text-[10px] uppercase">{file.status}</Badge>
                                </button>
                              )) : <p className="py-1 text-xs text-muted-foreground">No file changes.</p>}
                          </div> : null}
                        </div>
                      );
                    }) : <div className="border-t border-border px-4 py-8 text-center text-sm text-muted-foreground">{commits.isLoading ? "Loading…" : "No commits yet."}</div>}
                  </div>
                )}
              </CardContent></Card>
              <Card><CardContent className="p-0">
                <div className="truncate border-b border-border px-4 py-2.5 text-sm font-semibold">{historySelection?.path ?? diffPath ?? "Diff"}</div>
                <div className="max-h-[480px] overflow-auto p-3">
                  {historySelection ? (historyDiff.data !== undefined ? <pre className="overflow-x-auto font-mono text-xs leading-relaxed">{(historyDiff.data || "").split("\n").map((line, index) => <div key={index} className={diffLineClass(line)}>{line || " "}</div>)}</pre> : <p className="text-sm text-muted-foreground">{historyDiff.isLoading ? "Loading diff…" : "No diff to show."}</p>)
                    : diffPath ? (diff.data !== undefined ? <pre className="overflow-x-auto font-mono text-xs leading-relaxed">{(diff.data || "").split("\n").map((line, index) => <div key={index} className={diffLineClass(line)}>{line || " "}</div>)}</pre> : <p className="text-sm text-muted-foreground">{diff.isLoading ? "Loading diff…" : "No diff to show."}</p>)
                    : <p className="text-sm text-muted-foreground">Select a file to view its changes.</p>}
                </div>
              </CardContent></Card>
            </div>
          )}
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
            <div><h2 className="text-base font-semibold">References</h2><p className="text-sm text-muted-foreground">Design docs, art bibles, tickets, or local files — one click to open.</p></div>
            <form className="flex flex-wrap items-end gap-2" onSubmit={(e) => { e.preventDefault(); if (!refUrl.trim()) return; setWs({ ...ws, references: [...ws.references, { id: newId(), label: refLabel.trim() || refUrl.trim(), url: refUrl.trim() }] }); setRefLabel(""); setRefUrl(""); }}>
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">Label<Input aria-label="Reference label" placeholder="e.g. Art bible" value={refLabel} onChange={(e) => setRefLabel(e.target.value)} className="w-48" /></label>
              <label className="flex flex-1 flex-col gap-1 text-xs text-muted-foreground">Link or file path<Input aria-label="Reference URL or path" required placeholder="https://…  or  D:/Docs/spec.pdf" value={refUrl} onChange={(e) => setRefUrl(e.target.value)} className="min-w-56" /></label>
              <Button type="submit"><Plus size={15} /> Add reference</Button>
            </form>
            <ul className="space-y-1.5">{ws.references.length ? ws.references.map((reference) => {
              const isLink = /^https?:\/\//i.test(reference.url);
              const open = () => { if (isLink) void openExternal(reference.url); else void run("Opening reference", () => desktopApi.openPath(reference.url)); };
              return (
                <li key={reference.id} className="flex items-center gap-2.5 rounded-lg border border-border p-2.5">
                  <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-secondary text-muted-foreground">{isLink ? <Link2 size={15} /> : <FileCode2 size={15} />}</span>
                  <button className="min-w-0 flex-1 text-left" onClick={open} aria-label={`Open ${reference.label}`}>
                    <span className="block truncate text-sm font-medium text-primary hover:underline">{reference.label}</span>
                    <span className="block truncate text-xs text-muted-foreground">{reference.url}</span>
                  </button>
                  <Button variant="ghost" size="icon" aria-label={`Open ${reference.label}`} title="Open" onClick={open}><ExternalLink size={15} /></Button>
                  <Button variant="ghost" size="icon" aria-label={`Copy link for ${reference.label}`} title="Copy" onClick={() => { void navigator.clipboard?.writeText(reference.url); toast.success("Copied to clipboard."); }}><Copy size={14} /></Button>
                  <Button variant="ghost" size="icon" aria-label={`Remove ${reference.label}`} title="Remove" onClick={() => setWs({ ...ws, references: ws.references.filter((r) => r.id !== reference.id) })}><Trash2 size={14} /></Button>
                </li>
              );
            }) : <li className="text-sm text-muted-foreground">No references yet. Add a link or a file path above.</li>}</ul>
          </CardContent></Card>
        </TabsContent>
      </Tabs>

      <Dialog open={!!pendingSwitch} onOpenChange={(open) => { if (!open) setPendingSwitch(null); }}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Switch to "{pendingSwitch}"?</DialogTitle>
            <DialogDescription>
              {project.name} has uncommitted changes on "{git.data?.branch}". Bring them along to "{pendingSwitch}", or leave them here — they'll be offered back next time you're on this branch.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="sm:justify-between">
            <Button variant="ghost" disabled={switching} onClick={() => setPendingSwitch(null)}>Cancel</Button>
            <div className="flex gap-2">
              <Button variant="outline" disabled={switching} onClick={() => void resolveSwitch("leave")}>Leave my changes on "{git.data?.branch}"</Button>
              <Button disabled={switching} onClick={() => void resolveSwitch("bring")}>Bring my changes to "{pendingSwitch}"</Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}

import { type FormEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  AppWindow,
  Box,
  ChevronRight,
  CircleAlert,
  Code2,
  Download,
  FileCode2,
  Folder,
  GitBranch,
  Home,
  Laptop,
  MoreHorizontal,
  RefreshCw,
  Search,
  Settings,
  Wrench,
} from "lucide-react";
import { QueryClientProvider } from "@tanstack/react-query";
import { Toaster } from "@/components/ui/sonner";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { TooltipProvider } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { installedApps as defaultApps, pinnedProjects as defaultPinned, recentProjects as defaultRecent, type Project } from "./data";
import { APP_CATEGORY_LABELS, desktopApi, formatVersion, isDemoMode, isNativeRuntime, loadDashboard, type HealthIssue, type UpdateInfo } from "./bridge";
import { createQueryClient, useApps, useInvalidate, useProjects, useTools } from "./lib/queries";
import { type ThemePreference, useTheme } from "./theme";
import { Onboarding, type OnboardingPrefs } from "./components/onboarding";
import { PathInput } from "./components/path-input";
import voidlineImage from "./assets/voidline-reactor.png";

type UndoEntry = { label: string; undo: () => Promise<void>; redo: () => Promise<void> };

const navigation = [
  ["Home", Home],
  ["Projects", Folder],
  ["Applications", Box],
  ["Health", Activity],
  ["Tools", Wrench],
  ["Settings", Settings],
] as const;

type Screen = (typeof navigation)[number][0];

const sampleContinueProject: Project = {
  name: "Voidline",
  path: "D:/Dev/Projects/Voidline",
  engine: "Unity",
  version: "2022.3.18f1",
  branch: "main",
  lastOpened: "Today, 10:42 AM",
};

const CATEGORY_ORDER = ["game-engine", "dcc", "art", "code", "version-control", "utility"];

const themeSelectClass =
  "h-8 rounded-md border border-border bg-secondary px-2 text-sm text-secondary-foreground focus:outline-none focus:ring-2 focus:ring-ring";

function AppIcon({ executable, size = 22 }: { executable?: string; size?: number }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    if (executable && isNativeRuntime()) {
      desktopApi.appIcon(executable).then((value) => { if (active) setSrc(value ?? null); }).catch(() => undefined);
    } else {
      setSrc(null);
    }
    return () => { active = false; };
  }, [executable]);
  return src
    ? <img className="rounded object-contain" width={size} height={size} src={src} alt="" />
    : <AppWindow size={size} className="text-muted-foreground" />;
}

function ProjectTable({ projects, onOpen }: { projects: Project[]; onOpen: (project: Project) => void }) {
  return (
    <div className="overflow-hidden rounded-xl border border-border" role="table" aria-label="Projects">
      <div className="grid grid-cols-[2fr_1fr_1.2fr_1fr_auto] gap-3 border-b border-border bg-muted/40 px-4 py-2 text-xs font-medium uppercase tracking-wide text-muted-foreground" role="row">
        <span>Project</span><span>Last opened</span><span>Engine / version</span><span>Branch</span><span>Actions</span>
      </div>
      {projects.length === 0 ? <EmptyState text="No projects in this view yet." /> : projects.map((project) => (
        <div className="grid grid-cols-[2fr_1fr_1.2fr_1fr_auto] items-center gap-3 border-b border-border px-4 py-3 text-sm last:border-0 hover:bg-muted/30" role="row" key={project.name}>
          <span className="flex items-center gap-3">
            <span className="flex h-9 w-9 items-center justify-center rounded-lg bg-secondary text-primary"><Box size={18} /></span>
            <span className="flex flex-col"><strong className="font-medium">{project.name}</strong><small className="text-xs text-muted-foreground">{project.path}</small></span>
          </span>
          <span className="text-muted-foreground">{project.lastOpened}</span>
          <span className="flex items-center gap-1.5 text-muted-foreground"><Box size={14} /> {project.engine} {project.version}</span>
          <span className="flex items-center gap-1.5 text-muted-foreground"><GitBranch size={13} /> {project.branch}</span>
          <span className="flex items-center gap-1.5">
            <Button variant="outline" size="sm" onClick={() => onOpen(project)}><Folder size={14} /> Open Project</Button>
            <Button variant="ghost" size="icon" aria-label={`More actions for ${project.name}`} onClick={() => onOpen(project)}><MoreHorizontal size={17} /></Button>
          </span>
        </div>
      ))}
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return <div className="flex items-center gap-2 px-4 py-6 text-sm text-muted-foreground"><CircleAlert size={18} /><span>{text}</span></div>;
}

function SectionLabel({ children }: { children: ReactNode }) {
  return <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">{children}</div>;
}

function Panel({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <Card><CardContent className="space-y-3 p-5">
      <div><h2 className="text-base font-semibold">{title}</h2>{description ? <p className="mt-0.5 text-sm text-muted-foreground">{description}</p> : null}</div>
      {children}
    </CardContent></Card>
  );
}

function AppShell() {
  const [activeScreen, setActiveScreen] = useState<Screen>("Home");
  const [projectView, setProjectView] = useState<"pinned" | "recent">("pinned");
  const [query, setQuery] = useState("");
  const [continueProject, setContinueProject] = useState<Project | null>(isDemoMode() ? sampleContinueProject : null);
  const [pinnedProjects, setPinnedProjects] = useState(isDemoMode() ? defaultPinned : []);
  const [recentProjects, setRecentProjects] = useState(isDemoMode() ? defaultRecent : []);
  const [installedApps, setInstalledApps] = useState<Array<{ id: string; name: string; executable?: string | null; versions: string[] }>>(
    isDemoMode() ? defaultApps.map((app) => ({ id: app.name.toLowerCase().replaceAll(" ", "-"), ...app })) : [],
  );
  const [health, setHealth] = useState<HealthIssue[]>([]);
  const projectsQuery = useProjects(activeScreen === "Projects");
  const appsQuery = useApps(activeScreen === "Applications");
  const toolsQuery = useTools(activeScreen === "Tools");
  const registeredProjects = projectsQuery.data ?? [];
  const managedApps = appsQuery.data ?? [];
  const tools = toolsQuery.data ?? [];
  const invalidate = useInvalidate();
  const [rootInput, setRootInput] = useState("");
  const [nameInput, setNameInput] = useState("");
  const [profileInput, setProfileInput] = useState("editor");
  const [branchInput, setBranchInput] = useState("main");
  const [commitInput, setCommitInput] = useState("");
  const [scanRoots, setScanRoots] = useState("");
  const [overrideApp, setOverrideApp] = useState("unity");
  const [overrideVersion, setOverrideVersion] = useState("");
  const [overrideExecutable, setOverrideExecutable] = useState("");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [scanning, setScanning] = useState(false);
  const [onboarding, setOnboarding] = useState(false);
  const undoStack = useRef<UndoEntry[]>([]);
  const redoStack = useRef<UndoEntry[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const { preference, setPreference } = useTheme();
  const projects = projectView === "pinned" ? pinnedProjects : recentProjects;
  const filteredProjects = useMemo(
    () => projects.filter((project) => project.name.toLowerCase().includes(query.toLowerCase())),
    [projects, query],
  );

  useEffect(() => {
    void loadDashboard().then((snapshot) => {
      setContinueProject(snapshot.continueProject);
      setPinnedProjects(snapshot.pinnedProjects);
      setRecentProjects(snapshot.recentProjects);
      setInstalledApps(snapshot.apps);
      setHealth(snapshot.health);
    });
  }, []);

  useEffect(() => {
    if (!isNativeRuntime()) return;
    desktopApi.checkForUpdate().then((info) => { if (info.available) setUpdate(info); }).catch(() => undefined);
    if (localStorage.getItem("vantadeck.onboarded") !== "true") setOnboarding(true);
  }, []);

  async function undoLast() {
    const entry = undoStack.current.pop();
    if (!entry) return;
    await entry.undo();
    redoStack.current.push(entry);
    toast.message(`Undone: ${entry.label}`, { action: { label: "Redo", onClick: () => void redoLast() } });
  }
  async function redoLast() {
    const entry = redoStack.current.pop();
    if (!entry) return;
    await entry.redo();
    undoStack.current.push(entry);
    toast.message(`Redone: ${entry.label}`);
  }
  async function performUndoable(label: string, redo: () => Promise<void>, undo: () => Promise<void>) {
    try {
      await redo();
      undoStack.current.push({ label, undo, redo });
      redoStack.current = [];
      toast.success(label, { action: { label: "Undo", onClick: () => void undoLast() } });
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    }
  }

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const meta = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();
      if (meta && key === "k") { event.preventDefault(); searchRef.current?.focus(); }
      else if (meta && key === "z" && !event.shiftKey) { event.preventDefault(); void undoLast(); }
      else if (meta && (key === "y" || (key === "z" && event.shiftKey))) { event.preventDefault(); void redoLast(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function run(label: string, action: () => Promise<unknown>) {
    try { await action(); toast.success(`${label} complete.`); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }

  async function scanWith(rootsValue: string) {
    if (scanning) return;
    setScanning(true);
    try {
      await run("Scanning applications", async () => {
        await desktopApi.scanApps(rootsValue.split(";").map((value) => value.trim()).filter(Boolean));
        await invalidate.apps();
      });
    } finally {
      setScanning(false);
    }
  }
  const runScan = () => scanWith(scanRoots);

  function completeOnboarding(prefs: OnboardingPrefs) {
    localStorage.setItem("vantadeck.onboarded", "true");
    localStorage.setItem("vantadeck.profile", JSON.stringify(prefs));
    setPreference(prefs.theme);
    setScanRoots(prefs.scanRoots);
    setOnboarding(false);
    setActiveScreen("Applications");
    if (isNativeRuntime()) void scanWith(prefs.scanRoots);
  }
  function skipOnboarding() {
    localStorage.setItem("vantadeck.onboarded", "true");
    setOnboarding(false);
  }

  function openScreen(screen: Screen) {
    setActiveScreen(screen);
  }

  function launchInstalled(app: { id: string; name: string; executable?: string | null }) {
    if (app.executable) void run(`Launching ${app.name}`, () => desktopApi.launchApp(app.id, app.executable!));
    else openScreen("Applications");
  }

  async function submitImport(event: FormEvent) {
    event.preventDefault();
    if (!window.confirm(`Import ${rootInput} and create its local Vantadeck metadata?`)) return;
    await run("Importing project", async () => { await desktopApi.importProject(rootInput, nameInput); await invalidate.projects(); });
  }

  const runtimeLabel = isNativeRuntime() ? "Native desktop" : isDemoMode() ? "Browser demo" : "Browser preview";

  const managementContent = (
    <section className="space-y-5">
      <div className="flex items-center justify-between">
        <div><SectionLabel>Workspace management</SectionLabel><h1 className="text-2xl font-semibold">{activeScreen}</h1></div>
        <Badge variant="secondary">{runtimeLabel}</Badge>
      </div>

      {activeScreen === "Projects" ? <div className="space-y-4">
        <form onSubmit={submitImport}><Panel title="Import a local project" description="Creates a .vantadeck/project.toml file and registers the project in local storage.">
          <div className="flex flex-wrap items-start gap-2">
            <PathInput ariaLabel="Project path" required directory placeholder="D:/Projects/MyGame" value={rootInput} onChange={setRootInput} />
            <Input aria-label="Project name" placeholder="Optional display name" value={nameInput} onChange={(e) => setNameInput(e.target.value)} className="flex-1 min-w-48" />
            <Button type="submit">Import project</Button>
          </div>
        </Panel></form>
        <Panel title="Launch a project profile" description="Profiles come from the portable project file; executable selection remains machine-local.">
          <div className="flex flex-wrap items-start gap-2">
            <PathInput ariaLabel="Launch project path" directory placeholder="Project root" value={rootInput} onChange={setRootInput} />
            <Input aria-label="Launch profile ID" placeholder="editor" value={profileInput} onChange={(e) => setProfileInput(e.target.value)} className="w-40" />
            <Button onClick={() => void run("Launching profile", () => desktopApi.launchProjectProfile(rootInput, profileInput))}>Launch profile</Button>
          </div>
        </Panel>
        <Panel title="Git workflow" description="Status is read-only. Sync, commit, push, and branch switching always ask for confirmation.">
          <div className="flex flex-wrap gap-2">
            <Input aria-label="Git branch" placeholder="Branch" value={branchInput} onChange={(e) => setBranchInput(e.target.value)} className="w-40" />
            <Input aria-label="Git commit message" placeholder="Commit message" value={commitInput} onChange={(e) => setCommitInput(e.target.value)} className="flex-1 min-w-48" />
            <Button variant="outline" onClick={() => { if (window.confirm(`Switch ${rootInput} to ${branchInput}?`)) void run("Switching branch", () => desktopApi.gitSwitch(rootInput, branchInput, true)); }}>Switch</Button>
            <Button variant="outline" onClick={() => { if (window.confirm(`Pull changes into ${rootInput}?`)) void run("Syncing repository", () => desktopApi.gitSync(rootInput, true)); }}>Sync</Button>
            <Button variant="outline" onClick={() => { if (window.confirm(`Commit all changes in ${rootInput}?`)) void run("Committing changes", () => desktopApi.gitCommit(rootInput, commitInput, true)); }}>Commit</Button>
            <Button variant="outline" onClick={() => { if (window.confirm(`Push ${rootInput} to its configured remote?`)) void run("Pushing repository", () => desktopApi.gitPush(rootInput, true)); }}>Push</Button>
          </div>
        </Panel>
        <div className="grid gap-3 sm:grid-cols-2">{registeredProjects.length ? registeredProjects.map((project) => (
          <Card key={project.path}><CardContent className="flex items-start gap-3 p-4">
            <Folder className="mt-0.5 text-primary" />
            <div className="min-w-0 flex-1"><h3 className="truncate font-medium">{project.name}</h3><p className="truncate text-sm text-muted-foreground">{project.path}</p>
              <div className="mt-3 flex gap-2">
                <Button variant="outline" size="sm" onClick={() => void performUndoable(project.pinned ? "Unpinned project" : "Pinned project", async () => { await desktopApi.pinProject(project.path, !project.pinned); await invalidate.projects(); }, async () => { await desktopApi.pinProject(project.path, project.pinned); await invalidate.projects(); })}>{project.pinned ? "Unpin" : "Pin"}</Button>
                <Button variant="outline" size="sm" onClick={() => void run("Reading Git status", async () => { const result = await desktopApi.gitStatus(project.path); toast.message(`Branch ${result.branch ?? "detached"}`, { description: `${result.changedFiles.length} changed files.` }); })}>Git status</Button>
              </div>
            </div>
          </CardContent></Card>
        )) : <EmptyState text="No durable projects registered yet." />}</div>
      </div> : null}

      {activeScreen === "Applications" ? <div className="space-y-5">
        <Panel title="Detect installed applications" description="Leave roots blank to auto-scan standard install folders on every drive, or provide semicolon-separated roots. Detection uses bundled, auditable manifests.">
          <div className="flex flex-wrap items-start gap-2">
            <PathInput ariaLabel="Scan roots" directory multi disabled={scanning} placeholder="Blank = all drives, or e.g. D:/Tools; E:/Apps" value={scanRoots} onChange={setScanRoots} />
            <Button disabled={scanning} aria-busy={scanning} onClick={() => void runScan()}>{scanning ? <><RefreshCw size={15} className="animate-spin" /> Scanning…</> : "Scan now"}</Button>
          </div>
          {scanning ? <p className="flex items-center gap-2 text-sm text-muted-foreground"><RefreshCw size={14} className="animate-spin" /> Scanning installed applications across your drives — this can take a moment.</p> : null}
        </Panel>
        {CATEGORY_ORDER.filter((category) => managedApps.some((app) => app.category === category)).map((category) => (
          <section key={category} className="space-y-2">
            <SectionLabel>{APP_CATEGORY_LABELS[category] ?? category}</SectionLabel>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{managedApps.filter((app) => app.category === category).map((app) => {
              const iconInstall = app.installations.find((item) => item.runnable) ?? app.installations[0];
              const launchTarget = app.installations.find((item) => item.runnable);
              const hasIncompatible = app.installations.some((item) => !item.runnable);
              return (
                <Card key={app.id}><CardContent className="flex items-start gap-3 p-4">
                  <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-secondary"><AppIcon executable={iconInstall?.executable} size={26} /></span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center justify-between gap-2"><h3 className="truncate font-medium">{app.name}</h3>{launchTarget ? <Button variant="outline" size="sm" onClick={() => { if (window.confirm(`Launch ${app.name} ${formatVersion(launchTarget.version)}?`)) void run(`Launching ${app.name}`, () => desktopApi.launchApp(app.id, launchTarget.executable)); }}>Launch</Button> : null}</div>
                    <div className="mt-1.5 flex flex-wrap gap-1">{app.installations.length ? app.installations.map((item) => <Badge key={item.executable} variant={item.runnable ? "secondary" : "outline"} className={item.runnable ? "" : "text-muted-foreground line-through"}>{formatVersion(item.version)}</Badge>) : <span className="text-sm text-muted-foreground">Not detected</span>}</div>
                    {!app.launchable && app.installations.length ? <p className="mt-1.5 text-xs text-primary">Detected — used for project version control</p> : null}
                    {app.launchable && hasIncompatible ? <p className="mt-1.5 text-xs text-muted-foreground">Some versions are built for another architecture and are skipped on launch.</p> : null}
                  </div>
                </CardContent></Card>
              );
            })}</div>
          </section>
        ))}
        {managedApps.length && managedApps.every((app) => app.installations.length === 0) ? <EmptyState text="No applications detected yet. Click Scan now to discover installed creative tools across your drives." /> : null}
        <form onSubmit={(event) => { event.preventDefault(); if (window.confirm(`Set a manual executable override for ${overrideApp} ${overrideVersion}?`)) void run("Saving override", async () => { await desktopApi.setManualOverride(overrideApp, overrideVersion, overrideExecutable); await invalidate.apps(); }); }}>
          <Panel title="Manual override">
            <div className="flex flex-wrap items-start gap-2">
              <Input aria-label="Application ID" value={overrideApp} onChange={(e) => setOverrideApp(e.target.value)} className="w-40" />
              <Input aria-label="Version" required placeholder="2022.3.18" value={overrideVersion} onChange={(e) => setOverrideVersion(e.target.value)} className="w-40" />
              <PathInput ariaLabel="Executable path" required directory={false} placeholder="C:/Tools/app.exe" value={overrideExecutable} onChange={setOverrideExecutable} />
              <Button type="submit">Save override</Button>
            </div>
          </Panel>
        </form>
      </div> : null}

      {activeScreen === "Health" ? <div className="space-y-4">
        <Panel title="Run project diagnostics">
          <div className="flex flex-wrap items-start gap-2"><PathInput ariaLabel="Health project path" directory placeholder="Project root" value={rootInput} onChange={setRootInput} /><Button onClick={() => void run("Running health checks", async () => setHealth(await desktopApi.projectHealth(rootInput)))}>Run checks</Button></div>
        </Panel>
        <div className="grid gap-3 sm:grid-cols-2">{health.length ? health.map((issue) => (
          <Card key={issue.code} className={cn("border-l-4", issue.severity === "error" ? "border-l-destructive" : "border-l-primary")}><CardContent className="flex items-start gap-3 p-4">
            <CircleAlert className={issue.severity === "error" ? "text-destructive" : "text-primary"} />
            <div><h3 className="font-medium">{issue.title}</h3><p className="text-sm text-muted-foreground">{issue.detail}</p><small className="text-xs text-muted-foreground">{issue.code}</small></div>
          </CardContent></Card>
        )) : <EmptyState text="No health issues reported. Run checks for a registered project." />}</div>
      </div> : null}

      {activeScreen === "Tools" ? <div className="space-y-4">
        <Panel title="Curated Tools Hub" description="Vantadeck shows validated, locally cached community metadata. It never executes downloaded installers or scripts automatically.">
          {!isNativeRuntime() ? <p className="text-sm text-muted-foreground">Open the native desktop app to read your local cache. Network access remains independently opt-in.</p> : null}
        </Panel>
        <div className="grid gap-3 sm:grid-cols-2">{tools.length ? tools.filter((tool) => tool.reviewState !== "withdrawn").map((tool) => (
          <Card key={tool.id}><CardContent className="flex items-start gap-3 p-4">
            <Wrench className="text-primary" />
            <div className="min-w-0 flex-1"><h3 className="font-medium">{tool.name}</h3><p className="text-sm text-muted-foreground">{tool.description}</p><small className="text-xs text-muted-foreground">{tool.reviewState} · {tool.license} · checked {tool.lastVerifiedAt}</small></div>
            <Button variant="outline" size="sm" onClick={() => toast.message("Reviewed source", { description: `${tool.sourceUrl} — open this in your browser.` })}>Source</Button>
          </CardContent></Card>
        )) : <EmptyState text="No validated tool index is cached. Use the CLI cache command after reviewing an index source." />}</div>
      </div> : null}

      {activeScreen === "Settings" ? <div className="grid gap-4 sm:grid-cols-2">
        <Panel title="Appearance"><label className="flex items-center gap-3 text-sm">Theme<select aria-label="Settings theme" className={themeSelectClass} value={preference} onChange={(event) => setPreference(event.target.value as ThemePreference)}><option value="system">System</option><option value="dark">Dark</option><option value="light">Light</option></select></label></Panel>
        <Panel title="Updates">
          {update?.available ? <p className="text-sm">Version <strong>{update.version}</strong> is available (you have {update.currentVersion}).{update.notes ? <><br /><span className="text-muted-foreground">{update.notes}</span></> : null}</p> : <p className="text-sm text-muted-foreground">{update ? `You are on the latest version (${update.currentVersion}).` : "Check for a newer signed release. Updates are verified against Vantadeck's signing key before installation."}</p>}
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => void run("Checking for updates", async () => { const info = await desktopApi.checkForUpdate(); setUpdate(info); toast.message(info.available ? `Update ${info.version} is available.` : "You are on the latest version."); })}><RefreshCw size={15} /> Check for updates</Button>
            {update?.available ? <Button onClick={() => { if (window.confirm(`Download and install version ${update.version} now? Vantadeck will restart.`)) void run("Installing update", () => desktopApi.installUpdate()); }}><Download size={15} /> Install &amp; restart</Button> : null}
          </div>
        </Panel>
        <Panel title="Runtime"><p className="text-sm text-muted-foreground">{runtimeLabel}. Network operations are disabled by default; all indexed data is stored locally.</p></Panel>
      </div> : null}
    </section>
  );

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      <aside className="flex w-64 flex-none flex-col gap-4 border-r border-sidebar-border bg-sidebar p-4">
        <div className="px-2 py-1"><strong className="block text-lg font-bold tracking-tight text-primary">VANTADECK</strong><span className="text-[10px] uppercase tracking-widest text-muted-foreground">Local-first creative launcher</span></div>
        <nav aria-label="Primary navigation" className="flex flex-col gap-1">
          {navigation.map(([label, Icon]) => (
            <button key={label} onClick={() => openScreen(label)} className={cn("flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors", activeScreen === label ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium" : "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground")}>
              <Icon size={19} /><span>{label}</span>{label === "Health" && health.length ? <Badge variant="secondary" className="ml-auto">{health.length}</Badge> : null}
            </button>
          ))}
        </nav>
        <div className="mt-auto flex items-center gap-2.5 rounded-lg bg-secondary/60 p-3">
          <Laptop size={18} className="text-primary" /><span className="flex flex-col text-xs"><span className="text-muted-foreground">System status</span><strong>Offline &amp; Local</strong><em className="not-italic text-muted-foreground">All systems operational</em></span>
        </div>
        <div className="flex items-center justify-between px-2 text-xs text-muted-foreground"><span>v0.1.0</span><span className="flex items-center gap-1"><Code2 size={13} /> Open Source</span></div>
      </aside>

      <main className="flex flex-1 flex-col overflow-hidden">
        <header className="flex items-center gap-4 border-b border-border px-6 py-3">
          <label className="flex flex-1 items-center gap-2 rounded-lg border border-border bg-secondary/50 px-3"><Search size={18} className="text-muted-foreground" /><input ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search projects, apps, tools, docs..." className="h-9 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground" /><kbd className="rounded border border-border px-1.5 text-[10px] text-muted-foreground">Ctrl K</kbd></label>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground"><Laptop size={15} /> Offline • Local mode</div>
        </header>

        {update?.available ? <div className="flex items-center gap-2 border-b border-border bg-primary/10 px-6 py-2 text-sm"><Download size={15} className="text-primary" /><span>Vantadeck {update.version} is available.</span><Button variant="link" size="sm" className="ml-auto h-auto p-0" onClick={() => openScreen("Settings")}>View update</Button></div> : null}

        <div className="flex-1 overflow-y-auto p-6">
          {activeScreen === "Home" ? <div className="space-y-6">
            <section aria-labelledby="continue-title">
              <SectionLabel><span id="continue-title">Continue Project</span></SectionLabel>
              {continueProject ? <Card className="overflow-hidden"><CardContent className="grid gap-5 p-0 lg:grid-cols-[260px_1fr_280px]">
                <img src={voidlineImage} alt="Voidline reactor environment" className="h-full max-h-64 w-full object-cover" />
                <div className="space-y-3 py-5"><h1 className="text-2xl font-semibold">{continueProject.name}</h1><p className="text-sm text-muted-foreground">{continueProject.path}</p>
                  <div className="flex flex-wrap gap-3 text-sm text-muted-foreground"><span className="flex items-center gap-1.5"><Box size={14} /> {continueProject.engine} {continueProject.version}</span><span className="flex items-center gap-1.5"><GitBranch size={13} /> {continueProject.branch}</span><span>Last opened: {continueProject.lastOpened}</span></div>
                  <ul className="space-y-1.5 text-sm text-muted-foreground"><li className="flex items-center gap-2"><Folder size={14} /> Project metadata and activity stay on this machine.</li><li className="flex items-center gap-2"><FileCode2 size={14} /> Portable settings live in .vantadeck/project.toml.</li></ul>
                </div>
                <div className="space-y-3 border-l border-border p-5"><Button className="w-full" onClick={() => openScreen("Projects")}>Open Project</Button>
                  <div><h2 className="mb-2 text-sm font-semibold">Health summary</h2>{health.length ? <div className="space-y-2">{health.slice(0, 3).map((issue) => <div key={issue.code} className="flex items-start gap-2 text-sm"><CircleAlert className={cn("shrink-0", issue.severity === "error" ? "text-destructive" : "text-primary")} size={16} /><span className="min-w-0" title={issue.detail}><strong className="block truncate">{issue.title}</strong><small className="line-clamp-2 text-muted-foreground">{issue.detail}</small></span></div>)}</div> : <EmptyState text="No current health issues." />}</div>
                </div>
              </CardContent></Card> : <Card><CardContent className="p-6"><EmptyState text="Import a project to start working locally." /></CardContent></Card>}
            </section>

            <section className="grid gap-5 lg:grid-cols-[1fr_320px]">
              <div className="space-y-3">
                <div className="flex gap-2" role="tablist">
                  <button role="tab" aria-selected={projectView === "pinned"} onClick={() => setProjectView("pinned")} className={cn("rounded-lg px-3 py-1.5 text-sm", projectView === "pinned" ? "bg-secondary font-medium" : "text-muted-foreground hover:text-foreground")}>Pinned Projects</button>
                  <button role="tab" aria-selected={projectView === "recent"} onClick={() => setProjectView("recent")} className={cn("rounded-lg px-3 py-1.5 text-sm", projectView === "recent" ? "bg-secondary font-medium" : "text-muted-foreground hover:text-foreground")}>Recent Projects</button>
                </div>
                <ProjectTable projects={filteredProjects} onOpen={() => openScreen("Projects")} />
                <Button variant="link" className="px-0" onClick={() => openScreen("Projects")}>View all projects <ChevronRight size={16} /></Button>
              </div>
              <aside className="space-y-4">
                <Card><CardContent className="p-4"><div className="mb-2 flex items-center justify-between text-sm font-semibold">Project Health <Button variant="ghost" size="sm" className="h-auto p-0 text-muted-foreground" onClick={() => openScreen("Health")}>View All ({health.length})</Button></div>{health.length ? <div className="space-y-2">{health.slice(0, 3).map((issue) => <div key={issue.code} className="flex items-start gap-2 text-sm"><CircleAlert className={cn("shrink-0", issue.severity === "error" ? "text-destructive" : "text-primary")} size={16} /><span className="min-w-0" title={issue.detail}><strong className="block truncate">{issue.title}</strong><small className="line-clamp-2 text-muted-foreground">{issue.detail}</small></span></div>)}</div> : <EmptyState text="No current health issues." />}</CardContent></Card>
                <Card><CardContent className="p-4"><div className="mb-2 flex items-center justify-between text-sm font-semibold">Installed Apps <Button variant="ghost" size="sm" className="h-auto p-0 text-muted-foreground" onClick={() => openScreen("Applications")}>Manage</Button></div><div className="space-y-1">{installedApps.length ? installedApps.map((app) => <button key={app.name} onClick={() => launchInstalled(app)} title={app.executable ? `Launch ${app.name}` : "Open Applications"} className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-muted/50"><span className="flex h-8 w-8 items-center justify-center rounded-md bg-secondary"><AppIcon executable={app.executable ?? undefined} size={18} /></span><span className="min-w-0 flex-1"><strong className="block truncate">{app.name}</strong><small className="block truncate text-xs text-muted-foreground">{app.versions.join(", ")}</small></span><ChevronRight size={15} className="text-muted-foreground" /></button>) : <EmptyState text="No apps detected yet." />}</div></CardContent></Card>
              </aside>
            </section>
          </div> : managementContent}
        </div>

        <Onboarding open={onboarding} onComplete={completeOnboarding} onSkip={skipOnboarding} />
        <footer className="flex items-center gap-2 border-t border-border px-6 py-2.5">
          <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Quick Launch</span>
          {installedApps.slice(0, 6).map((app) => <Button key={app.id} variant="ghost" size="sm" title={app.executable ? `Launch ${app.name}` : "Open Applications"} onClick={() => launchInstalled(app)}><AppIcon executable={app.executable ?? undefined} size={18} /> {app.name}</Button>)}
          {installedApps.length === 0 ? <Button variant="ghost" size="sm" onClick={() => openScreen("Applications")}><Search size={18} /> Detect applications</Button> : null}
          <label className="ml-auto flex items-center gap-2 text-xs text-muted-foreground"><Settings size={15} /> Theme<select aria-label="Theme" className={themeSelectClass} value={preference} onChange={(event) => setPreference(event.target.value as ThemePreference)}><option value="system">System</option><option value="dark">Dark</option><option value="light">Light</option></select></label>
        </footer>
      </main>
    </div>
  );
}

const queryClient = createQueryClient();

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <AppShell />
        <Toaster richColors position="bottom-right" />
      </TooltipProvider>
    </QueryClientProvider>
  );
}

import { type FormEvent, type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  AppWindow,
  Box,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  ArrowLeft,
  ArrowRight,
  Clipboard,
  Code2,
  Download,
  FileCode2,
  Folder,
  FolderOpen,
  GitBranch,
  Home,
  Laptop,
  ExternalLink,
  MoreHorizontal,
  Plus,
  Rocket,
  RefreshCw,
  Search,
  Settings,
  Star,
  Trash2,
  Wrench,
} from "lucide-react";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
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
import { APP_CATEGORY_LABELS, browsePath, desktopApi, formatVersion, isDemoMode, isNativeRuntime, loadDashboard, onScanProgress, openExternal, type ActivityRecord, type EngineVersionOption, type HealthSummary, type ProjectHealthOverview, type RecentFile, type ScanProgress, type ToolManifest, type UpdateInfo } from "./bridge";
import { formatLastOpened, timeAgo } from "./lib/format";
import { ProjectThumb } from "./components/thumbnail";
import { Progress } from "@/components/ui/progress";
import { createQueryClient, useApps, useInvalidate, useProjects, useTools } from "./lib/queries";
import { type ThemePreference, useTheme } from "./theme";
import { BrandLockup } from "./components/brand";
import { Onboarding, type OnboardingPrefs } from "./components/onboarding";
import { PathInput } from "./components/path-input";
import { ProjectDetail } from "./components/project-detail";
import { HealthScreen } from "./components/health-screen";
import { loadCustomApps, loadQuickLaunch, loadToolSources, newId, saveCustomApps, saveQuickLaunch, saveToolSources, type CustomApp, type QuickLaunchEntry, type ToolSource } from "./lib/local-store";
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

type Screen = (typeof navigation)[number][0] | "Project";

const sampleContinueProject: Project = {
  name: "Voidline",
  path: "D:/Dev/Projects/Voidline",
  engine: "Unity",
  version: "2022.3.18f1",
  branch: "main",
  lastOpened: "Today, 10:42 AM",
};

const CATEGORY_ORDER = ["game-engine", "dcc", "art", "code", "version-control", "utility"];


const SEARCH_SCOPES = ["all", "projects", "apps", "tools", "health"] as const;
const SCOPE_LABELS: Record<string, string> = { all: "All", projects: "Projects", apps: "Apps", tools: "Tools", health: "Health" };

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

type ProjectActions = {
  onOpen: (project: Project) => void;
  onLaunch: (project: Project) => void;
  onOpenFolder: (project: Project) => void;
  onCopyPath: (project: Project) => void;
};

function ProjectTable({ projects, actions }: { projects: Project[]; actions: ProjectActions }) {
  return (
    <div className="overflow-hidden rounded-xl border border-border" role="table" aria-label="Projects">
      <div className="grid grid-cols-[2fr_1fr_1.3fr_0.9fr_190px] gap-3 border-b border-border bg-muted/40 px-4 py-2 text-xs font-medium uppercase tracking-wide text-muted-foreground" role="row">
        <span>Project</span><span>Last opened</span><span>Engine / version</span><span>Branch</span><span>Actions</span>
      </div>
      {projects.length === 0 ? <EmptyState text="No projects in this view yet." /> : projects.map((project) => (
        <div className="grid grid-cols-[2fr_1fr_1.3fr_0.9fr_190px] items-center gap-3 border-b border-border px-4 py-3 text-sm last:border-0 hover:bg-muted/30" role="row" key={project.name}>
          <span className="flex items-center gap-3">
            <ProjectThumb projectPath={project.path} thumbnail={project.thumbnail} className="h-9 w-14" alt={`${project.name} thumbnail`} />
            <span className="flex flex-col"><strong className="font-medium">{project.name}</strong><small className="text-xs text-muted-foreground">{project.path}</small></span>
          </span>
          <span className="text-muted-foreground">{formatLastOpened(project.lastOpened) || "—"}</span>
          <span className="flex items-center gap-1.5 text-muted-foreground"><AppIcon executable={project.engineExecutable ?? undefined} size={16} /> {project.engine}{project.version ? ` ${project.version}` : ""}</span>
          <span className="flex items-center gap-1.5 text-muted-foreground">{project.branch ? <><GitBranch size={13} /> {project.branch}</> : "—"}</span>
          <span className="flex items-center gap-1.5">
            <Button variant="outline" size="sm" onClick={() => actions.onOpen(project)}><Folder size={14} /> Open Project</Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon" aria-label={`More actions for ${project.name}`}><MoreHorizontal size={17} /></Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-52">
                <DropdownMenuItem onClick={() => actions.onOpen(project)}><Folder size={14} /> Open project</DropdownMenuItem>
                <DropdownMenuItem disabled={!project.engineId} onClick={() => actions.onLaunch(project)}><Rocket size={14} /> Open in Engine</DropdownMenuItem>
                <DropdownMenuItem onClick={() => actions.onOpen(project)}><GitBranch size={14} /> Source control</DropdownMenuItem>
                <DropdownMenuItem onClick={() => actions.onOpenFolder(project)}><FolderOpen size={14} /> Open folder</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => actions.onCopyPath(project)}><Clipboard size={14} /> Copy project path</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
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
  const [searchScope, setSearchScope] = useState<"all" | "projects" | "apps" | "tools" | "health">("all");
  const [searchOpen, setSearchOpen] = useState(false);
  const [continueProject, setContinueProject] = useState<Project | null>(isDemoMode() ? sampleContinueProject : null);
  const [continueFiles, setContinueFiles] = useState<RecentFile[]>([]);
  const [enginePicker, setEnginePicker] = useState<{ path: string; name: string; appId: string; options: EngineVersionOption[] } | null>(null);
  const [activity, setActivity] = useState<ActivityRecord[]>([]);
  const [overrideApp, setOverrideApp] = useState<{ id: string; name: string } | null>(null);
  const [overrideVersion, setOverrideVersion] = useState("");
  const [overrideExe, setOverrideExe] = useState("");
  const [pinnedProjects, setPinnedProjects] = useState(isDemoMode() ? defaultPinned : []);
  const [recentProjects, setRecentProjects] = useState(isDemoMode() ? defaultRecent : []);
  const [installedApps, setInstalledApps] = useState<Array<{ id: string; name: string; executable?: string | null; versions: string[] }>>(
    isDemoMode() ? defaultApps.map((app) => ({ id: app.name.toLowerCase().replaceAll(" ", "-"), ...app })) : [],
  );
  const [healthProjects, setHealthProjects] = useState<ProjectHealthOverview[]>([]);
  // Flattened issues across projects (for the continue-card summary and search).
  const health = useMemo<HealthSummary[]>(
    () => healthProjects.flatMap((project) => project.issues.map((issue) => ({ code: issue.code, title: issue.title, detail: issue.detail, severity: issue.severity, project: project.name }))),
    [healthProjects],
  );
  const projectsQuery = useProjects(activeScreen === "Projects" || activeScreen === "Health");
  const appsQuery = useApps(activeScreen === "Applications");
  const toolsQuery = useTools(activeScreen === "Tools");
  const registeredProjects = useMemo(() => projectsQuery.data ?? [], [projectsQuery.data]);
  const managedApps = useMemo(() => appsQuery.data ?? [], [appsQuery.data]);
  const tools = useMemo(() => toolsQuery.data ?? [], [toolsQuery.data]);
  const invalidate = useInvalidate();
  const [rootInput, setRootInput] = useState("");
  const [nameInput, setNameInput] = useState("");
  const [scanRoots, setScanRoots] = useState("");
  const [customApps, setCustomApps] = useState<CustomApp[]>(() => loadCustomApps());
  const [customName, setCustomName] = useState("");
  const [customExe, setCustomExe] = useState("");
  const [customCategory, setCustomCategory] = useState("dcc");
  const [quickLaunch, setQuickLaunch] = useState<QuickLaunchEntry[]>(() => loadQuickLaunch());
  const pinnedIds = useMemo(() => new Set(quickLaunch.map((entry) => entry.id)), [quickLaunch]);
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  const [toolSources, setToolSources] = useState<ToolSource[]>(() => loadToolSources());
  const [gitUrl, setGitUrl] = useState("");
  const [localTools, setLocalTools] = useState<ToolManifest[]>([]);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [autoUpdate, setAutoUpdate] = useState(() => localStorage.getItem("vantadeck.autoUpdate") !== "false");
  const [appVersion, setAppVersion] = useState("0.2.0");
  const [autostart, setAutostart] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [onboarding, setOnboarding] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [selectedProject, setSelectedProject] = useState<{ path: string; name: string } | null>(null);
  const undoStack = useRef<UndoEntry[]>([]);
  const redoStack = useRef<UndoEntry[]>([]);
  const navHistory = useRef<Array<{ screen: Screen; project: { path: string; name: string } | null }>>([{ screen: "Home", project: null }]);
  const navIndex = useRef(0);
  const [nav, setNav] = useState({ canBack: false, canForward: false });
  const searchRef = useRef<HTMLInputElement>(null);
  const { preference, setPreference } = useTheme();
  const projects = projectView === "pinned" ? pinnedProjects : recentProjects;
  const filteredProjects = useMemo(
    () => projects.filter((project) => project.name.toLowerCase().includes(query.toLowerCase())),
    [projects, query],
  );

  const reloadDashboard = useCallback(() => {
    void loadDashboard().then((snapshot) => {
      setContinueProject(snapshot.continueProject);
      setPinnedProjects(snapshot.pinnedProjects);
      setRecentProjects(snapshot.recentProjects);
      setInstalledApps(snapshot.apps);
    });
  }, []);

  // Per-project health for the home rollup. Loads the cached result instantly so
  // it survives reopening, independent of any in-progress rescan.
  const loadHealth = useCallback(() => {
    if (!isNativeRuntime()) return;
    desktopApi.healthOverview().then(setHealthProjects).catch(() => undefined);
  }, []);

  useEffect(() => { reloadDashboard(); }, [reloadDashboard]);

  // Recent activity for the Home log; refreshes on window focus.
  useEffect(() => {
    if (!isNativeRuntime()) return;
    const load = () => { desktopApi.recentActivity(8).then(setActivity).catch(() => undefined); };
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, []);

  // Recent items for the Continue card (like the dashboard reference).
  useEffect(() => {
    if (!continueProject || !isNativeRuntime()) { setContinueFiles([]); return; }
    let active = true;
    desktopApi.recentFiles(continueProject.path, 4).then((files) => { if (active) setContinueFiles(files); }).catch(() => undefined);
    return () => { active = false; };
  }, [continueProject]);

  // Health refresh strategy (low overhead): show cached instantly, run a full
  // scan once on launch, and a lightweight refresh when the window regains focus
  // (debounced). Each scan updates the cache, then we re-read the per-project view.
  const lastHealthRefresh = useRef(0);
  useEffect(() => {
    loadHealth(); // cached, instant
    if (!isNativeRuntime()) return;
    const refresh = (full: boolean) => {
      const now = Date.now();
      if (!full && now - lastHealthRefresh.current < 20000) return; // debounce focus
      lastHealthRefresh.current = now;
      desktopApi.refreshHealth(full).then(() => loadHealth()).catch(() => undefined);
    };
    // On launch, run the QUICK check first so health appears immediately and
    // reliably (no Git/LFS subprocess), then a full pass in the background to add
    // Git/LFS findings. Focus does a debounced quick refresh.
    refresh(false);
    const full = window.setTimeout(() => refresh(true), 2000);
    const onFocus = () => refresh(false);
    window.addEventListener("focus", onFocus);
    return () => { window.clearTimeout(full); window.removeEventListener("focus", onFocus); };
  }, [loadHealth]);

  useEffect(() => {
    if (!isNativeRuntime()) return;
    desktopApi.appVersion().then(setAppVersion).catch(() => undefined);
    desktopApi.autostartEnabled().then(setAutostart).catch(() => undefined);
    if (autoUpdate) {
      desktopApi.checkForUpdate().then((info) => { if (info.available) setUpdate(info); }).catch(() => undefined);
    }
    if (localStorage.getItem("vantadeck.onboarded") !== "true") setOnboarding(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function setAutoUpdatePref(enabled: boolean) {
    setAutoUpdate(enabled);
    localStorage.setItem("vantadeck.autoUpdate", String(enabled));
    if (enabled) desktopApi.checkForUpdate().then((info) => { if (info.available) setUpdate(info); }).catch(() => undefined);
  }
  function toggleAutostart(enabled: boolean) {
    setAutostart(enabled);
    void run(enabled ? "Enabling launch on startup" : "Disabling launch on startup", async () => {
      await desktopApi.setAutostart(enabled);
      setAutostart(await desktopApi.autostartEnabled());
    });
  }

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
      if (meta && key === "k") { event.preventDefault(); setPaletteOpen((open) => !open); }
      else if (meta && key === "z" && !event.shiftKey) { event.preventDefault(); void undoLast(); }
      else if (meta && (key === "y" || (key === "z" && event.shiftKey))) { event.preventDefault(); void redoLast(); }
      else if (event.altKey && key === "arrowleft") { event.preventDefault(); goBack(); }
      else if (event.altKey && key === "arrowright") { event.preventDefault(); goForward(); }
    };
    // Mouse back/forward buttons (button 3/4), like a browser.
    const onMouse = (event: MouseEvent) => {
      if (event.button === 3) { event.preventDefault(); goBack(); }
      else if (event.button === 4) { event.preventDefault(); goForward(); }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mouseup", onMouse);
    return () => { window.removeEventListener("keydown", onKey); window.removeEventListener("mouseup", onMouse); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function run(label: string, action: () => Promise<unknown>) {
    try { await action(); toast.success(`${label} complete.`); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }

  async function scanWith(rootsValue: string) {
    if (scanning) return;
    setScanning(true);
    setScanProgress({ completed: 0, total: 0, current: "", done: false });
    const unsubscribe = await onScanProgress(setScanProgress);
    try {
      await run("Scanning applications", async () => {
        await desktopApi.scanApps(rootsValue.split(";").map((value) => value.trim()).filter(Boolean));
        await invalidate.apps();
      });
    } finally {
      unsubscribe();
      setScanning(false);
      setScanProgress(null);
    }
  }
  const runScan = () => scanWith(scanRoots);

  function completeOnboarding(prefs: OnboardingPrefs) {
    localStorage.setItem("vantadeck.onboarded", "true");
    localStorage.setItem("vantadeck.profile", JSON.stringify(prefs));
    setPreference(prefs.theme);
    setScanRoots(prefs.scanRoots);
    setOnboarding(false);
    navigate("Applications");
    if (isNativeRuntime()) void scanWith(prefs.scanRoots);
  }
  function skipOnboarding() {
    localStorage.setItem("vantadeck.onboarded", "true");
    setOnboarding(false);
  }

  function applyNavEntry(entry: { screen: Screen; project: { path: string; name: string } | null }) {
    setActiveScreen(entry.screen);
    setSelectedProject(entry.project);
    setNav({ canBack: navIndex.current > 0, canForward: navIndex.current < navHistory.current.length - 1 });
  }
  function navigate(screen: Screen, project: { path: string; name: string } | null = null) {
    const current = navHistory.current[navIndex.current];
    if (current && current.screen === screen && current.project?.path === project?.path) {
      applyNavEntry(current);
      return;
    }
    navHistory.current = navHistory.current.slice(0, navIndex.current + 1);
    navHistory.current.push({ screen, project });
    navIndex.current = navHistory.current.length - 1;
    applyNavEntry({ screen, project });
  }
  function goBack() {
    if (navIndex.current <= 0) return;
    navIndex.current -= 1;
    applyNavEntry(navHistory.current[navIndex.current]);
  }
  function goForward() {
    if (navIndex.current >= navHistory.current.length - 1) return;
    navIndex.current += 1;
    applyNavEntry(navHistory.current[navIndex.current]);
  }

  function openScreen(screen: Screen) {
    navigate(screen, null);
  }

  /// "View update": open Settings and refresh the update check so the panel
  /// reflects the latest state (version, notes, install action).
  function viewUpdate() {
    openScreen("Settings");
    if (isNativeRuntime()) desktopApi.checkForUpdate().then(setUpdate).catch(() => undefined);
  }

  /// Opens the GitHub release page for the available version in the browser.
  function openReleaseNotes() {
    const version = update?.version;
    if (version) void openExternal(`https://github.com/ojaseminem/Vantadeck/releases/tag/v${version}`);
  }

  /// Confirms, then downloads, installs, and restarts to apply the update.
  function startUpdate() {
    if (!update?.available || !isNativeRuntime()) return;
    if (window.confirm(`Download and install version ${update.version} now? PipelineOS will restart.`)) {
      void run("Installing update", () => desktopApi.installUpdate());
    }
  }

  function openProject(target: { path: string; name: string }) {
    if (isNativeRuntime()) desktopApi.recordProjectOpened(target.path).catch(() => undefined);
    navigate("Project", target);
  }

  // Opens a project in its engine. If several versions are installed and none is
  // saved yet, prompts to choose one (then remembers it); otherwise opens directly.
  async function requestOpenInEngine(target: { path: string; name: string }) {
    if (!isNativeRuntime()) return;
    const choice = await desktopApi.engineOptions(target.path).catch(() => null);
    if (!choice) { toast.message("This project isn't connected to an engine."); return; }
    if (choice.options.length === 0) { toast.error("No installed version of this engine was found to open the project."); return; }
    const preferredValid = Boolean(choice.preferred && choice.options.some((option) => option.version === choice.preferred));
    if (choice.options.length > 1 && !preferredValid) {
      setEnginePicker({ path: target.path, name: target.name, appId: choice.appId, options: choice.options });
      return;
    }
    void run(`Opening ${target.name}`, () => desktopApi.openInEngine(target.path));
  }
  function confirmEnginePick(version: string) {
    if (!enginePicker) return;
    const { path, name, appId } = enginePicker;
    setEnginePicker(null);
    void run(`Opening ${name}`, async () => {
      await desktopApi.setProjectEngineVersion(path, appId, version);
      await desktopApi.openInEngine(path);
      reloadDashboard();
    });
  }

  async function importDroppedFolder(path: string) {
    const info = await desktopApi.pathInfo(path).catch(() => null);
    if (!info?.isDir) { toast.error("Drop a project folder to import."); return; }
    const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
    if (!window.confirm(`Import "${name}" as a PipelineOS project?`)) return;
    await run(`Importing ${name}`, async () => { await desktopApi.importProject(path, name); await invalidate.projects(); });
    openProject({ path, name });
  }

  useEffect(() => {
    if (!isNativeRuntime()) return;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload as { type: string; paths?: string[] };
        if (payload.type === "over" || payload.type === "enter") setDragOver(true);
        else if (payload.type === "leave") setDragOver(false);
        else if (payload.type === "drop") { setDragOver(false); const first = payload.paths?.[0]; if (first) void importDroppedFolder(first); }
      }).then((fn) => { unlisten = fn; }),
    ).catch(() => undefined);
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function applyCustomApps(apps: CustomApp[]) {
    setCustomApps(apps);
    saveCustomApps(apps);
  }
  useEffect(() => {
    if (!isNativeRuntime()) { setLocalTools([]); return; }
    let active = true;
    const locals = toolSources.filter((source) => source.type === "local");
    Promise.all(locals.map((source) => desktopApi.readToolsFromDir(source.value).catch(() => [])))
      .then((lists) => { if (active) setLocalTools(lists.flat()); });
    return () => { active = false; };
  }, [toolSources]);

  function applyToolSources(next: ToolSource[]) {
    setToolSources(next);
    saveToolSources(next);
  }
  async function addLocalToolSource() {
    const dir = await browsePath({ directory: true, title: "Choose a folder of tool manifests" });
    if (!dir || toolSources.some((source) => source.value === dir)) return;
    applyToolSources([...toolSources, { id: newId(), type: "local", value: dir }]);
  }
  function addGitToolSource() {
    const url = gitUrl.trim();
    if (!url || toolSources.some((source) => source.value === url)) return;
    applyToolSources([...toolSources, { id: newId(), type: "git", value: url }]);
    setGitUrl("");
  }
  function removeToolSource(id: string) {
    applyToolSources(toolSources.filter((source) => source.id !== id));
  }

  function addCustomApp(event: FormEvent) {
    event.preventDefault();
    if (!customName.trim() || !customExe.trim()) return;
    const previous = customApps;
    const next = [...previous, { id: newId(), name: customName.trim(), category: customCategory, executable: customExe.trim() }];
    setCustomName("");
    setCustomExe("");
    void performUndoable(`Added ${next[next.length - 1].name}`, async () => applyCustomApps(next), async () => applyCustomApps(previous));
  }
  function removeCustomApp(id: string) {
    const previous = customApps;
    const removed = previous.find((app) => app.id === id);
    const next = previous.filter((app) => app.id !== id);
    void performUndoable(`Removed ${removed?.name ?? "app"}`, async () => applyCustomApps(next), async () => applyCustomApps(previous));
  }

  function applyQuickLaunch(entries: QuickLaunchEntry[]) {
    setQuickLaunch(entries);
    saveQuickLaunch(entries);
  }
  function toggleQuickLaunch(entry: QuickLaunchEntry) {
    const previous = quickLaunch;
    const adding = !previous.some((existing) => existing.id === entry.id);
    const next = adding ? [...previous, entry] : previous.filter((existing) => existing.id !== entry.id);
    void performUndoable(
      adding ? "Pinned to Quick Launch" : "Removed from Quick Launch",
      async () => applyQuickLaunch(next),
      async () => applyQuickLaunch(previous),
    );
  }

  // Resolve pinned Quick Launch entries to launchable items, filling in any
  // missing executable/name from detected apps; fall back to the first few
  // detected apps when the user hasn't pinned anything yet.
  const quickLaunchItems = useMemo<QuickLaunchEntry[]>(() => {
    if (!quickLaunch.length) {
      return installedApps.slice(0, 6).map((app) => ({ id: app.id, appId: app.id, name: app.name, executable: app.executable ?? null, custom: false }));
    }
    return quickLaunch.map((entry) => {
      if (entry.executable || entry.custom) return entry;
      const detected = installedApps.find((app) => app.id === entry.appId);
      return detected ? { ...entry, name: detected.name, executable: detected.executable ?? null } : entry;
    });
  }, [quickLaunch, installedApps]);

  function launchQuick(item: QuickLaunchEntry) {
    if (!item.executable) { openScreen("Applications"); return; }
    void run(`Launching ${item.name}`, () => item.custom ? desktopApi.launchExecutable(item.executable!) : desktopApi.launchApp(item.appId, item.executable!));
  }

  const paletteProjects = useMemo(() => {
    const map = new Map<string, { name: string; path: string }>();
    [...pinnedProjects, ...recentProjects].forEach((p) => map.set(p.path, { name: p.name, path: p.path }));
    registeredProjects.forEach((p) => map.set(p.path, { name: p.name, path: p.path }));
    return [...map.values()];
  }, [pinnedProjects, recentProjects, registeredProjects]);

  const searchResults = useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return [] as Array<{ key: string; title: string; subtitle: string; kind: string; onSelect: () => void }>;
    const inScope = (scope: string) => searchScope === "all" || searchScope === scope;
    const results: Array<{ key: string; title: string; subtitle: string; kind: string; onSelect: () => void }> = [];
    if (inScope("projects")) {
      const seen = new Set<string>();
      const add = (p: { name: string; path: string }) => {
        if (seen.has(p.path) || !(p.name.toLowerCase().includes(term) || p.path.toLowerCase().includes(term))) return;
        seen.add(p.path);
        results.push({ key: `p:${p.path}`, title: p.name, subtitle: p.path, kind: "Project", onSelect: () => openProject({ path: p.path, name: p.name }) });
      };
      [...pinnedProjects, ...recentProjects].forEach(add);
      registeredProjects.forEach(add);
    }
    if (inScope("apps")) {
      installedApps.filter((a) => a.name.toLowerCase().includes(term)).forEach((a) =>
        results.push({ key: `a:${a.id}`, title: a.name, subtitle: [...new Set(a.versions.map(formatVersion))].join(", "), kind: "App", onSelect: () => openScreen("Applications") }));
    }
    if (inScope("tools")) {
      tools.filter((t) => t.name.toLowerCase().includes(term) || t.description.toLowerCase().includes(term)).forEach((t) =>
        results.push({ key: `t:${t.id}`, title: t.name, subtitle: t.description, kind: "Tool", onSelect: () => openScreen("Tools") }));
    }
    if (inScope("health")) {
      health.filter((h) => h.title.toLowerCase().includes(term) || h.detail.toLowerCase().includes(term) || h.code.toLowerCase().includes(term)).forEach((h) =>
        results.push({ key: `h:${h.code}`, title: h.title, subtitle: h.detail, kind: "Health", onSelect: () => openScreen("Health") }));
    }
    return results.slice(0, 12);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, searchScope, pinnedProjects, recentProjects, registeredProjects, installedApps, tools, health]);

  async function submitImport(event: FormEvent) {
    event.preventDefault();
    if (!window.confirm(`Import ${rootInput} and create its local PipelineOS metadata?`)) return;
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
        {(() => {
          const allTags = [...new Set(registeredProjects.flatMap((p) => p.tags))].sort();
          const shown = tagFilter ? registeredProjects.filter((p) => p.tags.includes(tagFilter)) : registeredProjects;
          return <>
            <div className="flex items-center justify-between gap-3">
              <SectionLabel>Your projects</SectionLabel>
              {allTags.length ? <div className="flex flex-wrap items-center gap-1.5">
                <button onClick={() => setTagFilter(null)} className={cn("rounded-full px-2.5 py-0.5 text-xs", !tagFilter ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>All</button>
                {allTags.map((tag) => <button key={tag} onClick={() => setTagFilter(tag)} className={cn("rounded-full px-2.5 py-0.5 text-xs", tagFilter === tag ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>{tag}</button>)}
              </div> : null}
            </div>
            <div className="grid gap-3 sm:grid-cols-2">{shown.length ? shown.map((project) => {
              const projectTags = project.tags;
              return (
                <Card key={project.path} className="cursor-pointer transition-colors hover:border-primary/50" onClick={() => openProject({ path: project.path, name: project.name })}>
                  <CardContent className="flex items-start gap-3 p-4">
                    <span className="mt-0.5 flex h-9 w-9 items-center justify-center rounded-lg bg-secondary text-primary"><Folder size={18} /></span>
                    <div className="min-w-0 flex-1"><h3 className="truncate font-medium">{project.name}</h3><p className="truncate text-sm text-muted-foreground">{project.path}</p>
                      {projectTags.length ? <div className="mt-2 flex flex-wrap gap-1">{projectTags.map((tag) => <span key={tag} className="rounded-full bg-secondary px-2 py-0.5 text-[10px] text-muted-foreground">{tag}</span>)}</div> : null}
                      <div className="mt-3 flex gap-2">
                        <Button variant="outline" size="sm" onClick={(e) => { e.stopPropagation(); openProject({ path: project.path, name: project.name }); }}>Open</Button>
                        <Button variant="outline" size="sm" onClick={(e) => { e.stopPropagation(); void performUndoable(project.pinned ? "Unpinned project" : "Pinned project", async () => { await desktopApi.pinProject(project.path, !project.pinned); await invalidate.projects(); reloadDashboard(); }, async () => { await desktopApi.pinProject(project.path, project.pinned); await invalidate.projects(); reloadDashboard(); }); }}>{project.pinned ? "Unpin" : "Pin"}</Button>
                        <Button variant="ghost" size="sm" className="ml-auto text-muted-foreground hover:text-destructive" onClick={(e) => { e.stopPropagation(); if (window.confirm(`Remove "${project.name}" from Pipeline OS? This unregisters it — your files and .vantadeck config stay on disk.`)) void run("Removing project", async () => { await desktopApi.removeProject(project.path); await invalidate.projects(); reloadDashboard(); }); }}>Remove</Button>
                      </div>
                    </div>
                  </CardContent></Card>
              );
            }) : <EmptyState text={tagFilter ? `No projects tagged "${tagFilter}".` : "No projects registered yet. Import one above to get started."} />}</div>
          </>;
        })()}
      </div> : null}

      {activeScreen === "Applications" ? <div className="space-y-5">
        <Panel title="Detect installed applications" description="Leave roots blank to auto-scan standard install folders on every drive, or provide semicolon-separated roots. Scans are additive — a new path adds to what's already detected; entries whose executable no longer exists are dropped. Detection uses bundled, auditable manifests.">
          <div className="flex flex-wrap items-start gap-2">
            <PathInput ariaLabel="Scan roots" directory multi disabled={scanning} placeholder="Blank = all drives, or e.g. D:/Tools; E:/Apps" value={scanRoots} onChange={setScanRoots} />
            <Button disabled={scanning} aria-busy={scanning} onClick={() => void runScan()}>{scanning ? <><RefreshCw size={15} className="animate-spin" /> Scanning…</> : "Scan now"}</Button>
          </div>
          {scanning ? <div className="space-y-2">
            <div className="flex items-center justify-between gap-3 text-sm text-muted-foreground">
              <span className="flex min-w-0 items-center gap-2"><RefreshCw size={14} className="shrink-0 animate-spin" /><span className="truncate">{scanProgress?.current ? `Scanning ${scanProgress.current}…` : "Scanning across your drives…"}</span></span>
              <span className="flex shrink-0 items-center gap-3">{scanProgress && scanProgress.total ? `${scanProgress.completed}/${scanProgress.total}` : ""}<Button variant="outline" size="sm" onClick={() => void desktopApi.cancelScan()}>Cancel</Button></span>
            </div>
            <Progress value={scanProgress && scanProgress.total ? (scanProgress.completed / scanProgress.total) * 100 : 8} />
          </div> : null}
        </Panel>
        {CATEGORY_ORDER.filter((category) => managedApps.some((app) => app.category === category && app.installations.length > 0)).map((category) => (
          <section key={category} className="space-y-2">
            <SectionLabel>{APP_CATEGORY_LABELS[category] ?? category}</SectionLabel>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{managedApps.filter((app) => app.category === category && app.installations.length > 0).map((app) => {
              const iconInstall = app.installations.find((item) => item.runnable) ?? app.installations[0];
              const launchTarget = app.installations.find((item) => item.runnable);
              return (
                <Card key={app.id}><CardContent className="flex items-start gap-3 p-4">
                  <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-secondary"><AppIcon executable={iconInstall?.executable} size={26} /></span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center justify-between gap-2"><h3 className="truncate font-medium">{app.name}</h3><span className="flex shrink-0 items-center gap-1"><Button variant="ghost" size="icon" className="h-7 w-7" aria-label={`Set a version override for ${app.name}`} title="Add or override a version (point at a specific executable)" onClick={() => { setOverrideApp({ id: app.id, name: app.name }); setOverrideVersion(""); setOverrideExe(""); }}><Plus size={15} /></Button>{app.launchable && launchTarget ? <Button variant="outline" size="sm" onClick={() => { if (window.confirm(`Launch ${app.name} ${formatVersion(launchTarget.version)}?`)) void run(`Launching ${app.name}`, () => desktopApi.launchApp(app.id, launchTarget.executable)); }}>Launch</Button> : null}</span></div>
                    {app.launchable ? <div className="mt-2 space-y-1">{app.installations.map((item) => {
                      const entry = { id: `${app.id}::${item.executable}`, appId: app.id, name: `${app.name} ${formatVersion(item.version)}`, executable: item.executable, version: item.version, custom: false };
                      const pinned = pinnedIds.has(entry.id);
                      return (
                        <div key={item.executable} className="flex items-center gap-1.5 text-sm">
                          <Badge variant={item.runnable ? "secondary" : "outline"} className={item.runnable ? "" : "text-muted-foreground line-through"}>{formatVersion(item.version)}</Badge>
                          {item.runnable ? <>
                            <Button variant="ghost" size="sm" className="ml-auto h-7 px-2" onClick={() => { if (window.confirm(`Launch ${entry.name}?`)) void run(`Launching ${entry.name}`, () => desktopApi.launchApp(app.id, item.executable)); }}>Launch</Button>
                            <Button variant="ghost" size="icon" className="h-7 w-7" aria-label={pinned ? `Unpin ${entry.name} from Quick Launch` : `Pin ${entry.name} to Quick Launch`} onClick={() => toggleQuickLaunch(entry)}><Star size={14} className={pinned ? "fill-primary text-primary" : "text-muted-foreground"} /></Button>
                          </> : <span className="ml-auto text-xs text-muted-foreground">Incompatible</span>}
                        </div>
                      );
                    })}</div> : <div className="mt-1.5 flex flex-wrap gap-1">{app.installations.map((item) => <Badge key={item.executable} variant="outline" className="text-muted-foreground">{formatVersion(item.version)}</Badge>)}</div>}
                    {!app.launchable ? <p className="mt-1.5 text-xs text-primary">Detected — used for project version control</p> : null}
                  </div>
                </CardContent></Card>
              );
            })}</div>
          </section>
        ))}
        {customApps.length ? <section className="space-y-2">
          <SectionLabel>Your apps</SectionLabel>
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{customApps.map((app) => (
            <Card key={app.id}><CardContent className="flex items-start gap-3 p-4">
              <span className="flex h-10 w-10 items-center justify-center rounded-lg bg-secondary"><AppIcon executable={app.executable} size={26} /></span>
              <div className="min-w-0 flex-1">
                <div className="flex items-center justify-between gap-2"><h3 className="truncate font-medium">{app.name}</h3><span className="flex items-center gap-1"><Button variant="ghost" size="icon" aria-label={pinnedIds.has(app.id) ? `Remove ${app.name} from Quick Launch` : `Add ${app.name} to Quick Launch`} onClick={() => toggleQuickLaunch({ id: app.id, appId: app.id, name: app.name, executable: app.executable, custom: true })}><Star size={15} className={pinnedIds.has(app.id) ? "fill-primary text-primary" : "text-muted-foreground"} /></Button><Button variant="outline" size="sm" onClick={() => { if (window.confirm(`Launch ${app.name}?`)) void run(`Launching ${app.name}`, () => desktopApi.launchExecutable(app.executable)); }}>Launch</Button></span></div>
                <p className="mt-1 truncate text-xs text-muted-foreground">{app.executable}</p>
                <Button variant="ghost" size="sm" className="mt-1 h-auto p-0 text-muted-foreground" onClick={() => removeCustomApp(app.id)}>Remove</Button>
              </div>
            </CardContent></Card>
          ))}</div>
        </section> : null}
        {managedApps.every((app) => app.installations.length === 0) && !customApps.length ? <EmptyState text="No applications detected yet. Click Scan now to find installed creative tools across your drives, or add your own below." /> : null}
        <form onSubmit={addCustomApp}>
          <Panel title="Add an application" description="PipelineOS supports many engines and creative tools out of the box. Add anything else installed on your machine here.">
            <div className="flex flex-wrap items-start gap-2">
              <Input aria-label="App name" required placeholder="App name" value={customName} onChange={(e) => setCustomName(e.target.value)} className="w-48" />
              <select aria-label="App category" value={customCategory} onChange={(e) => setCustomCategory(e.target.value)} className={themeSelectClass}>
                <option value="game-engine">Game Engine</option><option value="dcc">3D / DCC</option><option value="art">2D / Art</option><option value="code">Code / IDE</option><option value="utility">Utility</option>
              </select>
              <PathInput ariaLabel="App executable" required directory={false} placeholder="C:/Tools/app.exe" value={customExe} onChange={setCustomExe} />
              <Button type="submit">Add app</Button>
            </div>
          </Panel>
        </form>
        <details className="rounded-lg border border-border p-4 text-sm">
          <summary className="cursor-pointer font-medium">Supported applications ({managedApps.length})</summary>
          <div className="mt-3 flex flex-wrap gap-1.5">{managedApps.map((app) => <Badge key={app.id} variant={app.installations.length ? "secondary" : "outline"} className={app.installations.length ? "" : "text-muted-foreground"}>{app.name}</Badge>)}</div>
        </details>
      </div> : null}


      {activeScreen === "Tools" ? (() => {
        const allTools = [...tools.filter((tool) => tool.reviewState !== "withdrawn"), ...localTools];
        return <div className="space-y-4">
          <Panel title="Tool sources" description="PipelineOS reads validated tool manifests from local folders you trust, and lets you register Git repositories to clone and add. It never executes downloaded installers automatically; network access stays opt-in.">
            <div className="flex flex-wrap items-end gap-2">
              <Button variant="outline" onClick={() => void addLocalToolSource()} disabled={!isNativeRuntime()}><FolderOpen size={15} /> Add local folder…</Button>
              <form className="flex flex-1 items-end gap-2" onSubmit={(e) => { e.preventDefault(); addGitToolSource(); }}>
                <label className="flex flex-1 flex-col gap-1 text-xs text-muted-foreground">Git repository<Input aria-label="Git repository URL" placeholder="https://github.com/org/tools-index.git" value={gitUrl} onChange={(e) => setGitUrl(e.target.value)} className="min-w-56" /></label>
                <Button type="submit" variant="outline"><Plus size={15} /> Add repo</Button>
              </form>
            </div>
            {toolSources.length ? <ul className="space-y-1.5">{toolSources.map((source) => (
              <li key={source.id} className="flex items-center gap-2.5 rounded-lg border border-border p-2.5 text-sm">
                <Badge variant="outline" className="shrink-0 text-[10px] uppercase">{source.type}</Badge>
                <span className="min-w-0 flex-1 truncate" title={source.value}>{source.value}</span>
                {source.type === "git" ? <span className="shrink-0 text-xs text-muted-foreground">clone locally, then add its folder</span> : null}
                <Button variant="ghost" size="icon" aria-label="Remove source" onClick={() => removeToolSource(source.id)}><Trash2 size={14} /></Button>
              </li>
            ))}</ul> : <p className="text-sm text-muted-foreground">No custom sources yet. Add a local folder of tool manifests, or register a Git repo.</p>}
          </Panel>
          <SectionLabel>Available tools ({allTools.length})</SectionLabel>
          <div className="grid gap-3 sm:grid-cols-2">{allTools.length ? allTools.map((tool) => (
            <Card key={tool.id}><CardContent className="flex items-start gap-3 p-4">
              <Wrench className="text-primary" />
              <div className="min-w-0 flex-1"><h3 className="font-medium">{tool.name}</h3><p className="text-sm text-muted-foreground">{tool.description}</p><small className="text-xs text-muted-foreground">{tool.reviewState} · {tool.license} · checked {tool.lastVerifiedAt}</small></div>
              <Button variant="outline" size="sm" onClick={() => { if (/^https?:\/\//i.test(tool.sourceUrl)) void openExternal(tool.sourceUrl); else toast.message("Source", { description: tool.sourceUrl }); }}>Source</Button>
            </CardContent></Card>
          )) : <EmptyState text="No tools available. Add a local folder of tool manifests above to populate this list." />}</div>
        </div>;
      })() : null}

      {activeScreen === "Settings" ? <div className="grid gap-4 sm:grid-cols-2">
        <Panel title="Appearance"><label className="flex items-center gap-3 text-sm">Theme<select aria-label="Settings theme" className={themeSelectClass} value={preference} onChange={(event) => setPreference(event.target.value as ThemePreference)}><option value="system">System</option><option value="dark">Dark</option><option value="light">Light</option></select></label></Panel>
        <Panel title="Updates">
          {update?.available ? <p className="text-sm">Version <strong>{update.version}</strong> is available (you have {update.currentVersion}).{update.notes ? <><br /><span className="text-muted-foreground">{update.notes}</span></> : null}</p> : <p className="text-sm text-muted-foreground">{update ? `You are on the latest version (${update.currentVersion}).` : "Check for a newer signed release. Updates are verified against PipelineOS's signing key before installation."}</p>}
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => void run("Checking for updates", async () => { const info = await desktopApi.checkForUpdate(); setUpdate(info); toast.message(info.available ? `Update ${info.version} is available.` : "You are on the latest version."); })}><RefreshCw size={15} /> Check for updates</Button>
            {update?.available ? <Button variant="outline" onClick={openReleaseNotes}><ExternalLink size={15} /> Release notes</Button> : null}
            {update?.available ? <Button onClick={startUpdate}><Download size={15} /> Install &amp; restart</Button> : null}
          </div>
          <label className="flex items-center gap-2 pt-1 text-sm"><input type="checkbox" checked={autoUpdate} onChange={(event) => setAutoUpdatePref(event.target.checked)} className="size-4 accent-[var(--primary)]" /> Automatically check for updates on launch</label>
          <p className="text-xs text-muted-foreground">Update info comes from PipelineOS's GitHub releases. Downloads are verified against the signing key before install.</p>
        </Panel>
        <Panel title="Startup">
          <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={autostart} disabled={!isNativeRuntime()} onChange={(event) => toggleAutostart(event.target.checked)} className="size-4 accent-[var(--primary)]" /> Launch Pipeline OS when I sign in to Windows</label>
          <p className="text-xs text-muted-foreground">Starts the app automatically at login. You can turn this off any time.</p>
        </Panel>
        <Panel title="Runtime"><p className="text-sm text-muted-foreground">{runtimeLabel}. Network operations are disabled by default; all indexed data is stored locally.</p></Panel>
      </div> : null}
    </section>
  );

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      {dragOver ? <div className="pointer-events-none fixed inset-0 z-[100] flex items-center justify-center bg-background/80 backdrop-blur-sm"><div className="rounded-2xl border-2 border-dashed border-primary px-10 py-8 text-center"><Folder size={36} className="mx-auto mb-2 text-primary" /><p className="text-lg font-semibold">Drop a folder to import it as a project</p></div></div> : null}
      <aside className="flex w-64 flex-none flex-col gap-4 border-r border-sidebar-border bg-sidebar p-4">
        <BrandLockup className="px-2 py-1" />
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
        <div className="flex items-center justify-between px-2 text-xs text-muted-foreground"><span>v{appVersion}</span><span className="flex items-center gap-1"><Code2 size={13} /> Open Source</span></div>
      </aside>

      <main className="flex flex-1 flex-col overflow-hidden">
        <header className="flex items-center gap-3 border-b border-border px-6 py-3">
          <div className="flex items-center gap-0.5">
            <Button variant="ghost" size="icon" aria-label="Back" disabled={!nav.canBack} onClick={goBack}><ArrowLeft size={18} /></Button>
            <Button variant="ghost" size="icon" aria-label="Forward" disabled={!nav.canForward} onClick={goForward}><ArrowRight size={18} /></Button>
          </div>
          <div className="relative flex flex-1 items-center gap-1.5 rounded-lg border border-border bg-secondary/50 pl-1 pr-3">
            <DropdownMenu>
              <DropdownMenuTrigger asChild><Button variant="ghost" size="sm" className="h-7 gap-1 text-xs text-muted-foreground">{SCOPE_LABELS[searchScope]} <ChevronDown size={13} /></Button></DropdownMenuTrigger>
              <DropdownMenuContent align="start">{SEARCH_SCOPES.map((scope) => <DropdownMenuItem key={scope} onClick={() => setSearchScope(scope)}>{SCOPE_LABELS[scope]}</DropdownMenuItem>)}</DropdownMenuContent>
            </DropdownMenu>
            <Search size={16} className="shrink-0 text-muted-foreground" />
            <input ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} onFocus={() => setSearchOpen(true)} onBlur={() => window.setTimeout(() => setSearchOpen(false), 150)} placeholder={`Search ${SCOPE_LABELS[searchScope].toLowerCase()}…`} className="h-9 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground" />
            <kbd className="rounded border border-border px-1.5 text-[10px] text-muted-foreground">Ctrl K</kbd>
            {searchOpen && query.trim() ? <div className="absolute inset-x-0 top-12 z-50 max-h-96 overflow-y-auto rounded-lg border border-border bg-popover p-1 shadow-xl">
              {searchResults.length ? searchResults.map((result) => (
                <button key={result.key} onMouseDown={(event) => { event.preventDefault(); result.onSelect(); setSearchOpen(false); }} className="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm hover:bg-muted/60">
                  <span className="min-w-0 flex-1"><strong className="block truncate font-medium">{result.title}</strong><small className="block truncate text-xs text-muted-foreground">{result.subtitle}</small></span>
                  <Badge variant="outline" className="shrink-0 text-[10px]">{result.kind}</Badge>
                </button>
              )) : <div className="px-3 py-4 text-sm text-muted-foreground">No matches in {SCOPE_LABELS[searchScope]}.</div>}
            </div> : null}
          </div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground"><Laptop size={15} /> Offline • Local mode</div>
        </header>

        {update?.available ? <div className="flex items-center gap-2 border-b border-border bg-primary/10 px-6 py-2 text-sm"><Download size={15} className="shrink-0 text-primary" /><span className="min-w-0 flex-1 truncate">PipelineOS {update.version} is available.</span><Button variant="link" size="sm" className="ml-auto h-auto shrink-0 p-0" onClick={viewUpdate}>View update</Button></div> : null}

        <div className="flex-1 overflow-y-auto p-6">
          {activeScreen === "Home" ? <div className="space-y-6">
            <section aria-labelledby="continue-title">
              <SectionLabel><span id="continue-title">Continue Project</span></SectionLabel>
              {continueProject ? <Card className="overflow-hidden"><CardContent className="grid gap-5 p-0 lg:grid-cols-[260px_1fr_280px]">
                {isDemoMode() ? <img src={voidlineImage} alt="Voidline reactor environment" className="h-full max-h-64 w-full object-cover" /> : <ProjectThumb projectPath={continueProject.path} thumbnail={continueProject.thumbnail} className="h-full max-h-64 w-full rounded-none" iconSize={48} alt={`${continueProject.name} thumbnail`} />}
                <div className="space-y-3 py-5"><h1 className="text-2xl font-semibold">{continueProject.name}</h1><p className="text-sm text-muted-foreground">{continueProject.path}</p>
                  <div className="flex flex-wrap items-center gap-3 text-sm text-muted-foreground"><span className="flex items-center gap-1.5"><AppIcon executable={continueProject.engineExecutable ?? undefined} size={16} /> {continueProject.engine}{continueProject.version ? ` ${continueProject.version}` : ""}</span>{continueProject.branch ? <span className="flex items-center gap-1.5"><GitBranch size={13} /> {continueProject.branch}</span> : null}{formatLastOpened(continueProject.lastOpened) ? <span>Last opened: {formatLastOpened(continueProject.lastOpened)}</span> : null}</div>
                  {continueFiles.length ? <div className="pt-1"><div className="mb-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Recent items</div><ul className="space-y-1">{continueFiles.map((file) => (
                    <li key={file.path} className="flex items-center gap-2 rounded-md px-1 py-1 text-sm hover:bg-muted/40">
                      <FileCode2 size={14} className="shrink-0 text-muted-foreground" />
                      <span className="min-w-0 flex-1 truncate" title={file.path}>{file.name}</span>
                      <span className="shrink-0 text-xs text-muted-foreground">{timeAgo(file.modified)}</span>
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-6 w-6 shrink-0" aria-label={`Actions for ${file.name}`}><MoreHorizontal size={15} /></Button></DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-52">
                          <DropdownMenuItem onClick={() => void run("Opening file", () => desktopApi.openPath(file.path))}><FileCode2 size={14} /> Open file</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => { const dir = file.path.replace(/[\\/][^\\/]*$/, ""); if (dir) void run("Opening folder", () => desktopApi.openPath(dir)); }}><FolderOpen size={14} /> Open containing folder</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => { void navigator.clipboard?.writeText(file.path); toast.success("Path copied."); }}><Clipboard size={14} /> Copy path</DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </li>
                  ))}</ul></div> : null}
                </div>
                <div className="space-y-3 border-l border-border p-5"><div className="flex gap-1">
                  <Button className="flex-1" onClick={() => continueProject && openProject({ path: continueProject.path, name: continueProject.name })}>Open Project</Button>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild><Button variant="outline" size="icon" aria-label="More project commands"><ChevronDown size={16} /></Button></DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-52">
                      <DropdownMenuItem onClick={() => continueProject && openProject({ path: continueProject.path, name: continueProject.name })}><Folder size={14} /> Open project</DropdownMenuItem>
                      <DropdownMenuItem disabled={!continueProject.engineId} onClick={() => continueProject && void requestOpenInEngine(continueProject)}><Rocket size={14} /> Open in Engine</DropdownMenuItem>
                      <DropdownMenuItem onClick={() => continueProject && openProject({ path: continueProject.path, name: continueProject.name })}><GitBranch size={14} /> Source control</DropdownMenuItem>
                      <DropdownMenuItem onClick={() => { if (continueProject && isNativeRuntime()) void run("Opening folder", () => desktopApi.openPath(continueProject.path)); }}><FolderOpen size={14} /> Open folder</DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem onClick={() => { if (continueProject) { void navigator.clipboard?.writeText(continueProject.path); toast.success("Path copied."); } }}><Clipboard size={14} /> Copy project path</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
                  <div><h2 className="mb-2 text-sm font-semibold">Health summary</h2>{(() => {
                    const ph = healthProjects.find((entry) => entry.path === continueProject.path);
                    if (!ph || !ph.checkedAt) return <p className="text-sm text-muted-foreground">Checking project health…</p>;
                    if (!ph.issues.length) return <p className="flex items-center gap-1.5 text-sm text-muted-foreground"><CircleAlert size={15} className="text-primary" /> No issues — looks healthy.</p>;
                    return <div className="space-y-2">{ph.issues.slice(0, 3).map((issue, index) => <div key={`${issue.code}-${index}`} className="flex items-start gap-2 text-sm"><CircleAlert className={cn("shrink-0", issue.severity === "error" ? "text-destructive" : "text-primary")} size={16} /><span className="min-w-0" title={issue.detail}><strong className="block truncate">{issue.title}</strong><small className="line-clamp-2 text-muted-foreground">{issue.detail}</small></span></div>)}</div>;
                  })()}</div>
                </div>
              </CardContent></Card> : <Card><CardContent className="p-6"><EmptyState text="Import a project to start working locally." /></CardContent></Card>}
            </section>

            <section className="grid gap-5 lg:grid-cols-[1fr_320px]">
              <div className="space-y-3">
                <div className="flex gap-2" role="tablist">
                  <button role="tab" aria-selected={projectView === "pinned"} onClick={() => setProjectView("pinned")} className={cn("rounded-lg px-3 py-1.5 text-sm", projectView === "pinned" ? "bg-secondary font-medium" : "text-muted-foreground hover:text-foreground")}>Pinned Projects</button>
                  <button role="tab" aria-selected={projectView === "recent"} onClick={() => setProjectView("recent")} className={cn("rounded-lg px-3 py-1.5 text-sm", projectView === "recent" ? "bg-secondary font-medium" : "text-muted-foreground hover:text-foreground")}>Recent Projects</button>
                </div>
                <ProjectTable projects={filteredProjects} actions={{
                  onOpen: (p) => openProject({ path: p.path, name: p.name }),
                  onLaunch: (p) => void requestOpenInEngine(p),
                  onOpenFolder: (p) => { if (isNativeRuntime()) void run("Opening folder", () => desktopApi.openPath(p.path)); },
                  onCopyPath: (p) => { void navigator.clipboard?.writeText(p.path); toast.success("Path copied."); },
                }} />
                <Button variant="link" className="px-0" onClick={() => openScreen("Projects")}>View all projects <ChevronRight size={16} /></Button>
              </div>
              <aside className="space-y-4">
                <Card><CardContent className="p-4"><div className="mb-2 flex items-center justify-between text-sm font-semibold">Project Health <Button variant="ghost" size="sm" className="h-auto p-0 text-muted-foreground" onClick={() => openScreen("Health")}>View All</Button></div>{healthProjects.length ? <div className="space-y-0.5">{healthProjects.slice(0, 6).map((entry) => {
                  const errors = entry.issues.filter((issue) => issue.severity === "error").length;
                  const warnings = entry.issues.length - errors;
                  return <button key={entry.path} onClick={() => openProject({ path: entry.path, name: entry.name })} className="flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-sm hover:bg-muted/50">
                    <span className="min-w-0 flex-1 truncate">{entry.name}</span>
                    {!entry.checkedAt ? <span className="shrink-0 text-xs text-muted-foreground">…</span>
                      : errors ? <Badge variant="outline" className="shrink-0 border-destructive/50 text-destructive">{errors} error{errors > 1 ? "s" : ""}</Badge>
                      : warnings ? <Badge variant="outline" className="shrink-0">{warnings} warning{warnings > 1 ? "s" : ""}</Badge>
                      : <Badge variant="secondary" className="shrink-0 text-primary">Healthy</Badge>}
                  </button>;
                })}</div> : <EmptyState text="No projects to check yet." />}</CardContent></Card>
                <Card><CardContent className="p-4"><div className="mb-2 flex items-center justify-between text-sm font-semibold">Installed Apps <Button variant="ghost" size="sm" className="h-auto p-0 text-muted-foreground" onClick={() => openScreen("Applications")}>Manage</Button></div><div className="space-y-1">{installedApps.length ? installedApps.map((app) => <button key={app.name} onClick={() => openScreen("Applications")} title={`Manage ${app.name} versions`} className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-muted/50"><span className="flex h-8 w-8 items-center justify-center rounded-md bg-secondary"><AppIcon executable={app.executable ?? undefined} size={18} /></span><span className="min-w-0 flex-1"><strong className="block truncate">{app.name}</strong><small className="block truncate text-xs text-muted-foreground">{[...new Set(app.versions.map(formatVersion))].join(", ")}</small></span><ChevronRight size={15} className="text-muted-foreground" /></button>) : <EmptyState text="No apps detected yet." />}</div></CardContent></Card>
                {activity.length ? <Card><CardContent className="p-4"><div className="mb-2 text-sm font-semibold">Recent activity</div><ul className="space-y-1.5">{activity.map((entry) => (
                  <li key={entry.id} className="flex items-start gap-2 text-sm"><Activity size={14} className="mt-0.5 shrink-0 text-muted-foreground" /><span className="min-w-0 flex-1"><span className="block truncate">{entry.message}</span><span className="block text-xs text-muted-foreground">{formatLastOpened(entry.createdAt)}</span></span></li>
                ))}</ul></CardContent></Card> : null}
              </aside>
            </section>
          </div> : activeScreen === "Project" && selectedProject ? <ProjectDetail project={selectedProject} onBack={() => openScreen("Projects")} onRenamed={() => { void invalidate.projects(); reloadDashboard(); }} onOpenInEngine={requestOpenInEngine} /> : activeScreen === "Health" ? <HealthScreen projects={registeredProjects} onOpenProject={openProject} /> : managementContent}
        </div>

        <Dialog open={!!enginePicker} onOpenChange={(open) => { if (!open) setEnginePicker(null); }}>
          <DialogContent className="max-w-sm">
            <DialogHeader>
              <DialogTitle>Choose a version</DialogTitle>
              <DialogDescription>Multiple versions are installed. Pick the one to open {enginePicker?.name} in — it's saved for next time and editable from the project view.</DialogDescription>
            </DialogHeader>
            <div className="space-y-1">{enginePicker?.options.map((option) => (
              <Button key={option.version} variant="outline" className="w-full justify-start" onClick={() => confirmEnginePick(option.version)}>{formatVersion(option.version)}</Button>
            ))}</div>
          </DialogContent>
        </Dialog>
        <Dialog open={!!overrideApp} onOpenChange={(open) => { if (!open) setOverrideApp(null); }}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Set a version for {overrideApp?.name}</DialogTitle>
              <DialogDescription>Point a version at a specific executable on this machine. Useful when a version isn't auto-detected or is detected incorrectly. This override is machine-local.</DialogDescription>
            </DialogHeader>
            <form className="space-y-3" onSubmit={(e) => { e.preventDefault(); const app = overrideApp; const version = overrideVersion.trim(); const exe = overrideExe.trim(); if (!app || !version || !exe) return; setOverrideApp(null); void run("Saving version override", async () => { await desktopApi.setManualOverride(app.id, version, exe); await invalidate.apps(); }); }}>
              <label className="block text-sm">Version<Input aria-label="Version" required placeholder="e.g. 2022.3.18f1" value={overrideVersion} onChange={(e) => setOverrideVersion(e.target.value)} className="mt-1" /></label>
              <label className="block text-sm">Executable<PathInput ariaLabel="Executable" directory={false} required placeholder="C:/…/app.exe" value={overrideExe} onChange={setOverrideExe} /></label>
              <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={() => setOverrideApp(null)}>Cancel</Button><Button type="submit" disabled={!isNativeRuntime() || !overrideVersion.trim() || !overrideExe.trim()}>Save override</Button></div>
            </form>
          </DialogContent>
        </Dialog>
        <Onboarding open={onboarding} onComplete={completeOnboarding} onSkip={skipOnboarding} />
        <CommandDialog open={paletteOpen} onOpenChange={setPaletteOpen}>
          <CommandInput placeholder="Type a command or search projects, apps…" />
          <CommandList>
            <CommandEmpty>No results found.</CommandEmpty>
            <CommandGroup heading="Go to">
              {navigation.map(([label, Icon]) => <CommandItem key={label} value={`go ${label}`} onSelect={() => { openScreen(label); setPaletteOpen(false); }}><Icon size={15} /> {label}</CommandItem>)}
            </CommandGroup>
            {paletteProjects.length ? <CommandGroup heading="Projects">
              {paletteProjects.map((p) => <CommandItem key={p.path} value={`project ${p.name} ${p.path}`} onSelect={() => { openProject({ path: p.path, name: p.name }); setPaletteOpen(false); }}><Folder size={15} /> {p.name}</CommandItem>)}
            </CommandGroup> : null}
            {installedApps.length ? <CommandGroup heading="Applications">
              {installedApps.map((a) => <CommandItem key={a.id} value={`app ${a.name}`} onSelect={() => { openScreen("Applications"); setPaletteOpen(false); }}><AppWindow size={15} /> {a.name}</CommandItem>)}
            </CommandGroup> : null}
            <CommandGroup heading="Actions">
              <CommandItem value="scan applications" onSelect={() => { setPaletteOpen(false); openScreen("Applications"); void runScan(); }}><RefreshCw size={15} /> Scan applications</CommandItem>
              <CommandItem value="check for updates" onSelect={() => { setPaletteOpen(false); void run("Checking for updates", async () => { const info = await desktopApi.checkForUpdate(); setUpdate(info); toast.message(info.available ? `Update ${info.version} is available.` : "You are on the latest version."); }); }}><Download size={15} /> Check for updates</CommandItem>
            </CommandGroup>
          </CommandList>
        </CommandDialog>
        <footer className="flex items-center gap-2 border-t border-border px-6 py-2.5">
          <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Quick Launch</span>
          {quickLaunchItems.map((item) => <Button key={item.id} variant="ghost" size="sm" title={item.executable ? `Launch ${item.name}` : "Open Applications"} onClick={() => launchQuick(item)}><AppIcon executable={item.executable ?? undefined} size={18} /> {item.name}</Button>)}
          {quickLaunchItems.length === 0 ? <Button variant="ghost" size="sm" onClick={() => openScreen("Applications")}><Search size={18} /> Add apps to Quick Launch</Button> : null}
          <span className="ml-auto" />
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

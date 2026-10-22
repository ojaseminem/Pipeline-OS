import { type FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  AppWindow,
  ArrowLeft,
  ArrowRight,
  Box,
  Bug,
  ChevronDown,
  ChevronRight,
  Clipboard,
  Download,
  FileCode2,
  Folder,
  FolderOpen,
  FolderPlus,
  GitBranch,
  HardDrive,
  Home,
  Keyboard,
  LayoutDashboard,
  Loader2,
  Lock,
  MoreHorizontal,
  Palette,
  Pin,
  PinOff,
  Play,
  Plus,
  RefreshCw,
  Rocket,
  ScanSearch,
  Search,
  Settings,
  Star,
  Trash2,
  Workflow,
  Wrench,
  KanbanSquare,
} from "lucide-react";
import { QueryClientProvider } from "@tanstack/react-query";
import { toast } from "sonner";
import { DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut } from "@/components/ui/command";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Toaster } from "@/components/ui/sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { TooltipProvider } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { installedApps as defaultApps, pinnedProjects as defaultPinned, recentProjects as defaultRecent, type Project } from "./data";
import { APP_CATEGORY_LABELS, browsePath, desktopApi, formatVersion, isDemoMode, isNativeRuntime, loadDashboard, onProjectScanProgress, onScanProgress, openExternal, studioApi, studioAvailable, type ActivityRecord, type EngineVersionOption, type HealthSummary, type ProjectHealthOverview, type RecentFile, type ScanProgress, type ToolManifest, type UpdateInfo } from "./bridge";
import { formatLastOpened, timeAgo } from "./lib/format";
import { categoryClass } from "./lib/categories";
import { ProjectThumb } from "./components/thumbnail";
import { createQueryClient, useApps, useInvalidate, useProjects, useTools } from "./lib/queries";
import { THEMES, type ThemePreference, useTheme } from "./theme";
import { BrandGlyph, BrandLockup } from "./components/brand";
import { Onboarding, type OnboardingPrefs } from "./components/onboarding";
import { PathInput } from "./components/path-input";
import { ProjectDetail } from "./components/project-detail";
import { HealthScreen } from "./components/health-screen";
import { ConfirmHost } from "./components/confirm";
import { confirmAction } from "./lib/confirm";
import { EmptyState } from "./components/empty-state";
import { PageHeader } from "./components/page-header";
import { AppIcon } from "./components/app-icon";
import { ProjectTable, type ProjectActions } from "./components/project-table";
import { TaskRunsProvider } from "./components/task-runs";
import { useTaskRuns } from "./lib/task-runs-context";
import { ActiveTasksCard, GettingStarted, MyWorkCard } from "./components/home-widgets";
import { ActivityScreen } from "./components/activity-screen";
import { REPO_URL, SettingsScreen } from "./components/settings-screen";
import { AddProjectDialog } from "./components/add-project-dialog";
import { DiscoveredProjects } from "./components/discovered-projects";
import { PlanningScreen } from "./components/planning-screen";
import { UpdateIndicator } from "./components/update-indicator";
import { useDiscovery } from "./lib/discovery";
import { getPrefs, setPrefs, usePrefs } from "./lib/prefs";
import { useAppearance } from "./lib/appearance";
import { ACTIVITY_LABELS, useEngineActivity, useEngineActivityFeed } from "./lib/engine-activity";
import { currentPlatform, keyLabel, platformExamples } from "./lib/platform";
import { Kbd, ShortcutsDialog } from "./components/shortcuts-dialog";
import { loadCustomApps, loadQuickLaunch, loadToolSources, newId, saveCustomApps, saveQuickLaunch, saveToolSources, type CustomApp, type QuickLaunchEntry, type ToolSource } from "./lib/local-store";
import voidlineImage from "./assets/voidline-reactor.png";

type UndoEntry = { label: string; undo: () => Promise<void>; redo: () => Promise<void> };

const navigation = [
  ["Home", Home],
  ["Projects", Folder],
  ["Planning", KanbanSquare],
  ["Applications", Box],
  ["Health", Activity],
  ["Tools", Wrench],
  ["Activity", Workflow],
  ["Settings", Settings],
] as const;

type Screen = (typeof navigation)[number][0] | "Project";
type ProjectTarget = { path: string; name: string; tab?: string };

/** Build channel: "dev" for PipelineOS Dev builds (set by the dev pipeline). */
const CHANNEL: "stable" | "dev" = import.meta.env.VITE_PIPELINEOS_CHANNEL === "dev" ? "dev" : "stable";

const sampleContinueProject: Project = {
  name: "Voidline",
  path: "D:/Dev/Projects/Voidline",
  engine: "Unity",
  engineId: "unity",
  version: "2022.3.18f1",
  branch: "main",
  lastOpened: "Today, 10:42 AM",
};

const CATEGORY_ORDER = ["game-engine", "dcc", "art", "media", "productivity", "code", "version-control", "utility"];

const selectClass = "h-9 rounded-md border border-input bg-transparent px-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring";

function readFlag(key: string): boolean {
  try { return localStorage.getItem(key) === "true"; } catch { return false; }
}
function writeFlag(key: string, value: boolean) {
  try { localStorage.setItem(key, String(value)); } catch { /* storage unavailable */ }
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">{children}</div>;
}

function Panel({ title, description, children, actions }: { title: string; description?: string; children: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <Card><CardContent className="space-y-3 p-5">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div><h2 className="text-base font-semibold">{title}</h2>{description ? <p className="mt-0.5 text-sm text-muted-foreground">{description}</p> : null}</div>
        {actions}
      </div>
      {children}
    </CardContent></Card>
  );
}

function AppShell() {
  const [activeScreen, setActiveScreen] = useState<Screen>(() => getPrefs().startScreen);
  const [projectView, setProjectView] = useState<"pinned" | "recent">("pinned");
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
  const health = useMemo<HealthSummary[]>(
    () => healthProjects.flatMap((project) => project.issues.map((issue) => ({ code: issue.code, title: issue.title, detail: issue.detail, severity: issue.severity, project: project.name }))),
    [healthProjects],
  );
  const projectsQuery = useProjects(activeScreen === "Projects" || activeScreen === "Health" || activeScreen === "Home");
  const appsQuery = useApps(activeScreen === "Applications");
  const toolsQuery = useTools(activeScreen === "Tools");
  const registeredProjects = useMemo(() => projectsQuery.data ?? [], [projectsQuery.data]);
  const managedApps = useMemo(() => appsQuery.data ?? [], [appsQuery.data]);
  const tools = useMemo(() => toolsQuery.data ?? [], [toolsQuery.data]);
  const invalidate = useInvalidate();
  const { active: activeRuns } = useTaskRuns();
  const [customApps, setCustomApps] = useState<CustomApp[]>(() => loadCustomApps());
  const [addAppOpen, setAddAppOpen] = useState(false);
  const [customName, setCustomName] = useState("");
  const [customExe, setCustomExe] = useState("");
  const [customCategory, setCustomCategory] = useState("dcc");
  const [quickLaunch, setQuickLaunch] = useState<QuickLaunchEntry[]>(() => loadQuickLaunch());
  const pinnedIds = useMemo(() => new Set(quickLaunch.map((entry) => entry.id)), [quickLaunch]);
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  const [projectFilter, setProjectFilter] = useState("");
  const [toolSources, setToolSources] = useState<ToolSource[]>(() => loadToolSources());
  const [gitUrl, setGitUrl] = useState("");
  const [localTools, setLocalTools] = useState<ToolManifest[]>([]);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [autoUpdate, setAutoUpdate] = useState(() => { try { return localStorage.getItem("vantadeck.autoUpdate") !== "false"; } catch { return true; } });
  const [appVersion, setAppVersion] = useState("0.4.12");
  const [autostart, setAutostart] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [drives, setDrives] = useState<string[]>([]);
  const [selectedDrives, setSelectedDrives] = useState<Set<string>>(new Set());
  const [scanDriveLabel, setScanDriveLabel] = useState("");
  const cancelScanRef = useRef(false);
  const [projectScanning, setProjectScanning] = useState(false);
  const [projectScanProgress, setProjectScanProgress] = useState<ScanProgress | null>(null);
  const [projectScanDriveLabel, setProjectScanDriveLabel] = useState("");
  const cancelProjectScanRef = useRef(false);
  const [onboarding, setOnboarding] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [setupDismissed, setSetupDismissed] = useState(() => readFlag("vantadeck.setupDismissed"));
  const [dragOver, setDragOver] = useState(false);
  const [selectedProject, setSelectedProject] = useState<ProjectTarget | null>(null);
  const undoStack = useRef<UndoEntry[]>([]);
  const redoStack = useRef<UndoEntry[]>([]);
  const navHistory = useRef<Array<{ screen: Screen; project: ProjectTarget | null }>>([{ screen: getPrefs().startScreen, project: null }]);
  const navIndex = useRef(0);
  const [nav, setNav] = useState({ canBack: false, canForward: false });
  const { preference, setPreference } = useTheme();
  const appearance = useAppearance();
  const [prefs] = usePrefs();
  const discovery = useDiscovery();
  useEngineActivityFeed();
  const engineActivity = useEngineActivity();
  const engineRunning = (prefs.engineWatch ? engineActivity.data ?? [] : []).filter((activity) => activity.status === "running");
  const projects = projectView === "pinned" ? pinnedProjects : recentProjects;
  const [categoryFilter, setCategoryFilter] = useState<string | null>(null);
  const homeCategories = useMemo(() => [...new Set([...pinnedProjects, ...recentProjects].map((project) => project.category).filter((value): value is string => Boolean(value)))].sort(), [pinnedProjects, recentProjects]);
  const filteredProjects = useMemo(
    () => projects.filter((project) => !categoryFilter || project.category === categoryFilter),
    [projects, categoryFilter],
  );

  const reloadDashboard = useCallback(() => {
    void loadDashboard().then((snapshot) => {
      setContinueProject(snapshot.continueProject);
      setPinnedProjects(snapshot.pinnedProjects);
      setRecentProjects(snapshot.recentProjects);
      setInstalledApps(snapshot.apps);
    }).catch(() => undefined);
  }, []);

  const loadHealth = useCallback(() => {
    if (!isNativeRuntime()) return;
    desktopApi.healthOverview().then(setHealthProjects).catch(() => undefined);
  }, []);

  useEffect(() => { reloadDashboard(); }, [reloadDashboard]);

  useEffect(() => {
    if (!isNativeRuntime()) return;
    const load = () => { desktopApi.recentActivity(6).then(setActivity).catch(() => undefined); };
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, []);

  useEffect(() => {
    if (!continueProject || !isNativeRuntime()) { setContinueFiles([]); return; }
    let active = true;
    desktopApi.recentFiles(continueProject.path, 4).then((files) => { if (active) setContinueFiles(files); }).catch(() => undefined);
    return () => { active = false; };
  }, [continueProject]);

  // Health: cached instantly, a quick pass on launch, a full pass shortly
  // after, and a debounced quick pass when the window regains focus.
  const lastHealthRefresh = useRef(0);
  useEffect(() => {
    loadHealth();
    if (!isNativeRuntime()) return;
    const refresh = (full: boolean) => {
      const now = Date.now();
      if (!full && now - lastHealthRefresh.current < 20000) return;
      lastHealthRefresh.current = now;
      desktopApi.refreshHealth(full).then(() => loadHealth()).catch(() => undefined);
    };
    refresh(false);
    const full = window.setTimeout(() => refresh(true), 2000);
    const onFocus = () => refresh(false);
    window.addEventListener("focus", onFocus);
    return () => { window.clearTimeout(full); window.removeEventListener("focus", onFocus); };
  }, [loadHealth]);

  // The browser demo can preview first-run setup with ?demo=true&onboarding=1.
  useEffect(() => {
    if (isDemoMode() && new URLSearchParams(window.location.search).get("onboarding") === "1") setOnboarding(true);
  }, []);

  useEffect(() => {
    if (!isNativeRuntime()) return;
    desktopApi.appVersion().then(setAppVersion).catch(() => undefined);
    desktopApi.autostartEnabled().then(setAutostart).catch(() => undefined);
    desktopApi.listDrives().then((found) => { setDrives(found); setSelectedDrives(new Set(found)); }).catch(() => undefined);
    if (autoUpdate) {
      desktopApi.checkForUpdate().then((info) => { if (info.available) setUpdate(info); }).catch(() => undefined);
    }
    if (!readFlag("vantadeck.onboarded")) setOnboarding(true);
    // Refresh "Found on this computer" in the background; the cached list shows instantly.
    if (getPrefs().discoverOnLaunch) window.setTimeout(() => { void discovery.refresh().catch(() => undefined); }, 1500);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function setAutoUpdatePref(enabled: boolean) {
    setAutoUpdate(enabled);
    writeFlag("vantadeck.autoUpdate", enabled);
    if (enabled && isNativeRuntime()) desktopApi.checkForUpdate().then((info) => { if (info.available) setUpdate(info); }).catch(() => undefined);
  }
  function toggleAutostart(enabled: boolean) {
    setAutostart(enabled);
    void run(enabled ? "Enabling launch on startup" : "Disabling launch on startup", async () => {
      await desktopApi.setAutostart(enabled);
      setAutostart(await desktopApi.autostartEnabled());
    });
  }
  function checkForUpdates() {
    void run("Checking for updates", async () => {
      const info = await desktopApi.checkForUpdate();
      setUpdate(info);
      toast.message(info.available ? `Update ${info.version} is available.` : "You're on the latest version.");
    }, { quiet: true });
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
      const target = event.target as HTMLElement | null;
      const typing = !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT" || target.isContentEditable);
      if (meta && key === "k") { event.preventDefault(); setPaletteOpen((open) => !open); }
      else if (meta && key === "n") { event.preventDefault(); setAddProjectOpen(true); }
      else if (meta && key === ",") { event.preventDefault(); openScreen("Settings"); }
      else if (meta && /^[1-8]$/.test(key)) { event.preventDefault(); openScreen(navigation[Number(key) - 1][0]); }
      else if (meta && key === "z" && !event.shiftKey && !typing) { event.preventDefault(); void undoLast(); }
      else if (meta && (key === "y" || (key === "z" && event.shiftKey)) && !typing) { event.preventDefault(); void redoLast(); }
      else if (event.altKey && key === "arrowleft") { event.preventDefault(); goBack(); }
      else if (event.altKey && key === "arrowright") { event.preventDefault(); goForward(); }
      else if (event.key === "?" && !typing && !meta) { event.preventDefault(); setShortcutsOpen(true); }
    };
    const onMouse = (event: MouseEvent) => {
      if (event.button === 3) { event.preventDefault(); goBack(); }
      else if (event.button === 4) { event.preventDefault(); goForward(); }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mouseup", onMouse);
    return () => { window.removeEventListener("keydown", onKey); window.removeEventListener("mouseup", onMouse); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function run(label: string, action: () => Promise<unknown>, options: { quiet?: boolean } = {}) {
    try {
      await action();
      if (!options.quiet) toast.success(`${label}: done`);
      return true;
    } catch (error) {
      toast.error(`${label} failed`, { description: error instanceof Error ? error.message : String(error) });
      return false;
    }
  }

  // Scans run as passes (each drive on Windows, or the standard install
  // locations on macOS/Linux, then any extra folders from Settings) so a huge
  // drive can't stall the whole scan; the loop is cancellable between passes.
  async function runScanPasses(passes: Array<{ label: string; roots: string[] }>) {
    if (scanning) return;
    if (!isNativeRuntime()) { toast.message("Scanning is available in the desktop app."); return; }
    setScanning(true);
    cancelScanRef.current = false;
    setScanProgress({ completed: 0, total: 0, current: "", done: false });
    const unsubscribe = await onScanProgress(setScanProgress);
    try {
      for (let i = 0; i < passes.length; i++) {
        if (cancelScanRef.current) break;
        const pass = passes[i];
        setScanDriveLabel(passes.length > 1 ? `${pass.label} (${i + 1}/${passes.length})` : pass.label);
        setScanProgress({ completed: 0, total: 0, current: "", done: false });
        try {
          await desktopApi.scanApps(pass.roots);
        } catch (error) {
          if (cancelScanRef.current) break;
          toast.error(`Couldn't scan ${pass.label}`, { description: error instanceof Error ? error.message : String(error) });
        }
        await invalidate.apps();
      }
      reloadDashboard();
      if (!cancelScanRef.current) toast.success(passes.length === 1 ? `Scanned ${passes[0].label}.` : `Scanned ${passes.length} locations.`);
    } finally {
      unsubscribe();
      setScanning(false);
      setScanProgress(null);
      setScanDriveLabel("");
    }
  }

  async function scanDrives() {
    if (drives.length && !drives.some((drive) => selectedDrives.has(drive))) { toast.message("Select at least one drive to scan."); return; }
    const passes = drives.length
      ? drives.filter((drive) => selectedDrives.has(drive)).map((drive) => ({ label: drive, roots: [drive] }))
      : [{ label: "standard app locations", roots: [] as string[] }];
    const extra = getPrefs().extraScanFolders.split(";").map((value) => value.trim()).filter(Boolean);
    await runScanPasses([...passes, ...extra.map((folder) => ({ label: folder, roots: [folder] }))]);
  }

  async function scanFolder() {
    const folder = await browsePath({ directory: true, title: "Choose a folder to scan for apps" });
    if (folder) await runScanPasses([{ label: folder, roots: [folder] }]);
  }

  function cancelDriveScan() {
    cancelScanRef.current = true;
    void desktopApi.cancelScan();
  }

  async function findProjects() {
    openScreen("Projects");
    try {
      await discovery.refresh();
    } catch (error) {
      toast.error("Couldn't look for projects", { description: error instanceof Error ? error.message : String(error) });
    }
  }

  // Deep scan (Windows drives) for folders that already carry Pipeline OS settings.
  async function scanForProjects() {
    if (projectScanning) return;
    if (!isNativeRuntime()) { toast.message("Scanning is available in the desktop app."); return; }
    const targets = drives.filter((drive) => selectedDrives.has(drive));
    if (!targets.length) { toast.message("Select at least one drive to scan."); return; }
    setProjectScanning(true);
    cancelProjectScanRef.current = false;
    setProjectScanProgress({ completed: 0, total: 0, current: "", done: false });
    const unsubscribe = await onProjectScanProgress(setProjectScanProgress);
    let foundCount = 0;
    try {
      for (let i = 0; i < targets.length; i++) {
        if (cancelProjectScanRef.current) break;
        const drive = targets[i];
        setProjectScanDriveLabel(`${drive} (${i + 1}/${targets.length})`);
        setProjectScanProgress({ completed: 0, total: 0, current: "", done: false });
        try {
          const found = await desktopApi.scanProjects([drive]);
          foundCount += found.length;
        } catch (error) {
          if (cancelProjectScanRef.current) break;
          toast.error(`Couldn't scan ${drive}`, { description: error instanceof Error ? error.message : String(error) });
        }
        await invalidate.projects();
        reloadDashboard();
      }
      if (!cancelProjectScanRef.current) {
        toast.success(foundCount ? `Found and added ${foundCount} project${foundCount === 1 ? "" : "s"}.` : "No new projects found on the scanned drives.");
      }
    } finally {
      unsubscribe();
      setProjectScanning(false);
      setProjectScanProgress(null);
      setProjectScanDriveLabel("");
    }
  }

  function cancelProjectScan() {
    cancelProjectScanRef.current = true;
    void desktopApi.cancelProjectScan();
  }

  function toggleDrive(drive: string) {
    setSelectedDrives((prev) => {
      const next = new Set(prev);
      if (next.has(drive)) next.delete(drive); else next.add(drive);
      return next;
    });
  }

  function completeOnboarding(prefs: OnboardingPrefs) {
    writeFlag("vantadeck.onboarded", true);
    setPreference(prefs.theme);
    setOnboarding(false);
    navigate("Applications");
    if (prefs.scanRoots.trim()) setPrefs({ extraScanFolders: prefs.scanRoots.trim() });
    if (isNativeRuntime()) void scanDrives();
  }
  function skipOnboarding() {
    writeFlag("vantadeck.onboarded", true);
    setOnboarding(false);
  }

  function applyNavEntry(entry: { screen: Screen; project: ProjectTarget | null }) {
    setActiveScreen(entry.screen);
    setSelectedProject(entry.project);
    setNav({ canBack: navIndex.current > 0, canForward: navIndex.current < navHistory.current.length - 1 });
  }
  function navigate(screen: Screen, project: ProjectTarget | null = null) {
    const current = navHistory.current[navIndex.current];
    if (current && current.screen === screen && current.project?.path === project?.path && current.project?.tab === project?.tab) {
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

  // Downloads in the background; the header's update icon shows progress and
  // offers "Install & restart" (or the installer) when it's ready.
  async function startUpdate() {
    if (!update?.available || !isNativeRuntime()) return;
    try {
      await desktopApi.updateDownload();
      toast.message(`Downloading Pipeline OS ${update.version}`, { description: "Follow progress with the download icon at the top right." });
    } catch (error) {
      toast.error("Couldn't start the update", { description: error instanceof Error ? error.message : String(error) });
    }
  }

  function openProject(target: { path: string; name: string }, tab?: string) {
    if (isNativeRuntime()) desktopApi.recordProjectOpened(target.path).catch(() => undefined);
    navigate("Project", { path: target.path, name: target.name, tab });
  }

  // Opens a project in its engine. With several installed versions and none
  // saved yet, asks once (and remembers); otherwise opens directly.
  async function requestOpenInEngine(target: { path: string; name: string }) {
    if (!isNativeRuntime()) { toast.message(`Opening ${target.name} in its engine works in the desktop app.`); return; }
    const choice = await desktopApi.engineOptions(target.path).catch(() => null);
    if (!choice) { toast.message("This project isn't connected to an engine yet.", { description: "Link one from the project's Overview tab.", action: { label: "Open project", onClick: () => openProject(target) } }); return; }
    if (choice.options.length === 0) { toast.error("No installed version of this engine was found.", { description: "Scan for applications, or set a version override on the Applications screen." }); return; }
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

  async function importProject(path: string, name: string): Promise<boolean> {
    if (!isNativeRuntime()) { toast.message("Adding projects works in the desktop app."); return false; }
    const ok = await run(`Adding ${name || path.split(/[\\/]/).pop()}`, async () => {
      await desktopApi.importProject(path, name);
      await invalidate.projects();
      reloadDashboard();
    });
    if (ok) openProject({ path, name: name || path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path });
    return ok;
  }

  async function importDroppedFolder(path: string) {
    const info = await desktopApi.pathInfo(path).catch(() => null);
    if (!info?.isDir) { toast.error("Drop a project folder to add it."); return; }
    const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
    if (!(await confirmAction({ title: `Add "${name}" as a project?`, description: path, confirmLabel: "Add project" }))) return;
    await importProject(path, name);
  }

  useEffect(() => {
    if (!isNativeRuntime()) return;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload as { type: string; paths?: string[] };
        // Only files dragged in from the OS count; dragging cards or text
        // inside the app must never raise the "drop a folder" overlay.
        if (payload.type === "enter") setDragOver(Boolean(payload.paths?.length));
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
    setAddAppOpen(false);
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
      adding ? `Pinned ${entry.name} to Quick Launch` : `Removed ${entry.name} from Quick Launch`,
      async () => applyQuickLaunch(next),
      async () => applyQuickLaunch(previous),
    );
  }

  // Pinned Quick Launch entries resolved to launchable items; until the user
  // pins anything, the first few detected apps are shown.
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
    void run(`Launching ${item.name}`, () => item.custom ? desktopApi.launchExecutable(item.executable!) : desktopApi.launchApp(item.appId, item.executable!), { quiet: true });
  }

  const allProjects = useMemo(() => {
    const map = new Map<string, { name: string; path: string; engineId?: string }>();
    [...pinnedProjects, ...recentProjects].forEach((p) => map.set(p.path, { name: p.name, path: p.path, engineId: p.engineId }));
    registeredProjects.forEach((p) => { if (!map.has(p.path)) map.set(p.path, { name: p.name, path: p.path }); });
    return [...map.values()];
  }, [pinnedProjects, recentProjects, registeredProjects]);

  const runtimeLabel = isNativeRuntime() ? "Desktop app" : isDemoMode() ? "Browser demo" : "Browser preview";
  const projectActions: ProjectActions = {
    onOpen: (p, tab) => openProject({ path: p.path, name: p.name }, tab),
    onLaunch: (p) => void requestOpenInEngine(p),
    onOpenFolder: (p) => { if (isNativeRuntime()) void run("Opening folder", () => desktopApi.openPath(p.path), { quiet: true }); },
    onCopyPath: (p) => { void navigator.clipboard?.writeText(p.path); toast.success("Path copied."); },
    onAddProject: () => setAddProjectOpen(true),
  };
  const driveSummary = drives.length === 0 ? "your drives" : drives.filter((d) => selectedDrives.has(d)).length === drives.length ? `all ${drives.length} drives` : drives.filter((d) => selectedDrives.has(d)).join(", ");

  const drivePicker = (disabled: boolean) => drives.length === 0 ? null : (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" disabled={disabled} className="gap-2">
          <HardDrive size={15} />
          {(() => {
            const count = drives.filter((drive) => selectedDrives.has(drive)).length;
            if (count === 0) return "No drives selected";
            if (count === drives.length) return `All drives (${count})`;
            return drives.filter((drive) => selectedDrives.has(drive)).join(", ");
          })()}
          <ChevronDown size={15} className="opacity-70" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuLabel>Drives to scan</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {drives.map((drive) => (
          <DropdownMenuCheckboxItem key={drive} checked={selectedDrives.has(drive)} onCheckedChange={() => toggleDrive(drive)} onSelect={(event) => event.preventDefault()}>
            {drive}
          </DropdownMenuCheckboxItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => setSelectedDrives(new Set(drives))}>Select all</DropdownMenuItem>
        <DropdownMenuItem onClick={() => setSelectedDrives(new Set())}>Clear selection</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  // ------------------------------------------------------------ screens

  const homeScreen = (() => {
    const appsFound = installedApps.length > 0;
    const projectsAdded = pinnedProjects.length + recentProjects.length + registeredProjects.length > 0;
    const showSetup = !setupDismissed && !(appsFound && projectsAdded && quickLaunch.length > 0);
    const continueHealth = continueProject ? healthProjects.find((entry) => entry.path === continueProject.path) : undefined;
    return (
      <div className="pos-enter space-y-6">
        {showSetup ? (
          <GettingStarted
            appsFound={appsFound}
            projectsAdded={projectsAdded}
            pinned={quickLaunch.length > 0}
            onScan={() => { openScreen("Applications"); void scanDrives(); }}
            onAddProject={() => setAddProjectOpen(true)}
            onPin={() => openScreen("Applications")}
            onDismiss={() => { setSetupDismissed(true); writeFlag("vantadeck.setupDismissed", true); }}
          />
        ) : null}

        <section aria-labelledby="continue-title">
          <SectionLabel><span id="continue-title">Continue where you left off</span></SectionLabel>
          {continueProject ? (
            <Card className="overflow-hidden"><CardContent className="grid gap-0 p-0 lg:grid-cols-[240px_minmax(0,1fr)_280px]">
              {isDemoMode() ? <img src={voidlineImage} alt="" className="h-full max-h-60 w-full object-cover" /> : <ProjectThumb projectPath={continueProject.path} thumbnail={continueProject.thumbnail} className="h-full max-h-60 w-full rounded-none" iconSize={48} alt="" />}
              <div className="min-w-0 space-y-3 p-5">
                <div>
                  <h1 className="truncate text-2xl font-semibold tracking-tight">{continueProject.name}</h1>
                  <p className="truncate text-sm text-muted-foreground" title={continueProject.path}>{continueProject.path}</p>
                </div>
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
                  {continueProject.category ? <span className={cn("rounded-full px-2 py-0.5 text-[11px] font-medium", categoryClass(continueProject.category))}>{continueProject.category}</span> : null}
                  <span className="flex items-center gap-1.5"><AppIcon executable={continueProject.engineExecutable ?? undefined} size={16} /> {continueProject.engine}{continueProject.version ? ` ${continueProject.version}` : ""}</span>
                  {continueProject.branch ? <span className="flex items-center gap-1.5"><GitBranch size={13} /> {continueProject.branch}</span> : null}
                  {formatLastOpened(continueProject.lastOpened) ? <span>Opened {formatLastOpened(continueProject.lastOpened)}</span> : null}
                </div>
                {continueFiles.length ? <div><div className="mb-1 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Recently changed</div><ul className="space-y-0.5">{continueFiles.map((file) => (
                  <li key={file.path} className="group flex items-center gap-2 rounded-md px-1 py-1 text-sm hover:bg-muted/40">
                    <FileCode2 size={14} className="shrink-0 text-muted-foreground" />
                    <button className="min-w-0 flex-1 truncate text-left" title={file.path} onClick={() => void run("Opening file", () => desktopApi.openPath(file.path), { quiet: true })}>{file.name}</button>
                    <span className="shrink-0 text-xs text-muted-foreground">{timeAgo(file.modified)}</span>
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-6 w-6 shrink-0 opacity-60 group-hover:opacity-100" aria-label={`Actions for ${file.name}`}><MoreHorizontal size={15} /></Button></DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="w-52">
                        <DropdownMenuItem onClick={() => void run("Opening file", () => desktopApi.openPath(file.path), { quiet: true })}><FileCode2 size={14} /> Open file</DropdownMenuItem>
                        <DropdownMenuItem onClick={() => { const dir = file.path.replace(/[\\/][^\\/]*$/, ""); if (dir) void run("Opening folder", () => desktopApi.openPath(dir), { quiet: true }); }}><FolderOpen size={14} /> Show in folder</DropdownMenuItem>
                        <DropdownMenuItem onClick={() => { void navigator.clipboard?.writeText(file.path); toast.success("Path copied."); }}><Clipboard size={14} /> Copy path</DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </li>
                ))}</ul></div> : null}
              </div>
              <div className="space-y-3 border-t border-border p-5 lg:border-l lg:border-t-0">
                <Button className="w-full" disabled={!continueProject.engineId && !isDemoMode()} onClick={() => void requestOpenInEngine(continueProject)}><Play size={15} /> Open in {continueProject.engine || "engine"}</Button>
                <Button className="w-full" variant="outline" onClick={() => openProject({ path: continueProject.path, name: continueProject.name })}><LayoutDashboard size={15} /> Project workspace</Button>
                <div>
                  <h2 className="mb-1.5 text-sm font-semibold">Health</h2>
                  {!continueHealth || !continueHealth.checkedAt ? <p className="text-sm text-muted-foreground">{isNativeRuntime() ? "Checking…" : "Available in the desktop app."}</p>
                    : !continueHealth.issues.length ? <p className="text-sm text-success">All checks passed.</p>
                    : <div className="space-y-1.5">{continueHealth.issues.slice(0, 3).map((issue, index) => <div key={`${issue.code}-${index}`} className="flex items-start gap-2 text-sm"><span className={cn("mt-1.5 h-2 w-2 shrink-0 rounded-full", issue.severity === "error" ? "bg-destructive" : "bg-warning")} /><span className="min-w-0" title={issue.detail}><strong className="block truncate font-medium">{issue.title}</strong><small className="line-clamp-2 text-muted-foreground">{issue.detail}</small></span></div>)}</div>}
                </div>
              </div>
            </CardContent></Card>
          ) : (
            <Card><CardContent className="p-0">
              <EmptyState icon={FolderPlus} title="Add a project to get started" description="Point Pipeline OS at a Unity, Unreal, Godot, Blender, or any creative project folder. You can also drag a folder onto this window." action={<Button onClick={() => setAddProjectOpen(true)}><FolderPlus size={15} /> Add project</Button>} />
            </CardContent></Card>
          )}
        </section>

        <section className="grid gap-5 xl:grid-cols-[minmax(0,1fr)_340px]">
          <div className="min-w-0 space-y-3">
            <div className="flex flex-wrap items-center gap-2">
              <div className="flex rounded-lg bg-secondary/60 p-0.5" role="tablist" aria-label="Project lists">
                <button role="tab" aria-selected={projectView === "pinned"} onClick={() => setProjectView("pinned")} className={cn("rounded-md px-3 py-1.5 text-sm transition-colors", projectView === "pinned" ? "bg-background font-medium shadow-xs" : "text-muted-foreground hover:text-foreground")}>Pinned Projects</button>
                <button role="tab" aria-selected={projectView === "recent"} onClick={() => setProjectView("recent")} className={cn("rounded-md px-3 py-1.5 text-sm transition-colors", projectView === "recent" ? "bg-background font-medium shadow-xs" : "text-muted-foreground hover:text-foreground")}>Recent Projects</button>
              </div>
              {homeCategories.length ? <select aria-label="Filter by category" value={categoryFilter ?? ""} onChange={(event) => setCategoryFilter(event.target.value || null)} className={cn(selectClass, "ml-auto h-8 text-muted-foreground")}><option value="">All categories</option>{homeCategories.map((category) => <option key={category} value={category}>{category}</option>)}</select> : null}
            </div>
            <ProjectTable projects={filteredProjects} actions={projectActions} emptyText={projectView === "pinned" ? "No pinned projects yet" : "No recent projects yet"} />
            <Button variant="link" className="px-0" onClick={() => openScreen("Projects")}>View all projects <ChevronRight size={16} /></Button>
          </div>
          <aside className="min-w-0 space-y-4">
            <ActiveTasksCard onOpenProject={(target) => openProject(target, "pipeline")} />
            <MyWorkCard onOpenProject={(target, tab) => openProject(target, tab)} />
            <Card><CardContent className="p-4">
              <div className="mb-2 flex items-center justify-between text-sm font-semibold">Project health <Button variant="ghost" size="sm" className="h-auto p-0 text-muted-foreground" onClick={() => openScreen("Health")}>View all</Button></div>
              {healthProjects.length ? <div className="space-y-0.5">{healthProjects.slice(0, 6).map((entry) => {
                const errors = entry.issues.filter((issue) => issue.severity === "error").length;
                const warnings = entry.issues.length - errors;
                return <button key={entry.path} onClick={() => openProject({ path: entry.path, name: entry.name })} className="flex w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-sm hover:bg-muted/50">
                  <span className="min-w-0 flex-1 truncate">{entry.name}</span>
                  {!entry.checkedAt ? <Loader2 size={13} className="shrink-0 animate-spin text-muted-foreground" aria-label="Checking" />
                    : errors ? <Badge variant="outline" className="shrink-0 border-destructive/50 text-destructive">{errors} error{errors > 1 ? "s" : ""}</Badge>
                    : warnings ? <Badge variant="outline" className="shrink-0 border-warning/50 text-warning">{warnings} warning{warnings > 1 ? "s" : ""}</Badge>
                    : <Badge variant="secondary" className="shrink-0 text-success">Healthy</Badge>}
                </button>;
              })}</div> : <p className="px-1.5 py-2 text-sm text-muted-foreground">{isNativeRuntime() ? "Health appears once you add projects." : "Health checks run in the desktop app."}</p>}
            </CardContent></Card>
            <Card><CardContent className="p-4">
              <div className="mb-2 flex items-center justify-between text-sm font-semibold">Installed apps <Button variant="ghost" size="sm" className="h-auto p-0 text-muted-foreground" onClick={() => openScreen("Applications")}>Manage</Button></div>
              <div className="space-y-0.5">{installedApps.length ? installedApps.map((app) => <button key={app.id} onClick={() => openScreen("Applications")} title={`Manage ${app.name} versions`} className="flex w-full items-center gap-2.5 rounded-lg px-2 py-1.5 text-left text-sm hover:bg-muted/50"><span className="flex h-8 w-8 items-center justify-center rounded-md bg-secondary"><AppIcon executable={app.executable ?? undefined} size={18} /></span><span className="min-w-0 flex-1"><strong className="block truncate font-medium">{app.name}</strong><small className="block truncate text-xs text-muted-foreground">{[...new Set(app.versions.map(formatVersion))].join(", ")}</small></span><ChevronRight size={15} className="text-muted-foreground" /></button>)
                : <p className="px-1.5 py-2 text-sm text-muted-foreground">No apps detected yet. <button className="text-primary hover:underline" onClick={() => { openScreen("Applications"); void scanDrives(); }}>Scan now</button></p>}</div>
            </CardContent></Card>
            {activity.length ? <Card><CardContent className="p-4"><div className="mb-2 flex items-center justify-between text-sm font-semibold">Recent activity <Button variant="ghost" size="sm" className="h-auto p-0 text-muted-foreground" onClick={() => openScreen("Activity")}>View all</Button></div><ul className="space-y-1.5">{activity.map((entry) => (
              <li key={entry.id} className="flex items-start gap-2 text-sm"><Activity size={14} className="mt-0.5 shrink-0 text-muted-foreground" /><span className="min-w-0 flex-1"><span className="block truncate">{entry.message}</span><span className="block text-xs text-muted-foreground">{formatLastOpened(entry.createdAt)}</span></span></li>
            ))}</ul></CardContent></Card> : null}
          </aside>
        </section>
      </div>
    );
  })();

  const projectsScreen = (() => {
    const allTags = [...new Set(registeredProjects.flatMap((p) => p.tags))].sort();
    const term = projectFilter.trim().toLowerCase();
    const shown = registeredProjects.filter((p) => (!tagFilter || p.tags.includes(tagFilter)) && (!term || `${p.name} ${p.path} ${p.tags.join(" ")} ${p.category}`.toLowerCase().includes(term)));
    return (
      <section className="pos-enter space-y-5">
        <PageHeader title="Projects" description="Every project on this machine. Open one for its workspace: source control, pipeline, board, assets, and AI." actions={<>
          <Button variant="outline" onClick={() => void findProjects()} disabled={discovery.refreshing}>{discovery.refreshing ? <Loader2 size={15} className="animate-spin" /> : <ScanSearch size={15} />} Find projects</Button>
          <Button onClick={() => setAddProjectOpen(true)}><FolderPlus size={15} /> Add project</Button>
        </>} />
        {projectScanning ? <div className="flex items-center justify-between gap-3 rounded-lg border bg-secondary/40 px-3 py-2 text-sm">
          <span className="flex min-w-0 items-center gap-2"><RefreshCw size={14} className="shrink-0 animate-spin" /><span className="truncate">{projectScanDriveLabel ? `Scanning ${projectScanDriveLabel}${projectScanProgress?.current ? ` — ${projectScanProgress.current}` : "…"}` : "Scanning…"}</span></span>
          <Button variant="outline" size="sm" onClick={cancelProjectScan}>Cancel</Button>
        </div> : null}
        <DiscoveredProjects onAdded={() => { void invalidate.projects(); reloadDashboard(); }} />
        <div className="flex flex-wrap items-center gap-2">
          <div className="relative min-w-56 flex-1">
            <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" aria-hidden />
            <Input aria-label="Filter projects" placeholder="Filter by name, path, tag…" value={projectFilter} onChange={(event) => setProjectFilter(event.target.value)} className="h-9 pl-8" />
          </div>
          {allTags.length ? <div className="flex flex-wrap items-center gap-1.5" role="group" aria-label="Filter by tag">
            <button onClick={() => setTagFilter(null)} aria-pressed={!tagFilter} className={cn("rounded-full px-2.5 py-1 text-xs", !tagFilter ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>All</button>
            {allTags.map((tag) => <button key={tag} onClick={() => setTagFilter(tag)} aria-pressed={tagFilter === tag} className={cn("rounded-full px-2.5 py-1 text-xs", tagFilter === tag ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>{tag}</button>)}
          </div> : null}
        </div>
        {shown.length ? <div className="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">{shown.map((project) => (
          <Card key={project.path} role="button" tabIndex={0} aria-label={`Open ${project.name}`} className="cursor-pointer transition-colors hover:border-primary/50" onClick={() => openProject({ path: project.path, name: project.name })} onKeyDown={(event) => { if (event.key === "Enter" && event.target === event.currentTarget) openProject({ path: project.path, name: project.name }); }}>
            <CardContent className="flex items-start gap-3 p-4">
              <ProjectThumb projectPath={project.path} className="h-12 w-16 shrink-0" alt="" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2"><h3 className="truncate font-medium">{project.name}</h3>{project.pinned ? <Pin size={12} className="shrink-0 text-primary" aria-label="Pinned" /> : null}{project.category ? <span className={cn("shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium", categoryClass(project.category))}>{project.category}</span> : null}</div>
                <p className="truncate text-xs text-muted-foreground" title={project.path}>{project.path}</p>
                {project.tags.length ? <div className="mt-1.5 flex flex-wrap gap-1">{project.tags.map((tag) => <span key={tag} className="rounded-full bg-secondary px-2 py-0.5 text-[10px] text-muted-foreground">{tag}</span>)}</div> : null}
                <div className="mt-3 flex items-center gap-1.5" onClick={(event) => event.stopPropagation()}>
                  <Button size="sm" onClick={() => void requestOpenInEngine(project)}><Play size={13} /> Launch</Button>
                  <Button size="sm" variant="outline" onClick={() => openProject({ path: project.path, name: project.name })}>Open</Button>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="ml-auto h-8 w-8" aria-label={`More actions for ${project.name}`}><MoreHorizontal size={16} /></Button></DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-52">
                      <DropdownMenuItem onClick={() => void performUndoable(project.pinned ? `Unpinned ${project.name}` : `Pinned ${project.name}`, async () => { await desktopApi.pinProject(project.path, !project.pinned); await invalidate.projects(); reloadDashboard(); }, async () => { await desktopApi.pinProject(project.path, project.pinned); await invalidate.projects(); reloadDashboard(); })}>{project.pinned ? <><PinOff size={14} /> Unpin from Home</> : <><Pin size={14} /> Pin to Home</>}</DropdownMenuItem>
                      <DropdownMenuItem onClick={() => openProject({ path: project.path, name: project.name }, "pipeline")}><Workflow size={14} /> Pipeline tasks</DropdownMenuItem>
                      <DropdownMenuItem onClick={() => projectActions.onOpenFolder({ ...project, engine: "", version: "", branch: "", lastOpened: "" })}><FolderOpen size={14} /> Show in folder</DropdownMenuItem>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem variant="destructive" onClick={() => void (async () => {
                        if (!(await confirmAction({ title: `Remove "${project.name}" from Pipeline OS?`, description: "This only unregisters it. Your files and its .pipelineos settings stay on disk, and you can add it back any time.", confirmLabel: "Remove", destructive: true }))) return;
                        void run(`Removing ${project.name}`, async () => { await desktopApi.removeProject(project.path); await invalidate.projects(); reloadDashboard(); });
                      })()}><Trash2 size={14} /> Remove from list</DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </div>
              </div>
            </CardContent>
          </Card>
        ))}</div> : registeredProjects.length ? (
          <EmptyState compact icon={Search} title="No projects match" action={<Button variant="outline" size="sm" onClick={() => { setProjectFilter(""); setTagFilter(null); }}>Clear filters</Button>} />
        ) : (
          <Card><CardContent className="p-0"><EmptyState icon={FolderPlus} title={isNativeRuntime() ? "No projects yet" : "Projects appear in the desktop app"} description="Add a project folder, or let Pipeline OS find the Unity, Unreal, and Godot projects already on this computer." action={<><Button onClick={() => setAddProjectOpen(true)}><FolderPlus size={15} /> Add project</Button><Button variant="outline" onClick={() => void findProjects()}><ScanSearch size={15} /> Find projects</Button></>} /></CardContent></Card>
        )}
      </section>
    );
  })();

  const applicationsScreen = (
    <section className="pos-enter space-y-5">
      <PageHeader title="Applications" description="Every installed engine, DCC, and editor, with each version side by side. Star the versions you use most for Quick Launch." actions={<>
        {drivePicker(scanning)}
        <Button variant="outline" disabled={scanning} aria-busy={scanning} onClick={() => void scanDrives()} title={drives.length ? "Scan the selected drives' install folders" : "Scan standard install locations"}>{scanning ? <><Loader2 size={15} className="animate-spin" /> Scanning…</> : <><ScanSearch size={15} /> Scan</>}</Button>
        <Button variant="outline" disabled={scanning} onClick={() => void scanFolder()}><FolderOpen size={15} /> Scan a folder…</Button>
        <Button onClick={() => setAddAppOpen(true)}><Plus size={15} /> Add app</Button>
      </>} />
      {scanning ? <Card><CardContent className="space-y-2 p-4">
        <div className="flex items-center justify-between gap-3 text-sm text-muted-foreground">
          <span className="flex min-w-0 items-center gap-2"><RefreshCw size={14} className="shrink-0 animate-spin" /><span className="truncate">{scanDriveLabel ? `Scanning ${scanDriveLabel}${scanProgress?.current ? ` — ${scanProgress.current}` : "…"}` : "Scanning…"}</span></span>
          <span className="flex shrink-0 items-center gap-3 tabular-nums">{scanProgress && scanProgress.total ? `${scanProgress.completed}/${scanProgress.total}` : ""}<Button variant="outline" size="sm" onClick={cancelDriveScan}>Cancel</Button></span>
        </div>
        <Progress value={scanProgress && scanProgress.total ? (scanProgress.completed / scanProgress.total) * 100 : 8} />
      </CardContent></Card> : null}
      {CATEGORY_ORDER.filter((category) => managedApps.some((app) => app.category === category && app.installations.length > 0)).map((category) => (
        <section key={category} className="space-y-2">
          <SectionLabel>{APP_CATEGORY_LABELS[category] ?? category}</SectionLabel>
          <div className="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">{managedApps.filter((app) => app.category === category && app.installations.length > 0).map((app) => {
            const iconInstall = app.installations.find((item) => item.runnable) ?? app.installations[0];
            const launchTarget = app.installations.find((item) => item.runnable);
            return (
              <Card key={app.id}><CardContent className="flex items-start gap-3 p-4">
                <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-secondary"><AppIcon executable={iconInstall?.executable} size={26} /></span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center justify-between gap-2"><h3 className="truncate font-medium">{app.name}</h3><span className="flex shrink-0 items-center gap-1"><Button variant="ghost" size="icon" className="h-7 w-7" aria-label={`Add or override a version of ${app.name}`} title="Point a version at a specific executable" onClick={() => { setOverrideApp({ id: app.id, name: app.name }); setOverrideVersion(""); setOverrideExe(""); }}><Plus size={15} /></Button>{app.launchable && launchTarget ? <Button size="sm" onClick={() => void run(`Launching ${app.name}`, () => desktopApi.launchApp(app.id, launchTarget.executable), { quiet: true })}><Play size={13} /> Launch</Button> : null}</span></div>
                  {app.launchable ? <div className="mt-2 space-y-0.5">{app.installations.map((item) => {
                    const entry = { id: `${app.id}::${item.executable}`, appId: app.id, name: `${app.name} ${formatVersion(item.version)}`, executable: item.executable, version: item.version, custom: false };
                    const pinned = pinnedIds.has(entry.id);
                    return (
                      <div key={item.executable} className="flex items-center gap-1.5 rounded-md px-1 py-0.5 text-sm hover:bg-muted/40" title={item.executable}>
                        <Badge variant={item.runnable ? "secondary" : "outline"} className={item.runnable ? "" : "text-muted-foreground line-through"}>{formatVersion(item.version)}</Badge>
                        {item.runnable ? <>
                          <Button variant="ghost" size="sm" className="ml-auto h-7 px-2" onClick={() => void run(`Launching ${entry.name}`, () => desktopApi.launchApp(app.id, item.executable), { quiet: true })}><Play size={12} /> Launch</Button>
                          <Button variant="ghost" size="icon" className="h-7 w-7" aria-pressed={pinned} aria-label={pinned ? `Unpin ${entry.name} from Quick Launch` : `Pin ${entry.name} to Quick Launch`} onClick={() => toggleQuickLaunch(entry)}><Star size={14} className={pinned ? "fill-primary text-primary" : "text-muted-foreground"} /></Button>
                        </> : <span className="ml-auto text-xs text-muted-foreground">Incompatible build</span>}
                      </div>
                    );
                  })}</div> : <div className="mt-1.5 flex flex-wrap gap-1">{app.installations.map((item) => <Badge key={item.executable} variant="outline" className="text-muted-foreground">{formatVersion(item.version)}</Badge>)}</div>}
                  {!app.launchable ? <p className="mt-1.5 text-xs text-muted-foreground">Used by projects for version control.</p> : null}
                </div>
              </CardContent></Card>
            );
          })}</div>
        </section>
      ))}
      {customApps.length ? <section className="space-y-2">
        <SectionLabel>Your apps</SectionLabel>
        <div className="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">{customApps.map((app) => (
          <Card key={app.id}><CardContent className="flex items-start gap-3 p-4">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-secondary"><AppIcon executable={app.executable} size={26} /></span>
            <div className="min-w-0 flex-1">
              <div className="flex items-center justify-between gap-2"><h3 className="truncate font-medium">{app.name}</h3><span className="flex items-center gap-1"><Button variant="ghost" size="icon" className="h-7 w-7" aria-pressed={pinnedIds.has(app.id)} aria-label={pinnedIds.has(app.id) ? `Remove ${app.name} from Quick Launch` : `Add ${app.name} to Quick Launch`} onClick={() => toggleQuickLaunch({ id: app.id, appId: app.id, name: app.name, executable: app.executable, custom: true })}><Star size={14} className={pinnedIds.has(app.id) ? "fill-primary text-primary" : "text-muted-foreground"} /></Button><Button size="sm" onClick={() => void run(`Launching ${app.name}`, () => desktopApi.launchExecutable(app.executable), { quiet: true })}><Play size={13} /> Launch</Button></span></div>
              <p className="mt-1 truncate text-xs text-muted-foreground" title={app.executable}>{app.executable}</p>
              <Button variant="ghost" size="sm" className="mt-1 h-auto p-0 text-muted-foreground hover:text-destructive" onClick={() => removeCustomApp(app.id)}>Remove</Button>
            </div>
          </CardContent></Card>
        ))}</div>
      </section> : null}
      {managedApps.every((app) => app.installations.length === 0) && !customApps.length ? (
        <Card><CardContent className="p-0"><EmptyState icon={ScanSearch} title={isNativeRuntime() ? "No applications detected yet" : "Application detection runs in the desktop app"} description="Scan finds Unity, Unreal, Godot, Blender, Maya, Houdini, Substance, IDEs, and version-control tools across your drives, including every installed version." action={<><Button onClick={() => void scanDrives()}><ScanSearch size={15} /> Scan now</Button><Button variant="outline" onClick={() => setAddAppOpen(true)}><Plus size={15} /> Add an app manually</Button></>} /></CardContent></Card>
      ) : null}
      {managedApps.length ? <details className="rounded-lg border border-border p-4 text-sm">
        <summary className="cursor-pointer font-medium">Supported applications ({managedApps.length})</summary>
        <div className="mt-3 flex flex-wrap gap-1.5">{managedApps.map((app) => <Badge key={app.id} variant={app.installations.length ? "secondary" : "outline"} className={app.installations.length ? "" : "text-muted-foreground"}>{app.name}</Badge>)}</div>
      </details> : null}
    </section>
  );

  const toolsScreen = (() => {
    const allTools = [...tools.filter((tool) => tool.reviewState !== "withdrawn"), ...localTools];
    return (
      <section className="pos-enter space-y-5">
        <PageHeader title="Tools" description="Curated pipeline tools and scripts from sources you trust." />
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
        {allTools.length ? <div className="grid gap-3 sm:grid-cols-2 2xl:grid-cols-3">{allTools.map((tool) => (
          <Card key={tool.id}><CardContent className="flex items-start gap-3 p-4">
            <Wrench className="shrink-0 text-primary" />
            <div className="min-w-0 flex-1"><h3 className="font-medium">{tool.name}</h3><p className="text-sm text-muted-foreground">{tool.description}</p><small className="text-xs text-muted-foreground">{tool.reviewState} · {tool.license} · checked {tool.lastVerifiedAt}</small></div>
            <Button variant="outline" size="sm" onClick={() => { if (/^https?:\/\//i.test(tool.sourceUrl)) void openExternal(tool.sourceUrl); else toast.message("Source", { description: tool.sourceUrl }); }}>Source</Button>
          </CardContent></Card>
        ))}</div> : <Card><CardContent className="p-0"><EmptyState compact icon={Wrench} title="No tools yet" description="Add a local folder of tool manifests above to populate this list." /></CardContent></Card>}
      </section>
    );
  })();

  const settingsScreen = (
    <SettingsScreen
      preference={preference}
      setPreference={setPreference}
      appearance={appearance.appearance}
      onAppearanceChange={appearance.update}
      onAppearanceReset={appearance.reset}
      onScanApps={() => { openScreen("Applications"); void scanDrives(); }}
      onFindProjects={() => void findProjects()}
      appVersion={appVersion}
      channel={CHANNEL}
      update={update}
      autoUpdate={autoUpdate}
      onAutoUpdateChange={setAutoUpdatePref}
      onCheckUpdate={() => { if (isNativeRuntime()) checkForUpdates(); else toast.message("Updates are checked by the desktop app."); }}
      onInstallUpdate={() => void startUpdate()}
      autostart={autostart}
      autostartAvailable={isNativeRuntime()}
      onAutostartChange={toggleAutostart}
      runtimeLabel={runtimeLabel}
    />
  );

  let content: React.ReactNode;
  if (activeScreen === "Project" && selectedProject) {
    content = <ProjectDetail key={selectedProject.path} project={selectedProject} initialTab={selectedProject.tab} onBack={goBack} onRenamed={() => { void invalidate.projects(); reloadDashboard(); }} onOpenInEngine={requestOpenInEngine} onHealthChanged={loadHealth} />;
  } else if (activeScreen === "Projects") content = projectsScreen;
  else if (activeScreen === "Applications") content = applicationsScreen;
  else if (activeScreen === "Health") content = <div className="pos-enter"><HealthScreen projects={registeredProjects} onOpenProject={openProject} /></div>;
  else if (activeScreen === "Tools") content = toolsScreen;
  else if (activeScreen === "Activity") content = <ActivityScreen />;
  else if (activeScreen === "Planning") content = <PlanningScreen projects={allProjects} onOpenProject={(project, tab) => openProject(project, tab)} />;
  else if (activeScreen === "Settings") content = settingsScreen;
  else content = homeScreen;

  const paletteRun = (action: () => void) => () => { setPaletteOpen(false); action(); };

  return (
    <div className="app-shell flex h-screen overflow-hidden bg-background text-foreground">
      {dragOver ? <div className="pointer-events-none fixed inset-0 z-[100] flex items-center justify-center bg-background/80 backdrop-blur-sm"><div className="rounded-2xl border-2 border-dashed border-primary px-10 py-8 text-center"><FolderPlus size={36} className="mx-auto mb-2 text-primary" /><p className="text-lg font-semibold">Drop a folder to add it as a project</p></div></div> : null}
      <aside className={cn("flex flex-none flex-col gap-4 border-r border-sidebar-border bg-sidebar p-3", appearance.appearance.sidebarLabels ? "w-60" : "w-16 items-center")}>
        <div className="flex items-center justify-between px-2 pt-1">
          {appearance.appearance.sidebarLabels ? <BrandLockup /> : <BrandGlyph className="h-8 w-8 rounded-lg" />}
          {CHANNEL === "dev" ? <span className="rounded bg-warning/20 px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider text-warning" title="Dev channel build">Dev</span> : null}
        </div>
        <nav aria-label="Primary navigation" className="flex flex-col gap-0.5">
          {navigation.map(([label, Icon], index) => {
            const current = activeScreen === label || (label === "Projects" && activeScreen === "Project");
            return (
              <button key={label} onClick={() => openScreen(label)} aria-current={current ? "page" : undefined} title={`${label} (${keyLabel("Ctrl")}+${index + 1})`} aria-label={label} className={cn("relative flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors", current ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground" : "text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground")}>
                <Icon size={18} aria-hidden />{appearance.appearance.sidebarLabels ? <span>{label}</span> : null}
                {label === "Health" && health.length ? (appearance.appearance.sidebarLabels ? <Badge variant="secondary" className="ml-auto">{health.length}</Badge> : <span className="absolute right-1 top-1 size-2 rounded-full bg-warning" />) : null}
                {label === "Activity" && activeRuns.length + engineRunning.length ? (appearance.appearance.sidebarLabels ? <span className="ml-auto flex items-center gap-1 text-xs text-primary"><Loader2 size={12} className="animate-spin" />{activeRuns.length + engineRunning.length}</span> : <span className="absolute right-1 top-1 size-2 animate-pulse rounded-full bg-primary" />) : null}
              </button>
            );
          })}
        </nav>
        {appearance.appearance.sidebarLabels && !activeRuns.length && engineRunning.length ? (
          <button onClick={() => openScreen("Activity")} className="mx-1 rounded-lg border border-primary/30 bg-primary/10 p-2.5 text-left text-xs">
            <span className="flex items-center gap-1.5 font-medium text-primary"><Loader2 size={12} className="animate-spin" /> {engineRunning.length === 1 ? ACTIVITY_LABELS[engineRunning[0].kind] : `${engineRunning.length} editor jobs running`}</span>
            <span className="mt-0.5 block truncate text-muted-foreground">{engineRunning[0].projectName ?? (engineRunning[0].engine === "unreal" ? "Unreal Engine" : "Unity")}{engineRunning[0].progress != null ? ` · ${Math.round(engineRunning[0].progress)}%` : ""}</span>
          </button>
        ) : null}
        {appearance.appearance.sidebarLabels && activeRuns.length ? (
          <button onClick={() => openScreen("Activity")} className="mx-1 rounded-lg border border-primary/30 bg-primary/10 p-2.5 text-left text-xs">
            <span className="flex items-center gap-1.5 font-medium text-primary"><Loader2 size={12} className="animate-spin" /> {activeRuns.length === 1 ? activeRuns[0].taskName : `${activeRuns.length} tasks running`}</span>
            <span className="mt-0.5 block truncate text-muted-foreground">{activeRuns[0].projectName}{activeRuns[0].progress != null ? ` · ${Math.round(activeRuns[0].progress)}%` : ""}</span>
          </button>
        ) : null}
        {appearance.appearance.sidebarLabels ? <div className="mt-auto space-y-2 px-2 text-xs text-muted-foreground">
          <div className="flex items-center gap-1.5" title="No account, no telemetry. Your data stays on this machine."><Lock size={12} className="text-success" /> Local &amp; private</div>
          <div className="flex items-center justify-between"><span>v{appVersion}{CHANNEL === "dev" ? " · dev" : ""}</span><button className="flex items-center gap-1 hover:text-foreground" onClick={() => setShortcutsOpen(true)} title="Keyboard shortcuts (?)"><Keyboard size={13} /> Shortcuts</button></div>
        </div> : <button className="mt-auto rounded-lg p-2 text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground" onClick={() => setShortcutsOpen(true)} title="Keyboard shortcuts (?)" aria-label="Keyboard shortcuts"><Keyboard size={16} /></button>}
      </aside>

      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <header className="flex items-center gap-3 border-b border-border px-5 py-2.5">
          <div className="flex items-center gap-0.5">
            <Button variant="ghost" size="icon" aria-label="Back" title="Back (Alt+←)" disabled={!nav.canBack} onClick={goBack}><ArrowLeft size={18} /></Button>
            <Button variant="ghost" size="icon" aria-label="Forward" title="Forward (Alt+→)" disabled={!nav.canForward} onClick={goForward}><ArrowRight size={18} /></Button>
          </div>
          <button onClick={() => setPaletteOpen(true)} className="flex h-9 min-w-0 max-w-xl flex-1 items-center gap-2 rounded-lg border border-border bg-secondary/50 px-3 text-sm text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground" aria-label="Search and commands">
            <Search size={15} className="shrink-0" aria-hidden />
            <span className="truncate">Search projects, apps, and actions…</span>
            <span className="ml-auto flex shrink-0 gap-1"><Kbd>{keyLabel("Ctrl")}</Kbd><Kbd>K</Kbd></span>
          </button>
          <div className="ml-auto flex items-center gap-2">
            <UpdateIndicator update={update} releaseNotesUrl={CHANNEL === "dev" ? `${REPO_URL}/releases/tag/dev-latest` : `${REPO_URL}/releases/tag/v${update?.version ?? ""}`} />
            <Button size="sm" className="relative" onClick={() => setAddProjectOpen(true)} title={discovery.projects.length ? `Add project (Ctrl+N) · ${discovery.projects.length} found on this computer` : "Add project (Ctrl+N)"}>
              <FolderPlus size={15} /> Add project
              {discovery.projects.length ? <span className="absolute -right-1.5 -top-1.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-warning px-1 text-[10px] font-bold text-background" aria-label={`${discovery.projects.length} projects found`}>{discovery.projects.length}</span> : null}
            </Button>
          </div>
        </header>


        <div className="flex-1 overflow-y-auto p-5 lg:p-6">
          <div className="mx-auto max-w-[1600px]">{content}</div>
        </div>

        <footer className="flex items-center gap-1 overflow-x-auto border-t border-border px-5 py-2">
          <span className="mr-1 shrink-0 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Quick Launch</span>
          {quickLaunchItems.map((item) => <Button key={item.id} variant="ghost" size="sm" className="shrink-0" title={item.executable ? `Launch ${item.name}` : "Open Applications"} onClick={() => launchQuick(item)}><AppIcon executable={item.executable ?? undefined} size={18} /> {item.name}</Button>)}
          {quickLaunchItems.length === 0 ? <Button variant="ghost" size="sm" onClick={() => openScreen("Applications")}><Star size={16} /> Star apps to pin them here</Button> : null}
        </footer>

        <Dialog open={!!enginePicker} onOpenChange={(open) => { if (!open) setEnginePicker(null); }}>
          <DialogContent className="max-w-sm">
            <DialogHeader>
              <DialogTitle>Which version?</DialogTitle>
              <DialogDescription>Several versions are installed. Pick the one for {enginePicker?.name}. It's saved to the project, so teammates open the same version.</DialogDescription>
            </DialogHeader>
            <div className="space-y-1">{enginePicker?.options.map((option) => (
              <Button key={option.version} variant="outline" className="w-full justify-start" onClick={() => confirmEnginePick(option.version)}><Play size={13} /> {formatVersion(option.version)}</Button>
            ))}</div>
          </DialogContent>
        </Dialog>
        <Dialog open={!!overrideApp} onOpenChange={(open) => { if (!open) setOverrideApp(null); }}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Add a version of {overrideApp?.name}</DialogTitle>
              <DialogDescription>Point Pipeline OS at an installed copy it didn't find, or found wrongly. {currentPlatform() === "mac" ? "Pick the app in Applications; its version is read automatically." : "Its version is read automatically when possible."} This only affects this machine.</DialogDescription>
            </DialogHeader>
            <form className="space-y-3" onSubmit={(e) => { e.preventDefault(); const app = overrideApp; const version = overrideVersion.trim(); const exe = overrideExe.trim(); if (!app || !exe) return; setOverrideApp(null); void run("Saving version", async () => { await desktopApi.setManualOverride(app.id, version, exe); await invalidate.apps(); }); }}>
              <label className="block space-y-1 text-sm"><span className="font-medium">{platformExamples().executableLabel}</span><PathInput ariaLabel={platformExamples().executableLabel} directory={false} required placeholder={platformExamples().appExecutable} value={overrideExe} onChange={setOverrideExe} className="w-full" /></label>
              <label className="block space-y-1 text-sm"><span className="font-medium">Version <span className="font-normal text-muted-foreground">(optional)</span></span><Input aria-label="Version" placeholder="Read from the app when left empty, e.g. 4.2.3" value={overrideVersion} onChange={(e) => setOverrideVersion(e.target.value)} /></label>
              <DialogFooter><Button type="button" variant="ghost" onClick={() => setOverrideApp(null)}>Cancel</Button><Button type="submit" disabled={!isNativeRuntime() || !overrideExe.trim()}>Save version</Button></DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
        <Dialog open={addAppOpen} onOpenChange={setAddAppOpen}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Add an application</DialogTitle>
              <DialogDescription>For anything Pipeline OS doesn't detect on its own: in-house tools, portable apps, or niche software.</DialogDescription>
            </DialogHeader>
            <form className="space-y-3" onSubmit={addCustomApp}>
              <label className="block space-y-1 text-sm"><span className="font-medium">Name</span><Input aria-label="App name" required placeholder="e.g. Marmoset Toolbag" value={customName} onChange={(e) => setCustomName(e.target.value)} /></label>
              <label className="block space-y-1 text-sm"><span className="font-medium">Category</span>
                <select aria-label="App category" value={customCategory} onChange={(e) => setCustomCategory(e.target.value)} className={cn(selectClass, "w-full")}>
                  <option value="game-engine">Game engine</option><option value="dcc">3D / DCC</option><option value="art">2D / Art</option><option value="code">Code / IDE</option><option value="utility">Utility</option>
                </select>
              </label>
              <label className="block space-y-1 text-sm"><span className="font-medium">Executable</span><PathInput ariaLabel="App executable" required directory={false} placeholder={platformExamples().appExecutable} value={customExe} onChange={setCustomExe} className="w-full" /></label>
              <DialogFooter><Button type="button" variant="ghost" onClick={() => setAddAppOpen(false)}>Cancel</Button><Button type="submit">Add app</Button></DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
        <AddProjectDialog
          open={addProjectOpen}
          onOpenChange={setAddProjectOpen}
          onImport={importProject}
          onScan={() => void scanForProjects()}
          scanning={projectScanning}
          scanLabel={projectScanDriveLabel ? `Scanning ${projectScanDriveLabel}${projectScanProgress?.current ? ` — ${projectScanProgress.current}` : "…"}` : ""}
          onCancelScan={cancelProjectScan}
          driveSummary={driveSummary}
          canScanDrives={drives.length > 0}
          onAdded={() => { void invalidate.projects(); reloadDashboard(); }}
        />
        <ShortcutsDialog open={shortcutsOpen} onOpenChange={setShortcutsOpen} />
        <Onboarding open={onboarding} onComplete={completeOnboarding} onSkip={skipOnboarding} initialTheme={preference} onPreviewTheme={setPreference} onProjectsAdded={() => { void invalidate.projects(); reloadDashboard(); }} />
        <CommandDialog open={paletteOpen} onOpenChange={setPaletteOpen}>
          <CommandInput placeholder="Type a command or search projects, apps…" />
          <CommandList>
            <CommandEmpty>No results found.</CommandEmpty>
            <CommandGroup heading="Actions">
              <CommandItem value="add project import folder" onSelect={paletteRun(() => setAddProjectOpen(true))}><FolderPlus size={15} /> Add project…<CommandShortcut>Ctrl N</CommandShortcut></CommandItem>
              <CommandItem value="find projects on disk scan drives discover" onSelect={paletteRun(() => void findProjects())}><ScanSearch size={15} /> Find projects on this computer</CommandItem>
              <CommandItem value="scan applications detect apps" onSelect={paletteRun(() => { openScreen("Applications"); void scanDrives(); })}><RefreshCw size={15} /> Scan for applications</CommandItem>
              <CommandItem value="check for updates" onSelect={paletteRun(() => { if (isNativeRuntime()) checkForUpdates(); else toast.message("Updates are checked by the desktop app."); })}><Download size={15} /> Check for updates</CommandItem>
              <CommandItem value="keyboard shortcuts help" onSelect={paletteRun(() => setShortcutsOpen(true))}><Keyboard size={15} /> Keyboard shortcuts<CommandShortcut>?</CommandShortcut></CommandItem>
              <CommandItem value="report a bug feedback" onSelect={paletteRun(() => void openExternal(`${REPO_URL}/issues/new?labels=bug`))}><Bug size={15} /> Report a bug</CommandItem>
            </CommandGroup>
            {allProjects.length ? <CommandGroup heading="Projects">
              {allProjects.map((p) => <CommandItem key={p.path} value={`project ${p.name} ${p.path}`} onSelect={paletteRun(() => openProject({ path: p.path, name: p.name }))}><Folder size={15} /> {p.name}<span className="ml-2 truncate text-xs text-muted-foreground">{p.path}</span></CommandItem>)}
              {allProjects.map((p) => <CommandItem key={`launch:${p.path}`} value={`launch open in engine ${p.name}`} onSelect={paletteRun(() => void requestOpenInEngine(p))}><Rocket size={15} /> Launch {p.name} in its engine</CommandItem>)}
              {studioAvailable() ? allProjects.map((p) => <CommandItem key={`ai:${p.path}`} value={`claude code ai agent ${p.name}`} onSelect={paletteRun(() => void studioApi.openTerminal(p.path, "claude").then(() => toast.success(`Starting Claude Code in ${p.name}`)).catch((error) => toast.error(String(error))))}><Workflow size={15} /> Open Claude Code in {p.name}</CommandItem>) : null}
            </CommandGroup> : null}
            {installedApps.length ? <CommandGroup heading="Applications">
              {installedApps.map((a) => <CommandItem key={a.id} value={`app launch ${a.name}`} onSelect={paletteRun(() => { if (a.executable && isNativeRuntime()) void run(`Launching ${a.name}`, () => desktopApi.launchApp(a.id, a.executable!), { quiet: true }); else openScreen("Applications"); })}><AppWindow size={15} /> Launch {a.name}</CommandItem>)}
            </CommandGroup> : null}
            <CommandGroup heading="Go to">
              {navigation.map(([label, Icon], index) => <CommandItem key={label} value={`go ${label}`} onSelect={paletteRun(() => openScreen(label))}><Icon size={15} /> {label}<CommandShortcut>Ctrl {index + 1}</CommandShortcut></CommandItem>)}
            </CommandGroup>
            <CommandGroup heading="Theme">
              {[{ id: "system" as ThemePreference, name: "Match system" }, ...THEMES].map((theme) => <CommandItem key={theme.id} value={`theme ${theme.name}`} onSelect={paletteRun(() => setPreference(theme.id))}><Palette size={15} /> Theme: {theme.name}{preference === theme.id ? <CommandShortcut>current</CommandShortcut> : null}</CommandItem>)}
            </CommandGroup>
          </CommandList>
        </CommandDialog>
      </main>
    </div>
  );
}

export function App() {
  // One client per app instance (the app mounts once; tests mount many).
  const [queryClient] = useState(createQueryClient);
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <TaskRunsProvider>
          <AppShell />
          <ConfirmHost />
          <Toaster richColors position="bottom-right" />
        </TaskRunsProvider>
      </TooltipProvider>
    </QueryClientProvider>
  );
}

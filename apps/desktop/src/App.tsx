import { type FormEvent, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  AppWindow,
  Box,
  Boxes,
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
import { installedApps as defaultApps, pinnedProjects as defaultPinned, recentProjects as defaultRecent, type Project } from "./data";
import { APP_CATEGORY_LABELS, desktopApi, formatVersion, isDemoMode, isNativeRuntime, loadDashboard, type HealthIssue, type ManagedApp, type RegisteredProject, type ToolManifest, type UpdateInfo } from "./bridge";
import { type ThemePreference, useTheme } from "./theme";
import voidlineImage from "./assets/voidline-reactor.png";
import "./styles.css";

const navigation = [
  ["Home", Home],
  ["Projects", Folder],
  ["Applications", Box],
  ["Health", Activity],
  ["Tools", Wrench],
  ["Settings", Settings],
] as const;

const sampleContinueProject: Project = {
  name: "Voidline",
  path: "D:/Dev/Projects/Voidline",
  engine: "Unity",
  version: "2022.3.18f1",
  branch: "main",
  lastOpened: "Today, 10:42 AM",
};

function ProjectTable({ projects, onOpen }: { projects: Project[]; onOpen: (project: Project) => void }) {
  return (
    <div className="project-table" role="table" aria-label="Projects">
      <div className="project-row project-head" role="row">
        <span>Project</span><span>Last opened</span><span>Engine / version</span><span>Branch</span><span>Actions</span>
      </div>
      {projects.map((project) => (
        <div className="project-row" role="row" key={project.name}>
          <span className="project-identity"><span className="project-icon"><Boxes size={18} /></span><span><strong>{project.name}</strong><small>{project.path}</small></span></span>
          <span>{project.lastOpened}</span>
          <span className="engine"><Box size={15} /> {project.engine} {project.version}</span>
          <span className="branch"><GitBranch size={14} /> {project.branch}</span>
          <span className="row-actions"><button className="outline-button" onClick={() => onOpen(project)}><Folder size={14} /> Open Project</button><button className="icon-button" onClick={() => onOpen(project)} aria-label={`More actions for ${project.name}`}><MoreHorizontal size={17} /></button></span>
        </div>
      ))}
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return <div className="empty-state"><CircleAlert size={20} /><span>{text}</span></div>;
}

const CATEGORY_ORDER = ["game-engine", "dcc", "art", "code", "version-control", "utility"];

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
  return src ? <img className="app-icon-img" src={src} alt="" width={size} height={size} /> : <AppWindow size={size} />;
}

export function App() {
  const [activeScreen, setActiveScreen] = useState<(typeof navigation)[number][0]>("Home");
  const [projectView, setProjectView] = useState<"pinned" | "recent">("pinned");
  const [query, setQuery] = useState("");
  const [continueProject, setContinueProject] = useState<Project | null>(isDemoMode() ? sampleContinueProject : null);
  const [pinnedProjects, setPinnedProjects] = useState(isDemoMode() ? defaultPinned : []);
  const [recentProjects, setRecentProjects] = useState(isDemoMode() ? defaultRecent : []);
  const [installedApps, setInstalledApps] = useState<Array<{ id: string; name: string; versions: string[] }>>(
    isDemoMode()
      ? defaultApps.map((app) => ({ id: app.name.toLowerCase().replaceAll(" ", "-"), ...app }))
      : [],
  );
  const [registeredProjects, setRegisteredProjects] = useState<RegisteredProject[]>([]);
  const [managedApps, setManagedApps] = useState<ManagedApp[]>([]);
  const [health, setHealth] = useState<HealthIssue[]>([]);
  const [tools, setTools] = useState<ToolManifest[]>([]);
  const [rootInput, setRootInput] = useState("");
  const [nameInput, setNameInput] = useState("");
  const [profileInput, setProfileInput] = useState("editor");
  const [branchInput, setBranchInput] = useState("main");
  const [commitInput, setCommitInput] = useState("");
  const [scanRoots, setScanRoots] = useState("");
  const [overrideApp, setOverrideApp] = useState("unity");
  const [overrideVersion, setOverrideVersion] = useState("");
  const [overrideExecutable, setOverrideExecutable] = useState("");
  const [status, setStatus] = useState("");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
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
  }, []);

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  async function run(label: string, action: () => Promise<unknown>) {
    setStatus(`${label}...`);
    try { await action(); setStatus(`${label} complete.`); }
    catch (error) { setStatus(error instanceof Error ? error.message : String(error)); }
  }

  async function refreshProjects() { setRegisteredProjects(await desktopApi.listProjects()); }
  async function refreshApps() { setManagedApps(await desktopApi.listApps()); }

  function openScreen(screen: (typeof navigation)[number][0]) {
    setActiveScreen(screen);
    setStatus("");
    if (isNativeRuntime() && screen === "Projects") void run("Refreshing projects", refreshProjects);
    if (isNativeRuntime() && screen === "Applications") void run("Refreshing applications", refreshApps);
    if (isNativeRuntime() && screen === "Tools") void run("Loading cached tool index", async () => setTools(await desktopApi.listTools()));
  }

  async function submitImport(event: FormEvent) {
    event.preventDefault();
    if (!window.confirm(`Import ${rootInput} and create its local Vantadeck metadata?`)) return;
    await run("Importing project", async () => { await desktopApi.importProject(rootInput, nameInput); await refreshProjects(); });
  }

  const runtimeLabel = isNativeRuntime() ? "Native desktop" : isDemoMode() ? "Browser demo" : "Browser preview";
  const managementContent = (
    <section className="management-screen">
      <div className="page-heading"><div><span className="section-label">Workspace management</span><h1>{activeScreen}</h1></div><span className="runtime-badge">{runtimeLabel}</span></div>
      {activeScreen === "Projects" ? <>
        <form className="action-panel" onSubmit={submitImport}><h2>Import a local project</h2><p>Creates a `.vantadeck/project.toml` file and registers the project in local storage.</p><div className="form-row"><input aria-label="Project path" required placeholder="D:/Projects/MyGame" value={rootInput} onChange={(e) => setRootInput(e.target.value)} /><input aria-label="Project name" placeholder="Optional display name" value={nameInput} onChange={(e) => setNameInput(e.target.value)} /><button className="primary-button">Import project</button></div></form>
        <div className="action-panel"><h2>Launch a project profile</h2><p>Profiles come from the portable project file; executable selection remains machine-local.</p><div className="form-row"><input aria-label="Launch project path" placeholder="Project root" value={rootInput} onChange={(e) => setRootInput(e.target.value)} /><input aria-label="Launch profile ID" placeholder="editor" value={profileInput} onChange={(e) => setProfileInput(e.target.value)} /><button className="primary-button" onClick={() => void run("Launching profile", () => desktopApi.launchProjectProfile(rootInput, profileInput))}>Launch profile</button></div></div>
        <div className="action-panel"><h2>Git workflow</h2><p>Status is read-only. Sync, commit, push, and branch switching always ask for confirmation.</p><div className="form-row"><input aria-label="Git branch" placeholder="Branch" value={branchInput} onChange={(e) => setBranchInput(e.target.value)} /><input aria-label="Git commit message" placeholder="Commit message" value={commitInput} onChange={(e) => setCommitInput(e.target.value)} /><button className="outline-button" onClick={() => { if (window.confirm(`Switch ${rootInput} to ${branchInput}?`)) void run("Switching branch", () => desktopApi.gitSwitch(rootInput, branchInput, true)); }}>Switch</button><button className="outline-button" onClick={() => { if (window.confirm(`Pull changes into ${rootInput}?`)) void run("Syncing repository", () => desktopApi.gitSync(rootInput, true)); }}>Sync</button><button className="outline-button" onClick={() => { if (window.confirm(`Commit all changes in ${rootInput}?`)) void run("Committing changes", () => desktopApi.gitCommit(rootInput, commitInput, true)); }}>Commit</button><button className="outline-button" onClick={() => { if (window.confirm(`Push ${rootInput} to its configured remote?`)) void run("Pushing repository", () => desktopApi.gitPush(rootInput, true)); }}>Push</button></div></div>
        <div className="card-grid">{registeredProjects.length ? registeredProjects.map((project) => <article className="management-card" key={project.path}><Folder /><div><h3>{project.name}</h3><p>{project.path}</p></div><span className="card-actions"><button className="outline-button" onClick={() => void run(project.pinned ? "Unpinning project" : "Pinning project", async () => { await desktopApi.pinProject(project.path, !project.pinned); await refreshProjects(); })}>{project.pinned ? "Unpin" : "Pin"}</button><button className="outline-button" onClick={() => void run("Reading Git status", async () => { const result = await desktopApi.gitStatus(project.path); setStatus(`Branch ${result.branch ?? "detached"}; ${result.changedFiles.length} changed files.`); })}>Git status</button></span></article>) : <EmptyState text="No durable projects registered yet." />}</div>
      </> : null}
      {activeScreen === "Applications" ? <>
        <div className="action-panel"><h2>Detect installed applications</h2><p>Leave roots blank to auto-scan standard install folders on every drive, or provide semicolon-separated roots. Detection uses bundled, auditable manifests.</p><div className="form-row"><input aria-label="Scan roots" placeholder="Blank = all drives, or e.g. D:/Tools; E:/Apps" value={scanRoots} onChange={(e) => setScanRoots(e.target.value)} /><button className="primary-button" onClick={() => void run("Scanning applications", async () => { await desktopApi.scanApps(scanRoots.split(";").map((v) => v.trim()).filter(Boolean)); await refreshApps(); })}>Scan now</button></div></div>
        {CATEGORY_ORDER.filter((category) => managedApps.some((app) => app.category === category)).map((category) => (
          <section className="app-category" key={category}>
            <h2 className="app-category-title">{APP_CATEGORY_LABELS[category] ?? category}</h2>
            <div className="card-grid">{managedApps.filter((app) => app.category === category).map((app) => {
              const primary = app.installations[0];
              return (
                <article className="management-card" key={app.id}>
                  <span className="app-glyph"><AppIcon executable={primary?.executable} size={26} /></span>
                  <div>
                    <h3>{app.name}</h3>
                    <p>{app.installations.length ? app.installations.map((item) => formatVersion(item.version)).join(", ") : "Not detected"}</p>
                    {!app.launchable && app.installations.length ? <small className="vcs-note">Detected — used for project version control</small> : null}
                  </div>
                  {app.launchable && primary ? <button className="outline-button" onClick={() => { if (window.confirm(`Launch ${app.name}?`)) void run(`Launching ${app.name}`, () => desktopApi.launchApp(app.id, primary.executable)); }}>Launch</button> : null}
                </article>
              );
            })}</div>
          </section>
        ))}
        {managedApps.length && managedApps.every((app) => app.installations.length === 0) ? <EmptyState text="No applications detected yet. Click Scan now to discover installed creative tools across your drives." /> : null}
        <form className="action-panel" onSubmit={(event) => { event.preventDefault(); if (window.confirm(`Set a manual executable override for ${overrideApp} ${overrideVersion}?`)) void run("Saving override", async () => { await desktopApi.setManualOverride(overrideApp, overrideVersion, overrideExecutable); await refreshApps(); }); }}><h2>Manual override</h2><div className="form-row"><input aria-label="Application ID" value={overrideApp} onChange={(e) => setOverrideApp(e.target.value)} /><input aria-label="Version" required placeholder="2022.3.18" value={overrideVersion} onChange={(e) => setOverrideVersion(e.target.value)} /><input aria-label="Executable path" required placeholder="C:/Tools/app.exe" value={overrideExecutable} onChange={(e) => setOverrideExecutable(e.target.value)} /><button className="primary-button">Save override</button></div></form>
      </> : null}
      {activeScreen === "Health" ? <><div className="action-panel"><h2>Run project diagnostics</h2><div className="form-row"><input aria-label="Health project path" placeholder="Project root" value={rootInput} onChange={(e) => setRootInput(e.target.value)} /><button className="primary-button" onClick={() => void run("Running health checks", async () => setHealth(await desktopApi.projectHealth(rootInput)))}>Run checks</button></div></div><div className="issue-grid">{health.length ? health.map((issue) => <article className={`issue-card ${issue.severity}`} key={issue.code}><CircleAlert /><div><h3>{issue.title}</h3><p>{issue.detail}</p><small>{issue.code}</small></div></article>) : <EmptyState text="No health issues reported. Run checks for a registered project." />}</div></> : null}
      {activeScreen === "Tools" ? <><div className="action-panel"><h2>Curated Tools Hub</h2><p>Vantadeck shows validated, locally cached community metadata. It never executes downloaded installers or scripts automatically.</p>{!isNativeRuntime() ? <p>Open the native desktop app to read your local cache. Network access remains independently opt-in.</p> : null}</div><div className="card-grid">{tools.length ? tools.filter((tool) => tool.reviewState !== "withdrawn").map((tool) => <article className="management-card tool-card" key={tool.id}><Wrench /><div><h3>{tool.name}</h3><p>{tool.description}</p><small>{tool.reviewState} · {tool.license} · checked {tool.lastVerifiedAt}</small></div><button className="outline-button" onClick={() => setStatus(`${tool.sourceUrl} — open this reviewed source in your browser.`)}>Source</button></article>) : <EmptyState text="No validated tool index is cached. Use the CLI cache command after reviewing an index source." />}</div></> : null}
      {activeScreen === "Settings" ? <div className="settings-grid"><div className="action-panel"><h2>Appearance</h2><label>Theme<select aria-label="Settings theme" value={preference} onChange={(event) => setPreference(event.target.value as ThemePreference)}><option value="system">System</option><option value="dark">Dark</option><option value="light">Light</option></select></label></div>
        <div className="action-panel"><h2>Updates</h2>{update?.available ? <p>Version <strong>{update.version}</strong> is available (you have {update.currentVersion}).{update.notes ? <><br /><small>{update.notes}</small></> : null}</p> : <p>{update ? `You are on the latest version (${update.currentVersion}).` : "Check for a newer signed release. Updates are verified against Vantadeck's signing key before installation."}</p>}<div className="form-row"><button className="outline-button" onClick={() => void run("Checking for updates", async () => { const info = await desktopApi.checkForUpdate(); setUpdate(info); setStatus(info.available ? `Update ${info.version} is available.` : "You are on the latest version."); })}><RefreshCw size={15} /> Check for updates</button>{update?.available ? <button className="primary-button" onClick={() => { if (window.confirm(`Download and install version ${update.version} now? Vantadeck will restart.`)) void run("Installing update", () => desktopApi.installUpdate()); }}><Download size={15} /> Install &amp; restart</button> : null}</div></div>
        <div className="action-panel"><h2>Runtime</h2><p>{runtimeLabel}. Network operations are disabled by default; all indexed data is stored locally.</p></div></div> : null}
      {status ? <output className="operation-status">{status}</output> : null}
    </section>
  );

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><strong>VANTADECK</strong><span>LOCAL-FIRST CREATIVE LAUNCHER</span></div>
        <nav aria-label="Primary navigation">
          {navigation.map(([label, Icon]) => <button onClick={() => openScreen(label)} className={activeScreen === label ? "nav-item active" : "nav-item"} key={label}><Icon size={20} /><span>{label}</span>{label === "Health" && health.length ? <b>{health.length}</b> : null}</button>)}
        </nav>
        <div className="system-status"><Laptop size={19} /><span><small>System status</small><strong>Offline &amp; Local</strong><em>All systems operational</em></span></div>
        <div className="sidebar-footer"><span>v0.1.0</span><span><Code2 size={14} /> Open Source</span></div>
      </aside>

      <main>
        <header className="topbar">
          <label className="search"><Search size={19} /><input ref={searchRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search projects, apps, tools, docs..." /><kbd>Ctrl K</kbd></label>
          <div className="local-state"><Laptop size={16} /> Offline • Local mode</div>
        </header>

        {update?.available ? <div className="update-banner"><Download size={16} /><span>Vantadeck {update.version} is available.</span><button className="text-link" onClick={() => openScreen("Settings")}>View update</button></div> : null}

        <div className="content">
          {activeScreen === "Home" ? <><section className="continue-section" aria-labelledby="continue-title">
            <div className="section-label" id="continue-title">Continue Project</div>
            {continueProject ? <div className="continue-panel">
              <img src={voidlineImage} alt="Voidline reactor environment" />
              <div className="continue-details"><h1>{continueProject.name}</h1><p>{continueProject.path}</p><div className="metadata"><span><Box size={15} /> {continueProject.engine} {continueProject.version}</span><span><GitBranch size={14} /> {continueProject.branch}</span><span>Last opened: {continueProject.lastOpened}</span></div><h2>Local project</h2><ul><li><Folder size={15} /> Project metadata and activity stay on this machine.</li><li><FileCode2 size={15} /> Portable settings live in .vantadeck/project.toml.</li></ul></div>
              <div className="continue-health"><button className="primary-button" onClick={() => openScreen("Projects")}>Open Project</button><div><h2>Health summary</h2>{health.length ? <div className="health-list compact">{health.slice(0, 3).map((issue) => <div className="health-item" key={issue.code}><CircleAlert className={issue.severity} size={17} /><span><strong>{issue.title}</strong><small>{issue.detail}</small></span></div>)}</div> : <EmptyState text="No current health issues." />}</div></div>
            </div> : <div className="continue-panel empty-continue"><EmptyState text="Import a project to start working locally." /></div>}
          </section>

          <section className="workspace">
            <div className="projects-pane">
              <div className="tabs" role="tablist"><button role="tab" aria-selected={projectView === "pinned"} onClick={() => setProjectView("pinned")}>Pinned Projects</button><button role="tab" aria-selected={projectView === "recent"} onClick={() => setProjectView("recent")}>Recent Projects</button></div>
              <ProjectTable projects={filteredProjects} onOpen={() => openScreen("Projects")} />
              <button className="text-link" onClick={() => openScreen("Projects")}>View all projects <ChevronRight size={17} /></button>
            </div>
            <aside className="right-rail">
              <section className="rail-section"><div className="rail-title">Project Health <button onClick={() => openScreen("Health")}>View All ({health.length})</button></div>{health.length ? <div className="health-list">{health.slice(0, 3).map((issue) => <div className="health-item" key={issue.code}><CircleAlert className={issue.severity} size={17} /><span><strong>{issue.title}</strong><small>{issue.detail}</small></span></div>)}</div> : <EmptyState text="No current health issues." />}</section>
              <section className="rail-section"><div className="rail-title">Installed Apps <button onClick={() => openScreen("Applications")}>Manage</button></div><div className="apps-list">{installedApps.map((app) => <button className="app-row" onClick={() => openScreen("Applications")} key={app.name}><span className="app-glyph"><AppWindow size={19} /></span><span><strong>{app.name}</strong>{app.versions.map((version) => <small key={version}>{version}</small>)}</span><ChevronRight size={16} /></button>)}</div></section>
            </aside>
          </section></> : managementContent}
        </div>

        <footer className="quick-launch"><span className="quick-title">Quick Launch</span>{installedApps.slice(0, 5).map((app) => <button key={app.id} onClick={() => openScreen("Applications")}><AppWindow size={22} /><span>{app.name}</span></button>)}{installedApps.length === 0 ? <button onClick={() => openScreen("Applications")}><Search size={22} /><span>Detect applications</span></button> : null}<label className="theme-control"><Settings size={16} /><span>Theme</span><select aria-label="Theme" value={preference} onChange={(event) => setPreference(event.target.value as ThemePreference)}><option value="system">System</option><option value="dark">Dark</option><option value="light">Light</option></select></label></footer>
      </main>
    </div>
  );
}

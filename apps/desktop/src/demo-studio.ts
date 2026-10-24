// In-memory stand-in for the studio commands, used only in the browser demo
// (?demo=true) so tasks, the board, assets, and AI screens are explorable and
// testable without the desktop runtime. Mirrors the Rust command contracts.
import type { AiClientStatus, AssetEntry, DiscoveredProject, EngineActivity, AssetMeta, FolderLayout, Issue, NewIssue, TaskDefinition, TaskLineBatch, TaskRun, TrackerConfig } from "./bridge";

const iso = (minutesAgo = 0) => new Date(Date.now() - minutesAgo * 60_000).toISOString().replace(/\.\d+Z$/, "Z");

const templates: TaskDefinition[] = [
  { id: "unity-build-win64", name: "Unity: Build Windows (batch mode)", kind: "build", description: "Headless Unity build via an Editor build method.", program: "${engine}", args: ["-batchmode", "-quit", "-projectPath", "${project}", "-buildTarget", "Win64", "-executeMethod", "PipelineOS.Build.Windows", "-logFile", "-"] },
  { id: "unity-tests", name: "Unity: Run EditMode tests", kind: "test", description: "Runs the EditMode test suite headlessly.", program: "${engine}", args: ["-batchmode", "-runTests", "-testPlatform", "EditMode", "-projectPath", "${project}"] },
  { id: "unreal-buildcookrun", name: "Unreal: BuildCookRun Win64 Development", kind: "cook", description: "Builds, cooks, stages and packages via RunUAT.", program: "RunUAT.bat", args: ["BuildCookRun", "-project=${project}/${projectName}.uproject", "-platform=Win64", "-build", "-cook", "-stage", "-pak"] },
  { id: "godot-export", name: "Godot: Export release build", kind: "export", description: "Exports a preset headlessly.", program: "${engine}", args: ["--headless", "--export-release", "Windows Desktop"] },
  { id: "blender-render", name: "Blender: Render animation", kind: "render", description: "Renders the frame range in the background.", program: "${engine}", args: ["-b", "${project}/scene.blend", "-a"] },
  { id: "custom-script", name: "Custom script", kind: "script", description: "Any program with arguments.", program: "python", args: ["tools/script.py"] },
];

const layouts: FolderLayout[] = [
  { id: "game-art", name: "Game team", description: "Art, audio, design docs, and builds beside the engine project.", folders: ["Art/Concept", "Art/Reference", "Art/Source/Models", "Art/Exports", "Audio/Source", "Docs/Design", "Builds"] },
  { id: "studio-pipeline", name: "Studio pipeline", description: "Numbered production stages from brief to delivery.", folders: ["00_Admin", "01_Reference", "02_Concept", "03_Production", "04_Review", "05_Delivery"] },
  { id: "film-vfx", name: "Film / VFX / Animation", description: "Asset- and shot-based structure.", folders: ["editorial", "reference", "concept", "assets", "shots", "renders", "comp", "delivery"] },
];

type ProjectStore = { tasks: TaskDefinition[]; presets: TaskDefinition[]; trusted: boolean; issues: Issue[]; assets: AssetEntry[]; tracker: TrackerConfig | null; hasToken: boolean };
const stores = new Map<string, ProjectStore>();
const runs: TaskRun[] = [];
const logs = new Map<string, string[]>();
const timers = new Map<string, number>();
const runListeners = new Set<(run: TaskRun) => void>();
const lineListeners = new Set<(batch: TaskLineBatch) => void>();

function seededIssues(): Issue[] {
  const issue = (id: string, title: string, status: Issue["status"], priority: Issue["priority"], kind: Issue["kind"], assignee: string | null, minutes: number, body = ""): Issue =>
    ({ id, title, status, priority, kind, assignee, labels: [], files: [], createdAt: iso(minutes + 600), updatedAt: iso(minutes), body });
  return [
    issue("a1c9e2", "Reactor room lighting pass", "in-progress", "high", "art", "ana", 30, "Bake lightmaps after the new emissive panels land."),
    issue("b7f310", "Player clips through catwalk rails", "todo", "urgent", "bug", "sam", 90, "Repro: sprint + jump at the east rail."),
    issue("c2d4a8", "Hero concept v3 — silhouette options", "review", "medium", "art", "lee", 200),
    issue("d93b1f", "Save system: cloud-safe slots", "backlog", "medium", "feature", null, 3000),
    issue("e40c77", "Update build machine to 2022.3.18f1", "done", "low", "task", "sam", 5000),
  ];
}

function seededAssets(): AssetEntry[] {
  const now = Math.floor(Date.now() / 1000);
  const asset = (path: string, kind: AssetEntry["kind"], hoursAgo: number, status: AssetMeta["status"] = "none"): AssetEntry => {
    const parts = path.split("/");
    return { path, name: parts[parts.length - 1], folder: parts.slice(0, -1).join("/"), kind, size: 1_200_000 + hoursAgo * 7_000, modified: now - hoursAgo * 3600, meta: { status } };
  };
  return [
    asset("Art/Concept/reactor_room_v4.png", "image", 2, "review"),
    asset("Art/Concept/hero_silhouettes.psd", "source", 5, "wip"),
    asset("Art/Concept/catwalk_paintover.png", "image", 26, "approved"),
    asset("Art/Reference/industrial_moodboard.jpg", "image", 50),
    asset("Art/Source/Models/reactor_core.fbx", "model", 8, "wip"),
    asset("Art/Source/Models/catwalk_kit.blend", "model", 72, "approved"),
    asset("Audio/Source/reactor_hum_loop.wav", "audio", 30),
    asset("Docs/Design/level_flow.pdf", "document", 120),
    asset("Renders/turntable_reactor.mp4", "video", 12, "review"),
  ];
}

function store(path: string): ProjectStore {
  let entry = stores.get(path);
  if (!entry) {
    entry = { tasks: [templates[0], templates[1]], presets: [], trusted: false, issues: seededIssues(), assets: seededAssets(), tracker: null, hasToken: false };
    stores.set(path, entry);
  }
  return entry;
}

function fingerprint(tasks: TaskDefinition[]): string {
  let hash = 0;
  for (const char of JSON.stringify(tasks)) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return (hash >>> 0).toString(16);
}

function emitRun(run: TaskRun) {
  runListeners.forEach((listener) => listener({ ...run }));
}

function startDemoRun(path: string, task: TaskDefinition): TaskRun {
  const run: TaskRun = {
    id: `run-${Math.random().toString(36).slice(2, 10)}`, projectPath: path, projectName: path.split(/[\\/]/).pop() ?? path,
    taskId: task.id, taskName: task.name, kind: task.kind, commandLine: [task.program.replace("${engine}", "C:/Program Files/Unity/Editor/Unity.exe"), ...task.args].join(" "),
    status: "running", progress: 0, lastLine: null, startedAt: iso(), finishedAt: null, exitCode: null, durationMs: null,
  };
  runs.unshift(run);
  logs.set(run.id, []);
  emitRun(run);
  const steps = ["Loading project…", "Resolving packages", "Compiling scripts", "Importing assets", "Building player", "Compressing", "Writing build report"];
  let tick = 0;
  const started = Date.now();
  const timer = window.setInterval(() => {
    tick += 1;
    const progress = Math.min(100, tick * 8);
    const text = `[${tick}/13] ${steps[tick % steps.length]} (${progress}%)`;
    logs.get(run.id)?.push(text);
    run.progress = progress;
    run.lastLine = text;
    lineListeners.forEach((listener) => listener({ runId: run.id, lines: [{ stream: "stdout", text }] }));
    if (progress >= 100) {
      window.clearInterval(timer);
      timers.delete(run.id);
      run.status = "succeeded";
      run.exitCode = 0;
      run.finishedAt = iso();
      run.durationMs = Date.now() - started;
      run.lastLine = "Build succeeded: Builds/Win64/Voidline.exe";
    }
    emitRun(run);
  }, 450);
  timers.set(run.id, timer);
  return run;
}

export async function demoStudio(command: string, args: Record<string, unknown>): Promise<unknown> {
  const path = String(args.path ?? "");
  switch (command) {
    case "tasks_list": {
      const entry = store(path);
      return { tasks: entry.tasks, fingerprint: entry.tasks.length ? fingerprint(entry.tasks) : null, trusted: entry.trusted };
    }
    case "tasks_templates": return templates;
    case "tasks_presets": {
      const engineOf = (id: string) => id.split("-")[0];
      const groupOf: Record<string, string> = { unity: "Unity", unreal: "Unreal Engine", godot: "Godot", blender: "Blender", custom: "General" };
      const presets = [
        ...templates,
        { id: "unity-android-dev", name: "Unity: Android development APK", kind: "build" as const, description: "Development build with script debugging. Needs Assets/Editor/PipelineOSBuild.cs.", program: "${engine}", args: ["-batchmode", "-quit", "-projectPath", "${project}", "-buildTarget", "Android", "-executeMethod", "PipelineOS.Build.Android", "-pipelineosDevelopment"] },
        { id: "unity-android-release", name: "Unity: Android release (AAB)", kind: "build" as const, description: "Google Play App Bundle. Needs Assets/Editor/PipelineOSBuild.cs.", program: "${engine}", args: ["-batchmode", "-quit", "-projectPath", "${project}", "-buildTarget", "Android", "-executeMethod", "PipelineOS.Build.AndroidAppBundle"] },
      ];
      const own = store(path).presets.map((definition) => ({ definition, engine: null, group: "This project", recommended: false, project: true }));
      return [...own, ...presets.map((definition) => ({ definition, engine: engineOf(definition.id) === "custom" ? null : engineOf(definition.id), group: groupOf[engineOf(definition.id)] ?? "General", recommended: engineOf(definition.id) === "unity" }))];
    }
    case "presets_save": { store(path).presets = args.presets as TaskDefinition[]; return undefined; }
    case "tasks_to_toml": {
      const q = (value: string) => JSON.stringify(value);
      return (args.tasks as TaskDefinition[]).map((task) => [
        "[[task]]", `id = ${q(task.id)}`, `name = ${q(task.name)}`, `kind = ${q(task.kind)}`,
        ...(task.description ? [`description = ${q(task.description)}`] : []),
        `program = ${q(task.program)}`, `args = [${task.args.map(q).join(", ")}]`,
        ...(task.cwd ? [`cwd = ${q(task.cwd)}`] : []),
      ].join("\n")).join("\n\n") + "\n";
    }
    case "tasks_parse_toml": throw new Error("Importing tasks works in the desktop app.");
    case "tasks_install_unity_script": return `${path}/Assets/Editor/PipelineOSBuild.cs`;
    case "tasks_save": { const entry = store(path); entry.tasks = args.tasks as TaskDefinition[]; entry.trusted = true; return undefined; }
    case "tasks_trust": { store(path).trusted = true; return undefined; }
    case "task_plan": {
      const task = store(path).tasks.find((t) => t.id === args.taskId);
      if (!task) throw new Error(`task "${String(args.taskId)}" was not found`);
      return [task.program, ...task.args].join(" ").replaceAll("${project}", path);
    }
    case "task_run": {
      const entry = store(path);
      if (!entry.trusted) throw new Error("TASKS_UNTRUSTED: review and trust this project's tasks before running them.");
      const task = entry.tasks.find((t) => t.id === args.taskId);
      if (!task) throw new Error(`task "${String(args.taskId)}" was not found`);
      return startDemoRun(path, task);
    }
    case "task_cancel": {
      const id = String(args.runId);
      const timer = timers.get(id);
      const run = runs.find((r) => r.id === id);
      if (!timer || !run) throw new Error("That task is no longer running.");
      window.clearInterval(timer);
      timers.delete(id);
      Object.assign(run, { status: "cancelled", finishedAt: iso(), lastLine: "Cancelled." });
      emitRun(run);
      return undefined;
    }
    case "task_runs": return runs.filter((run) => !args.path || run.projectPath === args.path).map((run) => ({ ...run }));
    case "task_run_log": return [...(logs.get(String(args.runId)) ?? [])];
    case "task_clear_history": { for (let i = runs.length - 1; i >= 0; i--) if (runs[i].status !== "running") runs.splice(i, 1); return undefined; }
    case "issues_list": return { issues: [...store(path).issues], problems: [] };
    case "issue_create": {
      const input = args.issue as NewIssue;
      if (!input.title?.trim()) throw new Error("an issue needs a title");
      const issue: Issue = { id: Math.random().toString(16).slice(2, 8), title: input.title.trim(), status: input.status ?? "todo", priority: input.priority ?? "none", kind: input.kind ?? "task", assignee: input.assignee ?? null, labels: input.labels ?? [], files: input.files ?? [], createdAt: iso(), updatedAt: iso(), body: input.body ?? "" };
      store(path).issues.unshift(issue);
      return issue;
    }
    case "issue_update": {
      const next = { ...(args.issue as Issue), updatedAt: iso() };
      const list = store(path).issues;
      const index = list.findIndex((i) => i.id === next.id);
      if (index < 0) throw new Error(`issue "${next.id}" was not found`);
      list[index] = next;
      return next;
    }
    case "issue_delete": { const list = store(path).issues; const index = list.findIndex((i) => i.id === args.id); if (index >= 0) list.splice(index, 1); return undefined; }
    case "issues_everywhere": {
      const demoPath = "D:/Dev/Projects/Voidline";
      return store(demoPath).issues.filter((i) => i.status !== "done").map((issue) => ({ ...issue, projectName: "Voidline", projectPath: demoPath }));
    }
    case "project_brief": {
      const open = store(path).issues.filter((i) => i.status !== "done");
      const name = path.split(/[\\/]/).pop();
      return {
        markdown: `# ${name} — project brief\n\n## Overview\n- Type: unity\n- Engine: Unity (2022.3.18f1)\n- Version control: git, branch \`main\`\n\n## Working in this project\n- Unity project: authored content lives in \`Assets/\`.\n- Every asset has a sibling \`.meta\` file. Move/rename them together.\n- Do not edit \`Library/\`, \`Temp/\`, or \`obj/\`.\n\n## Layout\n- \`Assets/\` (4,812 files)\n- \`Library/\` (31 files) — Unity cache (generated)\n- \`Art/\` (214 files)\n\n## Pipeline tasks\n${store(path).tasks.map((t) => `- \`${t.id}\`: ${t.name}`).join("\n")}\n\n## Open issues (${open.length})\n${open.map((i) => `- #${i.id} [${i.status}] ${i.title}`).join("\n")}\n`,
        writtenTo: args.write ? `${path}/.pipelineos/context.md` : null,
      };
    }
    case "asset_layouts": return layouts;
    case "asset_apply_layout": {
      const layout = layouts.find((l) => l.id === args.layout);
      if (!layout) throw new Error("layout not found");
      return { created: layout.folders.slice(2), existing: layout.folders.slice(0, 2) };
    }
    case "assets_index": return { assets: [...store(path).assets], truncated: false };
    case "asset_set_meta": {
      const asset = store(path).assets.find((a) => a.path === args.asset);
      if (asset) asset.meta = args.meta as AssetMeta;
      return undefined;
    }
    case "tracker_get": { const entry = store(path); return { config: entry.tracker, hasToken: entry.hasToken }; }
    case "tracker_save": {
      const entry = store(path);
      entry.tracker = (args.config as TrackerConfig | null) ?? null;
      if (args.token) entry.hasToken = true;
      return undefined;
    }
    case "tracker_sync": {
      const entry = store(path);
      if (!entry.tracker) throw new Error("connect a tracker for this project first");
      const key = entry.tracker.kind === "jira" ? `${entry.tracker.project}-142` : "#142";
      if (entry.issues.some((issue) => issue.external?.key === key)) return { created: 0, updated: 0, unchanged: 1 };
      entry.issues.unshift({ id: "f1a2b3", title: "Ship demo build to publisher", status: "todo", priority: "high", kind: "task", assignee: "producer", labels: ["milestone"], files: [], createdAt: iso(), updatedAt: iso(), body: "Imported from the tracker.", external: { system: entry.tracker.kind, key, url: null } });
      return { created: 1, updated: 0, unchanged: 0 };
    }
    case "selfhost_forgejo": return { composePath: `${String(args.dir)}/docker-compose.yml`, dockerAvailable: false, started: false, url: "http://localhost:3000" };
    case "open_terminal": return undefined;
    case "cli_info": return { cliPath: "C:/Program Files/PipelineOS/vantadeck.exe", channel: "stable", mcpArgs: ["mcp"] };
    case "discover_projects": return { projects: discovered.filter((project) => !dismissed.has(project.path)), scannedAt: iso(1) };
    case "dismiss_discovered": { dismissed.add(String(args.path)); return undefined; }
    case "mcp_tools": return [
      { name: "list_projects", title: "List projects", description: "Projects registered in Pipeline OS, with paths.", readOnly: true },
      { name: "project_brief", title: "Project brief", description: "Markdown brief of a project. Read this first.", readOnly: true },
      { name: "run_task", title: "Run pipeline task", description: "Runs a trusted project task and returns its log.", readOnly: false },
      { name: "create_issue", title: "Create issue", description: "Adds an issue to the project board.", readOnly: false },
    ];
    case "ai_clients": return { clients: aiClients.map((client) => ({ ...client })), agentClis: ["claude", "codex"], canConfigure: true };
    case "ai_configure": {
      const ids = args.ids as string[];
      for (const client of aiClients) if (ids.includes(client.id)) client.configured = true;
      return ids.map((id) => ({ id, ok: true, error: null }));
    }
    case "ai_unconfigure": { const client = aiClients.find((item) => item.id === args.id); if (client) client.configured = false; return undefined; }
    case "project_active_time": {
      const days = Array.from({ length: 14 }, (_, index) => ({ date: new Date(Date.now() - (13 - index) * 86_400_000).toISOString().slice(0, 10), minutes: [0, 45, 120, 95, 0, 0, 210, 180, 60, 150, 240, 30, 0, 75][index] }));
      return { todayMinutes: 75, weekMinutes: 765, totalMinutes: 5230, lastActive: iso(12), days };
    }
    case "active_time_all": return [{ path: "D:/Dev/Projects/Voidline", minutes: 765, totalMinutes: 5230, lastActive: iso(12) }];
    case "engine_activity": return engineActivity.map((activity) => ({ ...activity }));
    case "clear_engine_activity": { for (let i = engineActivity.length - 1; i >= 0; i--) if (engineActivity[i].status !== "running") engineActivity.splice(i, 1); return undefined; }
    default: throw new Error(`demo: unsupported command ${command}`);
  }
}

const discovered: DiscoveredProject[] = [
  { path: "D:/Dev/Unity/Skyline", name: "Skyline", engine: "unity", engineVersion: "6000.0.23f1", source: "unity-hub", modified: Math.floor(Date.now() / 1000) - 3600 },
  { path: "D:/Dev/Unreal/Reactor", name: "Reactor", engine: "unreal", engineVersion: "5.4", source: "unreal-recent", modified: Math.floor(Date.now() / 1000) - 86_400 },
  { path: "D:/Dev/Godot/Pocket Garden", name: "Pocket Garden", engine: "godot", engineVersion: "4.3", source: "godot-projects", modified: Math.floor(Date.now() / 1000) - 7 * 86_400 },
];
const dismissed = new Set<string>();

const aiClients: AiClientStatus[] = [
  { id: "claude-desktop", name: "Claude Desktop", installed: true, configured: false, configPath: "%APPDATA%/Claude/claude_desktop_config.json", problem: null },
  { id: "claude-code", name: "Claude Code", installed: true, configured: true, configPath: "~/.claude.json", problem: null },
  { id: "cursor", name: "Cursor", installed: true, configured: false, configPath: "~/.cursor/mcp.json", problem: null },
  { id: "vscode", name: "VS Code (Copilot)", installed: true, configured: false, configPath: "%APPDATA%/Code/User/mcp.json", problem: null },
  { id: "codex", name: "Codex CLI", installed: true, configured: false, configPath: "~/.codex/config.toml", problem: null },
  { id: "windsurf", name: "Windsurf", installed: false, configured: false, configPath: "~/.codeium/windsurf/mcp_config.json", problem: null },
  { id: "gemini-cli", name: "Gemini CLI", installed: false, configured: false, configPath: "~/.gemini/settings.json", problem: null },
];

const engineActivity: EngineActivity[] = [
  { id: "demo-light", engine: "unreal", projectPath: "D:/Dev/Unreal/Reactor", projectName: "Reactor", kind: "lighting", status: "running", progress: 42, detail: "LogStaticLightingSystem: Running Lightmass w/ ImmediateImport mode ENABLED", startedAt: iso(6), updatedAt: iso(), finishedAt: null },
  { id: "demo-build", engine: "unity", projectPath: "D:/Dev/Projects/Voidline", projectName: "Voidline", kind: "build", status: "succeeded", progress: 100, detail: "Build completed with a result of 'Succeeded' in 182 seconds", startedAt: iso(48), updatedAt: iso(45), finishedAt: iso(45) },
];

export function demoEngineEvents(handler: (activity: EngineActivity) => void): () => void {
  const timer = window.setInterval(() => {
    const running = engineActivity.find((activity) => activity.status === "running");
    if (!running) return;
    running.progress = Math.min(100, (running.progress ?? 0) + 7);
    running.updatedAt = iso();
    if (running.progress >= 100) { running.status = "succeeded"; running.finishedAt = iso(); running.detail = "Lighting build took 312.4 seconds"; }
    handler({ ...running });
  }, 4000);
  return () => window.clearInterval(timer);
}

export function demoTaskEvents(onRun: (run: TaskRun) => void, onLines: (batch: TaskLineBatch) => void): () => void {
  runListeners.add(onRun);
  lineListeners.add(onLines);
  return () => { runListeners.delete(onRun); lineListeners.delete(onLines); };
}

// Settings, organized like a desktop app's preferences: a section list on the
// left, one focused page on the right. Everything is stored on this machine.
import { Bell, Bot, Bug, Check, Download, ExternalLink, FolderSearch, Info, Lock, Palette, RefreshCw, RotateCcw, ScanSearch, SlidersHorizontal } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { openExternal, type AgentId, type UpdateInfo } from "../bridge";
import { THEMES, type ThemePreference } from "../theme";
import { ACCENTS, SCALES, type Appearance, type Density, type Motion } from "../lib/appearance";
import { AGENTS } from "../lib/agents";
import { platformExamples } from "../lib/platform";
import { usePrefs, type StartScreen } from "../lib/prefs";
import { AiClientSetup } from "./ai-clients";
import { McpSetup } from "./project-ai";
import { PageHeader } from "./page-header";
import { PathInput } from "./path-input";

export const REPO_URL = "https://github.com/ojaseminem/Pipeline-OS";

const SECTIONS = [
  { id: "general", label: "General", icon: SlidersHorizontal },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "apps", label: "Apps & projects", icon: FolderSearch },
  { id: "ai", label: "AI & automation", icon: Bot },
  { id: "updates", label: "Updates", icon: Download },
  { id: "privacy", label: "Privacy", icon: Lock },
  { id: "about", label: "About", icon: Info },
] as const;
type SectionId = (typeof SECTIONS)[number]["id"];

function Group({ title, description, children, actions }: { title: string; description?: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <Card><CardContent className="space-y-4 p-5">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1"><h2 className="text-base font-semibold">{title}</h2>{description ? <p className="text-sm text-muted-foreground">{description}</p> : null}</div>
        {actions}
      </div>
      {children}
    </CardContent></Card>
  );
}

/** A labeled setting row: label and help on the left, control on the right. */
function Row({ label, description, children }: { label: string; description?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-2 border-t border-border pt-4 first:border-t-0 first:pt-0">
      <div className="min-w-48 flex-1"><div className="text-sm font-medium">{label}</div>{description ? <div className="text-xs text-muted-foreground">{description}</div> : null}</div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (value: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button role="switch" aria-checked={checked} aria-label={label} disabled={disabled} onClick={() => onChange(!checked)}
      className={cn("relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors disabled:opacity-50", checked ? "bg-primary" : "bg-secondary ring-1 ring-border")}>
      <span className={cn("inline-block size-4 rounded-full bg-background shadow transition-transform", checked ? "translate-x-4.5" : "translate-x-0.5")} />
    </button>
  );
}

function Segmented<T extends string | number>({ value, options, onChange, label }: { value: T; options: Array<{ value: T; label: string }>; onChange: (value: T) => void; label: string }) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-lg border border-border bg-secondary/40 p-0.5">
      {options.map((option) => (
        <button key={String(option.value)} role="radio" aria-checked={value === option.value} onClick={() => onChange(option.value)}
          className={cn("rounded-md px-3 py-1 text-sm transition-colors", value === option.value ? "bg-background font-medium text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground")}>
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function ThemePicker({ value, onChange }: { value: ThemePreference; onChange: (value: ThemePreference) => void }) {
  const options: Array<{ id: ThemePreference; name: string; description: string; swatch: readonly string[] }> = [
    { id: "system", name: "Match system", description: "Midnight or Daylight, following your OS.", swatch: ["#151a24", "#f7f8fa", "#4fd1c5"] },
    ...THEMES,
  ];
  return (
    <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4" role="radiogroup" aria-label="Theme">
      {options.map((option) => (
        <button
          key={option.id}
          role="radio"
          aria-checked={value === option.id}
          onClick={() => onChange(option.id)}
          className={cn("group rounded-lg border p-2.5 text-left transition-colors", value === option.id ? "border-primary ring-1 ring-primary" : "border-border hover:border-primary/40")}
        >
          <span className="mb-2 flex h-12 overflow-hidden rounded-md border border-border" aria-hidden>
            <span className="flex-[2]" style={{ background: option.swatch[0] }} />
            <span className="flex flex-[3] items-end p-1.5" style={{ background: option.swatch[1] }}><span className="h-2 w-8 rounded-full" style={{ background: option.swatch[2] }} /></span>
          </span>
          <span className="flex items-center gap-1.5 text-sm font-medium">{option.name}{value === option.id ? <Check size={13} className="text-primary" /> : null}</span>
          <span className="mt-0.5 block text-xs text-muted-foreground">{option.description}</span>
        </button>
      ))}
    </div>
  );
}

export type SettingsProps = {
  preference: ThemePreference;
  setPreference: (value: ThemePreference) => void;
  appearance: Appearance;
  onAppearanceChange: (patch: Partial<Appearance>) => void;
  onAppearanceReset: () => void;
  appVersion: string;
  channel: "stable" | "dev";
  update: UpdateInfo | null;
  autoUpdate: boolean;
  onAutoUpdateChange: (value: boolean) => void;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
  autostart: boolean;
  autostartAvailable: boolean;
  onAutostartChange: (value: boolean) => void;
  onScanApps: () => void;
  onFindProjects: () => void;
  runtimeLabel: string;
};

function readSection(): SectionId {
  try {
    const stored = localStorage.getItem("vantadeck.settingsSection");
    return SECTIONS.some((section) => section.id === stored) ? (stored as SectionId) : "general";
  } catch {
    return "general";
  }
}

export function SettingsScreen(props: SettingsProps) {
  const [section, setSectionState] = useState<SectionId>(readSection);
  const setSection = (id: SectionId) => {
    setSectionState(id);
    try { localStorage.setItem("vantadeck.settingsSection", id); } catch { /* storage unavailable */ }
  };
  const current = SECTIONS.find((item) => item.id === section) ?? SECTIONS[0];
  return (
    <section className="pos-enter space-y-5">
      <PageHeader title="Settings" description="Preferences for this machine. Project settings live with each project in .pipelineos." />
      <div className="grid gap-5 md:grid-cols-[200px_minmax(0,1fr)]">
        <nav aria-label="Settings sections" className="flex gap-1 overflow-x-auto md:flex-col">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button key={id} onClick={() => setSection(id)} aria-current={section === id ? "page" : undefined}
              className={cn("flex shrink-0 items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm transition-colors", section === id ? "bg-secondary font-medium text-foreground" : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground")}>
              <Icon size={16} aria-hidden /> {label}
            </button>
          ))}
        </nav>
        <div className="min-w-0 space-y-4" aria-label={current.label}>
          {section === "general" ? <GeneralSettings {...props} /> : null}
          {section === "appearance" ? <AppearanceSettings {...props} /> : null}
          {section === "apps" ? <AppsSettings {...props} /> : null}
          {section === "ai" ? <AiSettings /> : null}
          {section === "updates" ? <UpdateSettings {...props} /> : null}
          {section === "privacy" ? <PrivacySettings /> : null}
          {section === "about" ? <AboutSettings {...props} /> : null}
        </div>
      </div>
    </section>
  );
}

function GeneralSettings(props: SettingsProps) {
  const [prefs, setPrefs] = usePrefs();
  return (
    <>
      <Group title="Startup">
        <Row label="Start Pipeline OS when I sign in" description="Opens quietly so your projects and quick launch are ready.">
          <Switch label="Start Pipeline OS when I sign in" checked={props.autostart} disabled={!props.autostartAvailable} onChange={props.onAutostartChange} />
        </Row>
        <Row label="Open to" description="The screen shown when Pipeline OS starts.">
          <Segmented<StartScreen> label="Open to" value={prefs.startScreen} onChange={(startScreen) => setPrefs({ startScreen })}
            options={[{ value: "Home", label: "Home" }, { value: "Projects", label: "Projects" }, { value: "Activity", label: "Activity" }]} />
        </Row>
      </Group>
      <Group title="Notifications" description="Shown inside Pipeline OS, bottom-right.">
        <Row label="Pipeline task finished" description="Builds, renders, and exports started from Pipeline OS, the CLI, or an AI agent.">
          <Switch label="Notify when a pipeline task finishes" checked={prefs.taskToasts} onChange={(taskToasts) => setPrefs({ taskToasts })} />
        </Row>
        <Row label="Editor job finished" description="Lighting builds, cooks, and player builds you started in Unity or Unreal.">
          <Switch label="Notify when an editor job finishes" checked={prefs.engineToasts} disabled={!prefs.engineWatch} onChange={(engineToasts) => setPrefs({ engineToasts })} />
        </Row>
      </Group>
    </>
  );
}

function AppearanceSettings({ preference, setPreference, appearance, onAppearanceChange, onAppearanceReset }: SettingsProps) {
  return (
    <>
      <Group title="Theme" description="Applies instantly. High contrast boosts text and focus outlines.">
        <ThemePicker value={preference} onChange={setPreference} />
      </Group>
      <Group title="Look and feel" actions={<Button variant="ghost" size="sm" onClick={onAppearanceReset}><RotateCcw size={14} /> Reset</Button>}>
        <Row label="Accent color" description="Buttons, selection, and focus rings. Theme default uses each theme's own accent.">
          <div role="radiogroup" aria-label="Accent color" className="flex flex-wrap gap-1.5">
            {ACCENTS.map((accent) => (
              <button key={accent.id} role="radio" aria-checked={appearance.accent === accent.id} aria-label={accent.name} title={accent.name} onClick={() => onAppearanceChange({ accent: accent.id })}
                className={cn("flex size-7 items-center justify-center rounded-full border transition-transform hover:scale-110", appearance.accent === accent.id ? "border-foreground" : "border-border")}
                style={{ background: accent.color ?? "conic-gradient(#4fd1c5, #e8a33d, #f97316, #5b5bd6, #4fd1c5)" }}>
                {appearance.accent === accent.id ? <Check size={13} className="text-white mix-blend-difference" /> : null}
              </button>
            ))}
          </div>
        </Row>
        <Row label="Density" description="Compact fits more projects and apps on screen.">
          <Segmented<Density> label="Density" value={appearance.density} onChange={(density) => onAppearanceChange({ density })} options={[{ value: "comfortable", label: "Comfortable" }, { value: "compact", label: "Compact" }]} />
        </Row>
        <Row label="Interface size" description="Scales text and controls.">
          <Segmented<number> label="Interface size" value={appearance.scale} onChange={(scale) => onAppearanceChange({ scale })} options={SCALES.map((scale) => ({ value: scale, label: `${scale}%` }))} />
        </Row>
        <Row label="Motion" description="System follows your OS's reduce-motion setting.">
          <Segmented<Motion> label="Motion" value={appearance.motion} onChange={(motion) => onAppearanceChange({ motion })} options={[{ value: "system", label: "System" }, { value: "full", label: "Full" }, { value: "reduced", label: "Reduced" }]} />
        </Row>
        <Row label="Sidebar labels" description="Turn off for an icon-only sidebar and more room.">
          <Switch label="Show sidebar labels" checked={appearance.sidebarLabels} onChange={(sidebarLabels) => onAppearanceChange({ sidebarLabels })} />
        </Row>
      </Group>
    </>
  );
}

function AppsSettings({ onScanApps, onFindProjects }: SettingsProps) {
  const [prefs, setPrefs] = usePrefs();
  const examples = platformExamples();
  return (
    <>
      <Group title="Application scanning" description={`Scan checks ${examples.standardLocations}, including every installed version.`} actions={<Button variant="outline" size="sm" onClick={onScanApps}><ScanSearch size={14} /> Scan now</Button>}>
        <div className="space-y-1.5">
          <div className="text-sm font-medium">Extra folders</div>
          <PathInput ariaLabel="Extra folders to scan" directory multi value={prefs.extraScanFolders} onChange={(extraScanFolders) => setPrefs({ extraScanFolders })} placeholder={examples.toolFolders} className="w-full" />
          <p className="text-xs text-muted-foreground">Included in every scan. Separate folders with <code>;</code>.</p>
        </div>
      </Group>
      <Group title="Projects" actions={<Button variant="outline" size="sm" onClick={onFindProjects}><RefreshCw size={14} /> Look now</Button>}>
        <Row label="Find new projects on launch" description="Checks Unity Hub, Unreal's recent projects, Godot's project list, and common project folders, then suggests what isn't added yet.">
          <Switch label="Find new projects on launch" checked={prefs.discoverOnLaunch} onChange={(discoverOnLaunch) => setPrefs({ discoverOnLaunch })} />
        </Row>
        <Row label="Show editor activity" description="Reads Unity's Editor.log and each Unreal project's Saved/Logs to show lighting builds, shader compiles, cooks, and player builds in Activity. Read-only.">
          <Switch label="Show editor activity" checked={prefs.engineWatch} onChange={(engineWatch) => setPrefs({ engineWatch })} />
        </Row>
      </Group>
    </>
  );
}

function AiSettings() {
  const [prefs, setPrefs] = usePrefs();
  return (
    <>
      <Group title="Connect AI apps" description="Finds AI apps on this computer and adds the Pipeline OS MCP server to each, so they can list projects, read briefs, run trusted tasks, and update the board.">
        <AiClientSetup />
      </Group>
      <Group title="AI agents in projects">
        <Row label="Default agent" description="Started by “Open … here” in a project's AI tab.">
          <Segmented<AgentId> label="Default agent" value={prefs.defaultAgent} onChange={(defaultAgent) => setPrefs({ defaultAgent })} options={AGENTS.map((agent) => ({ value: agent.id, label: agent.name }))} />
        </Row>
        <Row label="Refresh the project brief first" description={<>Rewrites <code>.pipelineos/context.md</code> before an agent starts, so it knows the engine, folders to leave alone, and how to build.</>}>
          <Switch label="Refresh the project brief before starting an agent" checked={prefs.writeBriefOnAgentStart} onChange={(writeBriefOnAgentStart) => setPrefs({ writeBriefOnAgentStart })} />
        </Row>
      </Group>
      <Group title="Automation" description="Scripts, CI, and other tools use the same commands as AI apps.">
        <p className="text-sm text-muted-foreground">Every command takes <code>--json</code> for stable, machine-readable output, for example <code>vantadeck --json issues list --all</code> or <code>vantadeck tasks run build --project .</code>. Agents can only run task files you've trusted.</p>
        <details className="text-sm"><summary className="cursor-pointer text-muted-foreground hover:text-foreground">Manual MCP setup</summary><div className="pt-3"><McpSetup /></div></details>
      </Group>
    </>
  );
}

function UpdateSettings({ update, channel, appVersion, autoUpdate, onAutoUpdateChange, onCheckUpdate, onInstallUpdate }: SettingsProps) {
  return (
    <Group title="Updates" description={channel === "dev" ? "You're on the Dev channel: early builds from active development, installed separately from the stable app." : "Signed releases, verified against the Pipeline OS key before installing."}>
      {update?.available
        ? <p className="text-sm">Version <strong>{update.version}</strong> is available (you have {update.currentVersion}).{update.notes ? <><br /><span className="text-muted-foreground">{update.notes}</span></> : null}</p>
        : <p className="text-sm text-muted-foreground">{update ? `You're up to date (${update.currentVersion}).` : `Installed version ${appVersion}.`}</p>}
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={onCheckUpdate}><RefreshCw size={15} /> Check for updates</Button>
        {update?.available ? <Button variant="outline" onClick={() => void openExternal(channel === "dev" ? `${REPO_URL}/releases/tag/dev-latest` : `${REPO_URL}/releases/tag/v${update.version}`)}><ExternalLink size={15} /> Release notes</Button> : null}
        {update?.available ? <Button onClick={onInstallUpdate}><Download size={15} /> Install &amp; restart</Button> : null}
      </div>
      <Row label="Check for updates when Pipeline OS starts">
        <Switch label="Check for updates when Pipeline OS starts" checked={autoUpdate} onChange={onAutoUpdateChange} />
      </Row>
    </Group>
  );
}

function PrivacySettings() {
  return (
    <Group title="Privacy" description="Local-first by design.">
      <ul className="space-y-1.5 text-sm text-muted-foreground">
        <li className="flex gap-2"><Check size={15} className="mt-0.5 shrink-0 text-success" /> No account, telemetry, analytics, or ads.</li>
        <li className="flex gap-2"><Check size={15} className="mt-0.5 shrink-0 text-success" /> Projects, tasks, and issues stay in your project folders and on this machine.</li>
        <li className="flex gap-2"><Check size={15} className="mt-0.5 shrink-0 text-success" /> Network is only used when you ask: update checks, Git sync, tracker sync, and opening links.</li>
        <li className="flex gap-2"><Check size={15} className="mt-0.5 shrink-0 text-success" /> Tracker tokens are kept in your OS keychain, never in project files.</li>
        <li className="flex gap-2"><Bell size={15} className="mt-0.5 shrink-0 text-muted-foreground" /> Editor activity only reads engine log files; nothing leaves your machine.</li>
      </ul>
    </Group>
  );
}

function AboutSettings({ appVersion, channel, runtimeLabel }: SettingsProps) {
  const bugUrl = `${REPO_URL}/issues/new?labels=bug&title=${encodeURIComponent("[Bug] ")}&body=${encodeURIComponent(`**What happened?**\n\n**What did you expect?**\n\n**Steps to reproduce**\n1. \n\n---\nPipeline OS ${appVersion} (${channel}) · ${navigator.userAgent}`)}`;
  return (
    <Group title="About Pipeline OS" description={`Version ${appVersion}${channel === "dev" ? " (Dev channel)" : ""} · ${runtimeLabel}`}>
      <div className="flex flex-wrap gap-2">
        <Button variant="outline" size="sm" onClick={() => void openExternal(bugUrl)}><Bug size={14} /> Report a bug</Button>
        <Button variant="outline" size="sm" onClick={() => void openExternal(`${REPO_URL}/discussions`)}><ExternalLink size={14} /> Ideas &amp; discussions</Button>
        <Button variant="outline" size="sm" onClick={() => void openExternal(REPO_URL)}><ExternalLink size={14} /> Source code</Button>
      </div>
      <p className="text-xs text-muted-foreground">Free and open source under the MIT license.</p>
    </Group>
  );
}

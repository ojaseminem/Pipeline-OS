import { Bot, Bug, Check, Download, ExternalLink, Info, Lock, Palette, Power, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { openExternal, type UpdateInfo } from "../bridge";
import { THEMES, type ThemePreference } from "../theme";
import { McpSetup } from "./project-ai";
import { PageHeader } from "./page-header";

export const REPO_URL = "https://github.com/ojaseminem/Pipeline-OS";

function Section({ icon: Icon, title, description, children, className }: { icon: typeof Info; title: string; description?: string; children: ReactNode; className?: string }) {
  return (
    <Card className={className}><CardContent className="space-y-3 p-5">
      <div className="flex items-start gap-2.5">
        <Icon size={18} className="mt-0.5 shrink-0 text-primary" aria-hidden />
        <div><h2 className="text-base font-semibold">{title}</h2>{description ? <p className="text-sm text-muted-foreground">{description}</p> : null}</div>
      </div>
      {children}
    </CardContent></Card>
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
  runtimeLabel: string;
};

export function SettingsScreen(props: SettingsProps) {
  const { update, channel } = props;
  const bugUrl = `${REPO_URL}/issues/new?labels=bug&title=${encodeURIComponent("[Bug] ")}&body=${encodeURIComponent(`**What happened?**\n\n**What did you expect?**\n\n**Steps to reproduce**\n1. \n\n---\nPipeline OS ${props.appVersion} (${channel}) · ${navigator.userAgent}`)}`;
  return (
    <section className="pos-enter space-y-5">
      <PageHeader title="Settings" description="Appearance, updates, AI integrations, and privacy. Everything here is stored on this machine." />
      <Section icon={Palette} title="Appearance" description="Themes apply instantly. High contrast boosts text and focus outlines for accessibility.">
        <ThemePicker value={props.preference} onChange={props.setPreference} />
      </Section>
      <div className="grid gap-4 lg:grid-cols-2">
        <Section icon={Bot} title="AI & automation" description="Let AI apps and scripts use Pipeline OS.">
          <McpSetup />
          <p className="text-xs text-muted-foreground">Scripts and CI can call the same CLI with <code>--json</code> for stable, machine-readable output, for example <code>vantadeck --json issues list --all</code>.</p>
        </Section>
        <div className="space-y-4">
          <Section icon={Download} title="Updates" description={channel === "dev" ? "You're on the Dev channel: early builds from active development, installed separately from the stable app." : "Signed releases, verified against the Pipeline OS key before installing."}>
            {update?.available
              ? <p className="text-sm">Version <strong>{update.version}</strong> is available (you have {update.currentVersion}).{update.notes ? <><br /><span className="text-muted-foreground">{update.notes}</span></> : null}</p>
              : <p className="text-sm text-muted-foreground">{update ? `You're up to date (${update.currentVersion}).` : `Installed version ${props.appVersion}.`}</p>}
            <div className="flex flex-wrap gap-2">
              <Button variant="outline" onClick={props.onCheckUpdate}><RefreshCw size={15} /> Check for updates</Button>
              {update?.available ? <Button variant="outline" onClick={() => void openExternal(channel === "dev" ? `${REPO_URL}/releases/tag/dev-latest` : `${REPO_URL}/releases/tag/v${update.version}`)}><ExternalLink size={15} /> Release notes</Button> : null}
              {update?.available ? <Button onClick={props.onInstallUpdate}><Download size={15} /> Install &amp; restart</Button> : null}
            </div>
            <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={props.autoUpdate} onChange={(event) => props.onAutoUpdateChange(event.target.checked)} className="size-4 accent-[var(--primary)]" /> Check for updates when Pipeline OS starts</label>
          </Section>
          <Section icon={Power} title="Startup">
            <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={props.autostart} disabled={!props.autostartAvailable} onChange={(event) => props.onAutostartChange(event.target.checked)} className="size-4 accent-[var(--primary)]" /> Start Pipeline OS when I sign in</label>
          </Section>
        </div>
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <Section icon={Lock} title="Privacy" description="Local-first by design.">
          <ul className="space-y-1.5 text-sm text-muted-foreground">
            <li className="flex gap-2"><Check size={15} className="mt-0.5 shrink-0 text-success" /> No account, telemetry, analytics, or ads.</li>
            <li className="flex gap-2"><Check size={15} className="mt-0.5 shrink-0 text-success" /> Projects, tasks, and issues stay in your project folders and on this machine.</li>
            <li className="flex gap-2"><Check size={15} className="mt-0.5 shrink-0 text-success" /> Network is only used when you ask: update checks, Git sync, and opening links.</li>
          </ul>
        </Section>
        <Section icon={Info} title="About" description={`Pipeline OS ${props.appVersion}${channel === "dev" ? " (Dev channel)" : ""} · ${props.runtimeLabel}`}>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" onClick={() => void openExternal(bugUrl)}><Bug size={14} /> Report a bug</Button>
            <Button variant="outline" size="sm" onClick={() => void openExternal(`${REPO_URL}/discussions`)}><ExternalLink size={14} /> Ideas &amp; discussions</Button>
            <Button variant="outline" size="sm" onClick={() => void openExternal(REPO_URL)}><ExternalLink size={14} /> Source code</Button>
          </div>
          <p className="text-xs text-muted-foreground">Free and open source under the MIT license.</p>
        </Section>
      </div>
    </section>
  );
}

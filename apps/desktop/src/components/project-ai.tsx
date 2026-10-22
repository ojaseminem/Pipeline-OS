// Project "AI" tab: start an AI coding agent or a terminal in the project,
// preview the project brief agents receive, and connect AI apps over MCP.
import { useQuery } from "@tanstack/react-query";
import { Bot, Check, ChevronDown, Clipboard, FileDown, RefreshCw, Sparkles, SquareTerminal } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { studioApi, studioAvailable, type AgentId } from "../bridge";
import { EmptyState } from "./empty-state";
import { usePrefs } from "../lib/prefs";
import { AGENTS } from "../lib/agents";
import { AiClientSetup } from "./ai-clients";


function CopyBlock({ label, value }: { label: string; value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="space-y-1">
      <div className="text-xs font-medium text-muted-foreground">{label}</div>
      <div className="flex items-start gap-2 rounded-md border border-border bg-muted/30 p-2">
        <code className="min-w-0 flex-1 whitespace-pre-wrap break-all text-xs">{value}</code>
        <Button variant="ghost" size="icon" className="h-7 w-7 shrink-0" aria-label={`Copy ${label}`} onClick={() => { void navigator.clipboard?.writeText(value); setCopied(true); window.setTimeout(() => setCopied(false), 1500); }}>{copied ? <Check size={14} className="text-success" /> : <Clipboard size={14} />}</Button>
      </div>
    </div>
  );
}

/** Copy-ready MCP setup for Claude Code and Claude Desktop (also in Settings). */
export function McpSetup() {
  const info = useQuery({ queryKey: ["cli-info"], queryFn: () => studioApi.cliInfo(), enabled: studioAvailable(), staleTime: Infinity });
  const cli = info.data?.cliPath ?? "vantadeck";
  const args = info.data?.mcpArgs ?? ["mcp"];
  const quoted = /\s/.test(cli) ? `"${cli}"` : cli;
  const desktopConfig = JSON.stringify({ mcpServers: { "pipeline-os": { command: cli, args } } }, null, 2);
  return (
    <div className="space-y-3">
      <p className="text-sm text-muted-foreground">Pipeline OS includes an MCP server, so AI apps can list your projects, read project briefs, run pipeline tasks, and update the board, with your approval for each action.</p>
      <CopyBlock label="Claude Code (terminal)" value={`claude mcp add pipeline-os -- ${quoted} ${args.join(" ")}`} />
      <CopyBlock label="Claude Desktop (claude_desktop_config.json)" value={desktopConfig} />
      {!info.data?.cliPath ? <p className="text-xs text-muted-foreground">The <code>vantadeck</code> CLI ships with installed builds. When running from source, build it with <code>cargo build -p vantadeck</code>.</p> : null}
    </div>
  );
}

export function ProjectAi({ projectPath, projectName }: { projectPath: string; projectName: string }) {
  const available = studioAvailable();
  const brief = useQuery({ queryKey: ["brief", projectPath], queryFn: () => studioApi.brief(projectPath), enabled: available });
  const [prefs] = usePrefs();
  const primary = AGENTS.find((agent) => agent.id === prefs.defaultAgent) ?? AGENTS[0];

  async function open(agent?: AgentId) {
    try {
      // Refresh context.md first so the agent starts with an up-to-date brief.
      if (agent && prefs.writeBriefOnAgentStart) await studioApi.brief(projectPath, true).catch(() => undefined);
      await studioApi.openTerminal(projectPath, agent);
      const name = AGENTS.find((item) => item.id === agent)?.name;
      toast.success(name ? `Starting ${name} in ${projectName}` : `Opened a terminal in ${projectName}`, { description: name ? "If it doesn't start, install the agent's CLI and make sure it's on your PATH." : undefined });
    } catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }

  async function save() {
    try {
      const result = await studioApi.brief(projectPath, true);
      toast.success("Brief saved", { description: result.writtenTo ?? undefined });
    } catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }

  if (!available) return <EmptyState icon={Bot} title="AI tools live in the desktop app" description="Open Pipeline OS on your machine to start AI agents in a project and connect AI apps." />;

  return (
    <div className="grid gap-4 xl:grid-cols-[1fr_1.2fr]">
      <div className="space-y-4">
        <Card><CardContent className="space-y-3 p-5">
          <div className="flex items-center gap-2"><Sparkles size={18} className="text-primary" aria-hidden /><h2 className="text-base font-semibold">Work on this project with AI</h2></div>
          <p className="text-sm text-muted-foreground">Starts an agent in the project folder with the <code>vantadeck</code> CLI on its PATH. Point it at <code>.pipelineos/context.md</code>, or connect it over MCP, so it knows the engine, the generated folders to leave alone, and how to build.</p>
          <div className="flex flex-wrap gap-2">
            <Button onClick={() => void open(primary.id)}><Bot size={15} /> Open {primary.name} here</Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild><Button variant="outline">Other agents <ChevronDown size={14} className="opacity-70" /></Button></DropdownMenuTrigger>
              <DropdownMenuContent>
                {AGENTS.filter((agent) => agent.id !== primary.id).map((agent) => <DropdownMenuItem key={agent.id} onClick={() => void open(agent.id)}>{agent.name} <span className="ml-auto font-mono text-xs text-muted-foreground">{agent.hint}</span></DropdownMenuItem>)}
              </DropdownMenuContent>
            </DropdownMenu>
            <Button variant="outline" onClick={() => void open()}><SquareTerminal size={15} /> Terminal</Button>
          </div>
        </CardContent></Card>
        <Card><CardContent className="space-y-3 p-5">
          <h2 className="text-base font-semibold">Connect AI apps</h2>
          <p className="text-sm text-muted-foreground">Give Claude, Cursor, VS Code, Codex and other AI apps the Pipeline OS tools: list projects, read briefs, run trusted tasks, and update the board.</p>
          <AiClientSetup />
          <details className="group text-sm"><summary className="cursor-pointer text-muted-foreground hover:text-foreground">Set up manually</summary><div className="pt-3"><McpSetup /></div></details>
        </CardContent></Card>
      </div>
      <Card><CardContent className="flex h-full flex-col gap-3 p-5">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-base font-semibold">Project brief</h2>
          <span className="text-xs text-muted-foreground">What agents and new teammates see first</span>
          <span className="ml-auto flex gap-1.5">
            <Button variant="ghost" size="icon" aria-label="Refresh brief" onClick={() => void brief.refetch()}><RefreshCw size={14} className={brief.isFetching ? "animate-spin" : ""} /></Button>
            <Button variant="outline" size="sm" disabled={!brief.data} onClick={() => { void navigator.clipboard?.writeText(brief.data?.markdown ?? ""); toast.success("Brief copied. Paste it into any AI chat."); }}><Clipboard size={14} /> Copy</Button>
            <Button variant="outline" size="sm" onClick={() => void save()}><FileDown size={14} /> Save to context.md</Button>
          </span>
        </div>
        {brief.isError ? <p className="text-sm text-destructive">{String(brief.error)}</p> : null}
        <pre className="min-h-64 flex-1 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-muted/20 p-3 font-mono text-xs leading-relaxed">{brief.isLoading ? "Building brief…" : brief.data?.markdown}</pre>
      </CardContent></Card>
    </div>
  );
}

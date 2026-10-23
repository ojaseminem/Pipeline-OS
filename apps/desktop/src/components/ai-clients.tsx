// One-click MCP setup: finds AI apps on this machine (Claude Desktop, Claude
// Code, Cursor, VS Code, Windsurf, Codex, Gemini CLI) and registers the
// bundled Pipeline OS server in each one's own config file, with a backup.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Link2, Loader2, RefreshCw, Unlink, Wand2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { studioApi, studioAvailable, type AiClientStatus } from "../bridge";

const AGENT_NAMES: Record<string, string> = { claude: "Claude Code", codex: "Codex CLI", gemini: "Gemini CLI", aider: "Aider" };

export function AiClientSetup() {
  const queryClient = useQueryClient();
  const state = useQuery({ queryKey: ["ai-clients"], queryFn: () => studioApi.aiClients(), enabled: studioAvailable() });
  const [busy, setBusy] = useState<string | null>(null);
  const clients = state.data?.clients ?? [];
  const pending = clients.filter((client) => client.installed && !client.configured && !client.problem);

  async function connect(ids: string[]) {
    if (!ids.length) return;
    setBusy(ids.length > 1 ? "all" : ids[0]);
    try {
      const results = await studioApi.aiConfigure(ids);
      const failed = results.filter((result) => !result.ok);
      const done = results.length - failed.length;
      if (done) toast.success(`Connected ${done} app${done === 1 ? "" : "s"} to Pipeline OS`, { description: "Restart the app (or start a new chat) to load the Pipeline OS tools." });
      for (const failure of failed) toast.error(`Couldn't connect ${clients.find((c) => c.id === failure.id)?.name ?? failure.id}`, { description: failure.error ?? undefined });
    } catch (error) {
      toast.error("Couldn't connect AI apps", { description: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusy(null);
      await queryClient.invalidateQueries({ queryKey: ["ai-clients"] });
    }
  }

  async function disconnect(client: AiClientStatus) {
    setBusy(client.id);
    try {
      await studioApi.aiUnconfigure(client.id);
      toast.success(`Disconnected ${client.name}`);
    } catch (error) {
      toast.error(`Couldn't disconnect ${client.name}`, { description: error instanceof Error ? error.message : String(error) });
    } finally {
      setBusy(null);
      await queryClient.invalidateQueries({ queryKey: ["ai-clients"] });
    }
  }

  if (!studioAvailable()) return <p className="text-sm text-muted-foreground">AI app setup runs in the desktop app.</p>;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => void connect(pending.map((client) => client.id))} disabled={!pending.length || !state.data?.canConfigure || busy !== null}>
          {busy === "all" ? <Loader2 size={15} className="animate-spin" /> : <Wand2 size={15} />}
          {pending.length ? `Connect all (${pending.length})` : "All found apps connected"}
        </Button>
        <Button variant="outline" onClick={() => void state.refetch()} disabled={state.isFetching}><RefreshCw size={15} className={state.isFetching ? "animate-spin" : ""} /> Scan again</Button>
        {state.data && !state.data.canConfigure ? <span className="text-xs text-warning">The bundled CLI wasn't found, so apps can't be connected from this build.</span> : null}
      </div>
      <ul className="divide-y divide-border rounded-lg border border-border">
        {state.isLoading ? <li className="flex items-center gap-2 p-3 text-sm text-muted-foreground"><Loader2 size={14} className="animate-spin" /> Looking for AI apps…</li> : null}
        {clients.map((client) => (
          <li key={client.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 px-3 py-2.5">
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-2 text-sm font-medium">
                {client.name}
                {client.configured ? <Badge variant="secondary" className="gap-1 text-success"><Check size={11} /> Connected</Badge>
                  : client.installed ? <Badge variant="outline">Found</Badge>
                  : <Badge variant="outline" className="text-muted-foreground">Not installed</Badge>}
              </span>
              <span className="block truncate font-mono text-[11px] text-muted-foreground" title={client.configPath}>{client.problem ?? client.configPath}</span>
            </span>
            {client.configured
              ? <Button variant="ghost" size="sm" disabled={busy !== null} onClick={() => void disconnect(client)}>{busy === client.id ? <Loader2 size={14} className="animate-spin" /> : <Unlink size={14} />} Disconnect</Button>
              : client.installed
                ? <Button variant="outline" size="sm" disabled={busy !== null || !state.data?.canConfigure || Boolean(client.problem)} onClick={() => void connect([client.id])}>{busy === client.id ? <Loader2 size={14} className="animate-spin" /> : <Link2 size={14} />} Connect</Button>
                : null}
          </li>
        ))}
      </ul>
      {state.data ? (
        <p className="text-xs text-muted-foreground">
          Agent CLIs on this machine: {state.data.agentClis.length ? state.data.agentClis.map((id) => AGENT_NAMES[id] ?? id).join(", ") : "none found"}.
          {" "}Each config file is backed up next to itself (<code>.pipelineos-backup</code>) before it's changed, and only the <code>pipeline-os</code> entry is touched.
        </p>
      ) : null}
    </div>
  );
}

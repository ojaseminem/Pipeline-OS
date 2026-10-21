// Connect a project's board to an external issue tracker, or host your own.
import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, Link2, Loader2, RefreshCw, Server, Unlink } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { browsePath, isNativeRuntime, openExternal, studioApi, type TrackerConfig, type TrackerKind } from "../bridge";
import { confirmAction } from "../lib/confirm";
import { TRACKERS } from "../lib/trackers";

export function TrackerButton({ projectPath }: { projectPath: string }) {
  const queryClient = useQueryClient();
  const state = useQuery({ queryKey: ["tracker", projectPath], queryFn: () => studioApi.tracker(projectPath) });
  const [open, setOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const config = state.data?.config ?? null;
  const meta = config ? TRACKERS.find((tracker) => tracker.kind === config.kind) : null;

  async function sync() {
    setSyncing(true);
    try {
      const result = await studioApi.syncTracker(projectPath);
      toast.success(`Synced with ${meta?.name ?? "tracker"}`, { description: `${result.created} new · ${result.updated} updated · ${result.unchanged} unchanged` });
      await queryClient.invalidateQueries({ queryKey: ["issues", projectPath] });
      await queryClient.invalidateQueries({ queryKey: ["issues-everywhere"] });
    } catch (error) {
      toast.error("Sync failed", { description: error instanceof Error ? error.message : String(error) });
    } finally { setSyncing(false); }
  }

  return (
    <>
      {config ? (
        <span className="flex items-center">
          <Button variant="outline" size="sm" className="rounded-r-none" onClick={() => void sync()} disabled={syncing} title={`Pull issues from ${meta?.name}`}>
            {syncing ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />} Sync {meta?.name}
          </Button>
          <Button variant="outline" size="sm" className="-ml-px rounded-l-none px-2" aria-label="Tracker settings" onClick={() => setOpen(true)}><Link2 size={14} /></Button>
        </span>
      ) : <Button variant="outline" size="sm" onClick={() => setOpen(true)}><Link2 size={14} /> Connect tracker</Button>}
      <TrackerDialog open={open} onOpenChange={setOpen} projectPath={projectPath} current={config} hasToken={state.data?.hasToken ?? false} onSaved={() => void queryClient.invalidateQueries({ queryKey: ["tracker", projectPath] })} />
    </>
  );
}

function TrackerDialog({ open, onOpenChange, projectPath, current, hasToken, onSaved }: {
  open: boolean; onOpenChange: (open: boolean) => void; projectPath: string; current: TrackerConfig | null; hasToken: boolean; onSaved: () => void;
}) {
  const [kind, setKind] = useState<TrackerKind>(current?.kind ?? "github");
  const [baseUrl, setBaseUrl] = useState(current?.baseUrl ?? "");
  const [project, setProject] = useState(current?.project ?? "");
  const [query, setQuery] = useState(current?.query ?? "");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<"connect" | "host">("connect");
  const meta = TRACKERS.find((tracker) => tracker.kind === kind)!;

  useEffect(() => {
    if (!open) return;
    setKind(current?.kind ?? "github");
    setBaseUrl(current?.baseUrl ?? "");
    setProject(current?.project ?? "");
    setQuery(current?.query ?? "");
    setToken("");
    setView("connect");
  }, [open, current]);

  async function save() {
    setBusy(true);
    try {
      await studioApi.saveTracker(projectPath, { kind, baseUrl: baseUrl.trim() || null, project: project.trim(), query: query.trim() || null }, token);
      toast.success(`Connected to ${meta.name}`, { description: "Use Sync to pull issues onto the board." });
      onSaved();
      onOpenChange(false);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally { setBusy(false); }
  }

  async function disconnect() {
    if (!(await confirmAction({ title: "Disconnect the tracker?", description: "Imported issues stay on the board. Your token stays in the keychain for other projects on the same server.", confirmLabel: "Disconnect" }))) return;
    await studioApi.saveTracker(projectPath, null).catch((error) => toast.error(String(error)));
    onSaved();
    onOpenChange(false);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{view === "host" ? "Host your own tracker" : "Connect an issue tracker"}</DialogTitle>
          <DialogDescription>{view === "host"
            ? "Forgejo is a free, open-source server for Git with LFS, issues, and pull requests, in one small container. Run it on any studio machine with Docker."
            : "Pull issues from the tracker your team already uses onto this project's board. The connection is saved in .pipelineos/tracker.toml; your token stays in this computer's keychain."}</DialogDescription>
        </DialogHeader>
        {view === "connect" ? (
          <form className="space-y-3" onSubmit={(event) => { event.preventDefault(); void save(); }}>
            <div className="grid grid-cols-3 gap-2 sm:grid-cols-6" role="radiogroup" aria-label="Tracker">
              {TRACKERS.map((tracker) => (
                <button key={tracker.kind} type="button" role="radio" aria-checked={kind === tracker.kind} onClick={() => setKind(tracker.kind)} className={cn("rounded-lg border px-2 py-2 text-center text-xs font-medium transition-colors", kind === tracker.kind ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40")}>{tracker.name}</button>
              ))}
            </div>
            {meta.needsUrl || kind === "github" ? (
              <label className="block space-y-1 text-sm"><span className="font-medium">Server URL{kind === "github" ? <span className="font-normal text-muted-foreground"> (GitHub Enterprise only)</span> : null}</span>
                <Input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder={meta.urlPlaceholder} required={meta.needsUrl} />
              </label>
            ) : null}
            <label className="block space-y-1 text-sm"><span className="font-medium">{meta.projectLabel}</span>
              <Input value={project} onChange={(event) => setProject(event.target.value)} placeholder={meta.projectPlaceholder} required />
            </label>
            {kind === "jira" ? (
              <label className="block space-y-1 text-sm"><span className="font-medium">Extra JQL filter <span className="font-normal text-muted-foreground">(optional)</span></span>
                <Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder='e.g. labels = art AND sprint in openSprints()' />
              </label>
            ) : null}
            <label className="block space-y-1 text-sm"><span className="font-medium">{meta.tokenLabel} {hasToken && current?.kind === kind ? <span className="font-normal text-success">(saved, leave blank to keep)</span> : meta.tokenOptional ? <span className="font-normal text-muted-foreground">(optional for public repos)</span> : null}</span>
              <Input type="password" autoComplete="off" value={token} onChange={(event) => setToken(event.target.value)} placeholder={meta.tokenPlaceholder} />
              <button type="button" className="flex items-center gap-1 text-xs text-primary hover:underline" onClick={() => void openExternal(meta.tokenHelpUrl)}>How to create one <ExternalLink size={11} /></button>
            </label>
            <DialogFooter className="sm:justify-between">
              <span className="flex gap-1">
                {current ? <Button type="button" variant="ghost" onClick={() => void disconnect()}><Unlink size={14} /> Disconnect</Button> : null}
                <Button type="button" variant="ghost" onClick={() => setView("host")}><Server size={14} /> Host your own…</Button>
              </span>
              <span className="flex gap-2">
                <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>Cancel</Button>
                <Button type="submit" disabled={busy || !project.trim()}>{busy ? "Saving…" : current ? "Save" : "Connect"}</Button>
              </span>
            </DialogFooter>
          </form>
        ) : <SelfHost onBack={() => setView("connect")} onUse={(url) => { setKind("gitea"); setBaseUrl(url); setView("connect"); }} />}
      </DialogContent>
    </Dialog>
  );
}

function SelfHost({ onBack, onUse }: { onBack: () => void; onUse: (url: string) => void }) {
  const [dir, setDir] = useState("");
  const [port, setPort] = useState("3000");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ composePath: string; dockerAvailable: boolean; started: boolean; url: string } | null>(null);

  async function create(start: boolean) {
    if (!dir.trim()) { toast.error("Choose a folder for the server's data."); return; }
    setBusy(true);
    try { setResult(await studioApi.selfHostForgejo(dir.trim(), Number(port) || 3000, start)); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  }

  return (
    <div className="space-y-3">
      <ol className="list-decimal space-y-1 pl-5 text-sm text-muted-foreground">
        <li>Pick an empty folder on the machine that will host it (a NAS or always-on PC works).</li>
        <li>Pipeline OS writes a <code>docker-compose.yml</code> and, if Docker is installed, starts it.</li>
        <li>Open the server, create the admin account and an access token, then connect it here as <strong>Gitea / Forgejo</strong>.</li>
      </ol>
      <div className="flex flex-wrap items-end gap-2">
        <label className="min-w-64 flex-1 space-y-1 text-sm"><span className="font-medium">Server folder</span>
          <span className="flex gap-2"><Input value={dir} onChange={(event) => setDir(event.target.value)} placeholder="D:/Servers/forgejo" />
            <Button type="button" variant="outline" disabled={!isNativeRuntime()} onClick={() => void browsePath({ directory: true, title: "Choose a folder for Forgejo" }).then((value) => value && setDir(value))}>Browse…</Button></span>
        </label>
        <label className="w-24 space-y-1 text-sm"><span className="font-medium">Port</span><Input value={port} onChange={(event) => setPort(event.target.value.replace(/\D/g, ""))} /></label>
      </div>
      {result ? (
        <div className="rounded-lg border border-border bg-muted/30 p-3 text-sm">
          <p>Wrote <code>{result.composePath}</code>.</p>
          {result.started ? <p className="text-success">Forgejo is starting at {result.url}.</p>
            : result.dockerAvailable ? <p>Start it with <code>docker compose up -d</code> in that folder.</p>
            : <p className="text-warning">Docker wasn't found. Install Docker Desktop, then run <code>docker compose up -d</code> in that folder.</p>}
          <div className="mt-2 flex gap-2">
            <Button size="sm" variant="outline" onClick={() => void openExternal(result.url)}><ExternalLink size={13} /> Open {result.url}</Button>
            <Button size="sm" onClick={() => onUse(result.url)}>Connect this server</Button>
          </div>
        </div>
      ) : null}
      <DialogFooter className="sm:justify-between">
        <Button variant="ghost" onClick={onBack}>Back</Button>
        <span className="flex gap-2">
          <Button variant="outline" disabled={busy} onClick={() => void create(false)}>Only write the file</Button>
          <Button disabled={busy} onClick={() => void create(true)}>{busy ? <Loader2 size={14} className="animate-spin" /> : <Server size={14} />} Create and start</Button>
        </span>
      </DialogFooter>
    </div>
  );
}

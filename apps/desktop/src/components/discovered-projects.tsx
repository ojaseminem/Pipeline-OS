// "Found on this computer": projects from Unity Hub, Unreal, Godot and common
// folders that aren't added yet. Add them all at once, pick some, or dismiss.
import { useQueryClient } from "@tanstack/react-query";
import { Check, FolderPlus, Loader2, RefreshCw, Sparkles, X } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { desktopApi, isNativeRuntime, studioApi, type DiscoveredProject } from "../bridge";
import { DISCOVERY_KEY, useDiscovery } from "../lib/discovery";
import { formatLastOpened } from "../lib/format";

const ENGINE_LABEL: Record<DiscoveredProject["engine"], string> = { unity: "Unity", unreal: "Unreal", godot: "Godot", "pipeline-os": "Pipeline OS" };
const SOURCE_LABEL: Record<string, string> = { "unity-hub": "Unity Hub", "unreal-recent": "Unreal recents", "godot-projects": "Godot", folders: "Project folders" };

export function DiscoveredProjects({ onAdded, compact = false }: { onAdded: () => void; compact?: boolean }) {
  const queryClient = useQueryClient();
  const discovery = useDiscovery();
  const [unselected, setUnselected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const projects = discovery.projects;
  const selected = useMemo(() => projects.filter((project) => !unselected.has(project.path)), [projects, unselected]);

  function toggle(path: string) {
    setUnselected((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path); else next.add(path);
      return next;
    });
  }

  async function add(targets: DiscoveredProject[]) {
    if (!targets.length) return;
    setBusy(true);
    let added = 0;
    const failures: string[] = [];
    for (const project of targets) {
      try {
        if (isNativeRuntime()) await desktopApi.importProject(project.path, project.name);
        else await studioApi.dismissDiscovered(project.path);
        added++;
      } catch (error) {
        failures.push(`${project.name}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
    setBusy(false);
    setUnselected(new Set());
    await queryClient.invalidateQueries({ queryKey: DISCOVERY_KEY });
    onAdded();
    if (added) toast.success(`Added ${added} project${added === 1 ? "" : "s"}`);
    if (failures.length) toast.error(`Couldn't add ${failures.length} project${failures.length === 1 ? "" : "s"}`, { description: failures.slice(0, 3).join("\n") });
  }

  async function dismiss(project: DiscoveredProject) {
    await studioApi.dismissDiscovered(project.path).catch(() => undefined);
    await queryClient.invalidateQueries({ queryKey: DISCOVERY_KEY });
  }

  async function refresh() {
    try {
      await discovery.refresh();
    } catch (error) {
      toast.error("Couldn't look for projects", { description: error instanceof Error ? error.message : String(error) });
    }
  }

  const header = (
    <div className="flex flex-wrap items-center gap-2">
      <h2 className="mr-auto flex items-center gap-2 text-sm font-semibold">
        <Sparkles size={15} className="text-primary" /> Found on this computer
        {projects.length ? <Badge variant="secondary">{projects.length}</Badge> : null}
      </h2>
      <Button variant="ghost" size="sm" onClick={() => void refresh()} disabled={discovery.refreshing} title={discovery.scannedAt ? `Last checked ${formatLastOpened(discovery.scannedAt)}` : undefined}>
        <RefreshCw size={14} className={discovery.refreshing ? "animate-spin" : ""} /> {discovery.refreshing ? "Looking…" : "Look again"}
      </Button>
      {projects.length > 1 ? <Button size="sm" variant="outline" disabled={busy || !selected.length || selected.length === projects.length} onClick={() => void add(selected)}>Add selected ({selected.length})</Button> : null}
      {projects.length ? <Button size="sm" disabled={busy} onClick={() => void add(projects)}>{busy ? <Loader2 size={14} className="animate-spin" /> : <FolderPlus size={14} />} Add {projects.length === 1 ? "" : "all "}{projects.length === 1 ? "it" : projects.length}</Button> : null}
    </div>
  );

  if (!projects.length) {
    if (compact) return (
      <div className="space-y-1">{header}<p className="text-xs text-muted-foreground">{discovery.refreshing ? "Checking Unity Hub, Unreal, Godot and your project folders…" : "Nothing new found. Pipeline OS checks Unity Hub, Unreal's recent projects, Godot's project list and common project folders."}</p></div>
    );
    return null;
  }

  const list = (
    <ul className={cn("divide-y divide-border rounded-lg border border-border", compact ? "max-h-72 overflow-y-auto" : "")}>
      {projects.map((project) => {
        const checked = !unselected.has(project.path);
        return (
          <li key={project.path} className="flex items-center gap-3 px-3 py-2">
            <button role="checkbox" aria-checked={checked} aria-label={`Select ${project.name}`} onClick={() => toggle(project.path)} className={cn("flex size-4 shrink-0 items-center justify-center rounded border", checked ? "border-primary bg-primary text-primary-foreground" : "border-border")}>{checked ? <Check size={11} /> : null}</button>
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-2 text-sm font-medium">{project.name}<Badge variant="outline" className="font-normal">{ENGINE_LABEL[project.engine]}{project.engineVersion ? ` ${project.engineVersion}` : ""}</Badge></span>
              <span className="block truncate text-xs text-muted-foreground" title={project.path}>{project.path} · {SOURCE_LABEL[project.source] ?? project.source}</span>
            </span>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => void add([project])}>Add</Button>
            <Button variant="ghost" size="icon" className="h-7 w-7" aria-label={`Dismiss ${project.name}`} title="Don't suggest this again" onClick={() => void dismiss(project)}><X size={14} /></Button>
          </li>
        );
      })}
    </ul>
  );

  if (compact) return <div className="space-y-2">{header}{list}</div>;
  return <Card><CardContent className="space-y-3 p-4">{header}{list}</CardContent></Card>;
}

// "Found on this computer": projects from Unity Hub, Unreal, Godot and common
// folders that aren't added yet. Built to stay usable with hundreds of
// results: search, filter by engine, sensible pre-selection (recently used),
// one "Add N projects" action, and hide what you don't want suggested again.
import { useQueryClient } from "@tanstack/react-query";
import { Check, ChevronDown, EyeOff, FolderPlus, Loader2, Minus, RefreshCw, Search, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { DiscoveredProject } from "../bridge";
import { addDiscoveredProjects, hideDiscoveredProjects, isRecommendedDiscovery, useDiscovery, useDiscoverySelection, type DiscoverySelection } from "../lib/discovery";
import { timeAgo } from "../lib/format";

const ENGINE_LABEL: Record<DiscoveredProject["engine"], string> = {
  unity: "Unity", unreal: "Unreal", godot: "Godot", "pipeline-os": "Pipeline OS", gamemaker: "GameMaker", defold: "Defold",
  blender: "Blender", houdini: "Houdini", maya: "Maya", spine: "Spine", xcode: "Xcode", "android-studio": "Android Studio", "visual-studio": "Visual Studio",
};
const SOURCE_LABEL: Record<string, string> = { "unity-hub": "Unity Hub", "unreal-recent": "Unreal recents", "godot-projects": "Godot", folders: "Project folders" };


export function DiscoveredProjects({ onAdded, compact = false, selection: external, showActions = true }: {
  onAdded: () => void;
  compact?: boolean;
  /** Provide to control selection from outside (e.g. onboarding). */
  selection?: DiscoverySelection;
  /** The "Add N projects" footer; off when the parent has its own button. */
  showActions?: boolean;
}) {
  const queryClient = useQueryClient();
  const discovery = useDiscovery();
  const internal = useDiscoverySelection(discovery.projects);
  const selection = external ?? internal;
  const [query, setQuery] = useState("");
  const [engine, setEngine] = useState<DiscoveredProject["engine"] | null>(null);
  const [busy, setBusy] = useState(false);

  const projects = useMemo(() => [...discovery.projects].sort((a, b) => b.modified - a.modified), [discovery.projects]);
  const engines = useMemo(() => {
    const counts = new Map<DiscoveredProject["engine"], number>();
    for (const project of projects) counts.set(project.engine, (counts.get(project.engine) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [projects]);
  const shown = projects.filter((project) =>
    (!engine || project.engine === engine)
    && (!query || `${project.name} ${project.path}`.toLowerCase().includes(query.toLowerCase())));
  const chosen = projects.filter((project) => selection.selected.has(project.path));
  const shownSelected = shown.filter((project) => selection.selected.has(project.path)).length;
  const many = projects.length > 6;

  async function add(targets: DiscoveredProject[]) {
    setBusy(true);
    await addDiscoveredProjects(targets, queryClient);
    setBusy(false);
    onAdded();
  }

  async function hide(targets: DiscoveredProject[]) {
    await hideDiscoveredProjects(targets, queryClient);
    selection.set([...selection.selected].filter((path) => !targets.some((project) => project.path === path)));
  }

  function selectShown(on: boolean) {
    const next = new Set(selection.selected);
    for (const project of shown) { if (on) next.add(project.path); else next.delete(project.path); }
    selection.set(next);
  }

  const header = (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <h2 className="mr-auto flex min-w-0 items-center gap-2 text-sm font-semibold">
        <Sparkles size={15} className="shrink-0 text-primary" /> Found on this computer
        {projects.length ? <span className="rounded-full bg-secondary px-2 py-0.5 text-xs font-medium tabular-nums text-muted-foreground">{projects.length}</span> : null}
      </h2>
      <Button variant="ghost" size="sm" onClick={() => void discovery.refresh().catch(() => undefined)} disabled={discovery.refreshing} title={discovery.scannedAt ? `Last checked ${timeAgo(Date.parse(discovery.scannedAt) / 1000)}` : undefined}>
        <RefreshCw size={14} className={discovery.refreshing ? "animate-spin" : ""} /> {discovery.refreshing ? "Looking…" : "Look again"}
      </Button>
    </div>
  );

  if (!projects.length) {
    if (!compact) return null;
    return (
      <div className="min-w-0 space-y-1">
        {header}
        <p className="text-xs text-muted-foreground">{discovery.refreshing ? "Checking Unity Hub, Unreal, Godot and your project folders…" : "Nothing new found. Pipeline OS checks Unity Hub, Unreal's recent projects, Godot's project list and common project folders."}</p>
      </div>
    );
  }

  const allShownSelected = shown.length > 0 && shownSelected === shown.length;
  const toolbar = (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <button role="checkbox" aria-checked={allShownSelected ? true : shownSelected ? "mixed" : false} aria-label="Select all shown" onClick={() => selectShown(!allShownSelected)}
        className={cn("ml-3 flex size-4 shrink-0 items-center justify-center rounded border", shownSelected ? "border-primary bg-primary text-primary-foreground" : "border-border")}>
        {allShownSelected ? <Check size={11} /> : shownSelected ? <Minus size={11} /> : null}
      </button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild><Button variant="ghost" size="sm" className="h-7 gap-1 px-1.5 text-xs text-muted-foreground">Select <ChevronDown size={12} /></Button></DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem onClick={() => selectShown(true)}>All {engine || query ? "shown" : ""} ({shown.length})</DropdownMenuItem>
          <DropdownMenuItem onClick={() => selection.set(projects.filter(isRecommendedDiscovery).map((project) => project.path))}>Recently used ({projects.filter(isRecommendedDiscovery).length})</DropdownMenuItem>
          <DropdownMenuItem onClick={() => selection.set([])}>None</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {many ? (
        <div className="relative min-w-40 flex-1">
          <Search size={13} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" aria-hidden />
          <Input aria-label="Search found projects" placeholder="Search by name or folder" value={query} onChange={(event) => setQuery(event.target.value)} className="h-8 pl-8 text-sm" />
        </div>
      ) : <span className="flex-1" />}
      {engines.length > 1 ? (
        <div className="flex flex-wrap gap-1" role="group" aria-label="Filter by engine">
          <button onClick={() => setEngine(null)} aria-pressed={!engine} className={cn("rounded-full px-2.5 py-1 text-xs", !engine ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>All</button>
          {engines.map(([id, count]) => (
            <button key={id} onClick={() => setEngine(engine === id ? null : id)} aria-pressed={engine === id} className={cn("rounded-full px-2.5 py-1 text-xs", engine === id ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>{ENGINE_LABEL[id]} <span className="tabular-nums opacity-70">{count}</span></button>
          ))}
        </div>
      ) : null}
    </div>
  );

  const list = (
    <ul className={cn("min-w-0 divide-y divide-border overflow-y-auto rounded-lg border border-border", compact ? "max-h-[min(22rem,45vh)]" : "max-h-[28rem]")}>
      {shown.map((project) => {
        const checked = selection.selected.has(project.path);
        return (
          <li key={project.path} className={cn("group flex min-w-0 cursor-pointer items-center gap-3 px-3 py-2 transition-colors hover:bg-secondary/40", checked && "bg-primary/5")} onClick={() => selection.toggle(project.path)}>
            <span role="checkbox" aria-checked={checked} aria-label={`Select ${project.name}`} tabIndex={0} onKeyDown={(event) => { if (event.key === " " || event.key === "Enter") { event.preventDefault(); selection.toggle(project.path); } }}
              className={cn("flex size-4 shrink-0 items-center justify-center rounded border", checked ? "border-primary bg-primary text-primary-foreground" : "border-border")}>
              {checked ? <Check size={11} /> : null}
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex min-w-0 items-center gap-2 text-sm">
                <span className="truncate font-medium">{project.name}</span>
                <span className="shrink-0 rounded border border-border px-1.5 text-[11px] text-muted-foreground">{ENGINE_LABEL[project.engine]}{project.engineVersion ? ` ${project.engineVersion}` : ""}</span>
              </span>
              <span className="block truncate text-xs text-muted-foreground" title={project.path}>{project.path}</span>
            </span>
            <span className="hidden shrink-0 text-right text-xs text-muted-foreground sm:block">
              <span className="block">{project.modified ? timeAgo(project.modified) : ""}</span>
              <span className="block text-[10px] opacity-70">{SOURCE_LABEL[project.source] ?? project.source}</span>
            </span>
            <Button variant="ghost" size="icon" className="h-7 w-7 shrink-0 opacity-0 group-hover:opacity-100 focus-visible:opacity-100" aria-label={`Don't suggest ${project.name}`} title="Don't suggest this again" onClick={(event) => { event.stopPropagation(); void hide([project]); }}><EyeOff size={13} /></Button>
          </li>
        );
      })}
      {!shown.length ? <li className="px-3 py-6 text-center text-sm text-muted-foreground">No found projects match.</li> : null}
    </ul>
  );

  const footer = showActions ? (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <span className="mr-auto text-xs text-muted-foreground">{chosen.length ? `${chosen.length} of ${projects.length} selected` : "Select the projects to add"}</span>
      {chosen.length ? <Button variant="ghost" size="sm" onClick={() => void hide(chosen)}><EyeOff size={14} /> Don't suggest</Button> : null}
      <Button size="sm" disabled={busy || !chosen.length} onClick={() => void add(chosen)}>{busy ? <Loader2 size={14} className="animate-spin" /> : <FolderPlus size={14} />} Add {chosen.length || ""} project{chosen.length === 1 ? "" : "s"}</Button>
    </div>
  ) : null;

  const body = <div className="min-w-0 space-y-2">{header}{toolbar}{list}{footer}</div>;
  return compact ? body : <Card><CardContent className="p-4">{body}</CardContent></Card>;
}

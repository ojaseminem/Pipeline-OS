// "Add tasks": every preset for this kind of project, recommended ones first,
// grouped by engine and platform. Pick several and add them in one go, or
// start from one and edit it.
import { useQuery } from "@tanstack/react-query";
import { Bookmark, Check, ClipboardCopy, FileCode2, FileUp, Pencil, Plus, Search, Sparkles, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { studioApi, studioAvailable, type TaskDefinition, type TaskPreset } from "../bridge";
import { KIND_ICONS } from "../lib/task-meta";

const needsUnityScript = (preset: TaskPreset) => preset.definition.args.some((arg) => arg.startsWith("PipelineOS.Build"));

export function TaskPresetDialog({ open, onOpenChange, projectPath, existing, onAdd, onCustomize, onDeletePreset, onCopyPreset, onImport }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectPath: string;
  existing: TaskDefinition[];
  onAdd: (tasks: TaskDefinition[]) => Promise<boolean>;
  onCustomize: (task: TaskDefinition | null) => void;
  /** Project presets (.pipelineos/presets.toml) only. */
  onDeletePreset: (preset: TaskDefinition) => Promise<boolean>;
  onCopyPreset: (preset: TaskDefinition) => void;
  onImport: () => void;
}) {
  const presets = useQuery({ queryKey: ["task-presets", projectPath], queryFn: () => studioApi.presets(projectPath), enabled: open && studioAvailable(), staleTime: 60_000 });
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const existingIds = useMemo(() => new Set(existing.map((task) => task.id)), [existing]);

  const filtered = (presets.data ?? []).filter((preset) => !query || `${preset.definition.name} ${preset.group} ${preset.definition.description ?? ""}`.toLowerCase().includes(query.toLowerCase()));
  const own = filtered.filter((preset) => preset.project);
  const recommended = filtered.filter((preset) => preset.recommended && !preset.project);
  const groups = new Map<string, TaskPreset[]>();
  for (const preset of filtered.filter((item) => !item.recommended && !item.project)) groups.set(preset.group, [...(groups.get(preset.group) ?? []), preset]);
  const chosen = (presets.data ?? []).filter((preset) => picked.has(preset.definition.id));

  function toggle(id: string) {
    setPicked((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }

  async function addPicked() {
    setBusy(true);
    try {
      if (chosen.some(needsUnityScript)) {
        const path = await studioApi.installUnityBuildScript(projectPath).catch(() => null);
        if (path) toast.message("Added the Pipeline OS Unity build script", { description: path });
      }
      if (await onAdd(chosen.map((preset) => preset.definition))) { setPicked(new Set()); onOpenChange(false); }
    } finally {
      setBusy(false);
    }
  }

  const row = (preset: TaskPreset) => {
    const Icon = KIND_ICONS[preset.definition.kind];
    const added = existingIds.has(preset.definition.id);
    const checked = picked.has(preset.definition.id);
    return (
      <li key={preset.definition.id} className={cn("flex items-start gap-3 rounded-lg px-2.5 py-2", added ? "opacity-60" : "hover:bg-secondary/50")}>
        <button role="checkbox" aria-checked={checked} aria-label={`Select ${preset.definition.name}`} disabled={added} onClick={() => toggle(preset.definition.id)}
          className={cn("mt-0.5 flex size-4 shrink-0 items-center justify-center rounded border", checked ? "border-primary bg-primary text-primary-foreground" : "border-border")}>
          {checked ? <Check size={11} /> : null}
        </button>
        <Icon size={15} className="mt-0.5 shrink-0 text-muted-foreground" aria-hidden />
        <button className="min-w-0 flex-1 text-left" disabled={added} onClick={() => toggle(preset.definition.id)}>
          <span className="flex flex-wrap items-center gap-1.5 text-sm font-medium">{preset.definition.name}{added ? <Badge variant="secondary">Added</Badge> : null}{needsUnityScript(preset) ? <Badge variant="outline" className="gap-1 font-normal"><FileCode2 size={10} /> build script</Badge> : null}</span>
          {preset.definition.description ? <span className="block text-xs text-muted-foreground">{preset.definition.description}</span> : null}
        </button>
        <Button variant="ghost" size="sm" className="h-7 shrink-0" onClick={() => { onOpenChange(false); onCustomize(preset.definition); }} title="Start from this preset and edit it"><Pencil size={13} /> Customize</Button>
        {preset.project ? (
          <>
            <Button variant="ghost" size="icon" className="h-7 w-7 shrink-0" aria-label={`Copy ${preset.definition.name} as TOML`} title="Copy as TOML" onClick={() => onCopyPreset(preset.definition)}><ClipboardCopy size={13} /></Button>
            <Button variant="ghost" size="icon" className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive" aria-label={`Delete preset ${preset.definition.name}`} title="Delete preset" onClick={() => void onDeletePreset(preset.definition)}><Trash2 size={13} /></Button>
          </>
        ) : null}
      </li>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Add pipeline tasks</DialogTitle>
          <DialogDescription>Your project's own presets and the ones that fit this project are listed first. Pick as many as you need; every task is saved to <code>.pipelineos/tasks.toml</code> so the team shares them. Save any task as a preset from its <strong>⋯</strong> menu.</DialogDescription>
        </DialogHeader>
        <div className="relative">
          <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" aria-hidden />
          <Input aria-label="Search presets" autoFocus placeholder="Search: android, shipping, webgl, render…" value={query} onChange={(event) => setQuery(event.target.value)} className="pl-8" />
        </div>
        <div className="-mx-1 min-h-0 flex-1 space-y-4 overflow-y-auto px-1">
          {presets.isLoading ? <p className="text-sm text-muted-foreground">Loading presets…</p> : null}
          {own.length ? (
            <section className="space-y-1">
              <h3 className="flex items-center gap-1.5 px-1 text-xs font-semibold uppercase tracking-wide text-primary"><Bookmark size={12} /> This project's presets</h3>
              <ul>{own.map(row)}</ul>
            </section>
          ) : null}
          {recommended.length ? (
            <section className="space-y-1">
              <h3 className="flex items-center gap-1.5 px-1 text-xs font-semibold uppercase tracking-wide text-primary"><Sparkles size={12} /> Recommended for this project</h3>
              <ul>{recommended.map(row)}</ul>
            </section>
          ) : null}
          {[...groups.entries()].map(([group, items]) => (
            <section key={group} className="space-y-1">
              <h3 className="px-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{group}</h3>
              <ul>{items.map(row)}</ul>
            </section>
          ))}
          {presets.data && !filtered.length ? <p className="text-sm text-muted-foreground">No presets match “{query}”.</p> : null}
        </div>
        <DialogFooter className="items-center sm:justify-between">
          <span className="flex gap-1">
            <Button variant="ghost" onClick={() => { onOpenChange(false); onCustomize(null); }}><Plus size={14} /> Blank task</Button>
            <Button variant="ghost" onClick={onImport}><FileUp size={14} /> Import…</Button>
          </span>
          <Button disabled={!chosen.length || busy} onClick={() => void addPicked()}>{chosen.length ? `Add ${chosen.length} task${chosen.length === 1 ? "" : "s"}` : "Select tasks to add"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

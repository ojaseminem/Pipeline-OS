// Organize a project: pick a folder layout (built-in, your own, or the
// project's), preview exactly which loose files move where, untick anything
// to leave alone, then apply. Engine-managed folders are never touched and the
// last organize can be undone. Layouts are plain JSON files, so they can be
// edited here or in any text editor, copied, imported, and shared.
import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, CheckCircle2, Clipboard, Copy, FileDown, FileUp, FolderOpen, FolderTree, MoreHorizontal, Pencil, Plus, RotateCcw, ShieldCheck, Trash2, TriangleAlert } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { desktopApi, isNativeRuntime, saveTextFile, studioApi, type FolderLayout, type LayoutEntry, type LayoutScope, type LayoutSource, type PlannedMove } from "../bridge";
import { confirmAction } from "../lib/confirm";

const SOURCE_LABEL: Record<LayoutSource, string> = { project: "This project", user: "My layouts", builtin: "Built in" };
const SOURCE_HINT: Record<LayoutSource, string> = {
  project: "Saved in .pipelineos/layouts, shared with the team through Git.",
  user: "Saved on this computer, available in every project.",
  builtin: "Ships with Pipeline OS. Duplicate one to change it.",
};

const errorText = (error: unknown) => (error instanceof Error ? error.message : String(error));

function formatSize(bytes: number): string {
  if (bytes < 1024 ** 2) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** The layout itself, without where it was listed from. */
function layoutOf(entry: LayoutEntry): FolderLayout {
  return { id: entry.id, name: entry.name, description: entry.description, folders: entry.folders, rules: entry.rules, keep: entry.keep };
}

function ruleText(rule: FolderLayout["rules"][number]): string {
  const parts = [rule.match?.length ? rule.match.join(", ") : null, rule.kinds?.length ? `${rule.kinds.join(", ")} files` : null].filter(Boolean);
  return parts.join(" · ");
}

export function OrganizeDialog({ open, projectPath, onClose, onChanged }: { open: boolean; projectPath: string; onClose: () => void; onChanged: () => void }) {
  const queryClient = useQueryClient();
  const layoutsKey = ["layouts", projectPath];
  const layouts = useQuery({ queryKey: layoutsKey, queryFn: () => studioApi.layouts(projectPath), enabled: open });
  const last = useQuery({ queryKey: ["organize-last", projectPath], queryFn: () => studioApi.lastOrganize(projectPath), enabled: open });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tab, setTab] = useState<"preview" | "layout">("preview");
  const [editor, setEditor] = useState<{ text: string; scope: LayoutScope; title: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const entries = useMemo(() => layouts.data ?? [], [layouts.data]);
  const selected = entries.find((entry) => entry.id === selectedId) ?? entries[0];
  const plan = useQuery({
    queryKey: ["organize-plan", projectPath, selected?.id, selected?.source],
    queryFn: () => studioApi.planOrganize(projectPath, selected!.id),
    enabled: open && Boolean(selected),
  });
  const [unticked, setUnticked] = useState<Set<string>>(new Set());
  useEffect(() => setUnticked(new Set()), [plan.data]);

  const moves = useMemo(() => plan.data?.moves ?? [], [plan.data]);
  const chosen = moves.filter((move) => !unticked.has(move.from));
  const groups = useMemo(() => {
    const byFolder = new Map<string, PlannedMove[]>();
    for (const move of moves) {
      const folder = move.to.slice(0, move.to.lastIndexOf("/"));
      byFolder.set(folder, [...(byFolder.get(folder) ?? []), move]);
    }
    return [...byFolder.entries()];
  }, [moves]);

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: layoutsKey }),
      queryClient.invalidateQueries({ queryKey: ["organize-plan", projectPath] }),
      queryClient.invalidateQueries({ queryKey: ["organize-last", projectPath] }),
    ]);
    onChanged();
  };

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try { await action(); } catch (error) { toast.error(errorText(error)); } finally { setBusy(false); }
  }

  const toggle = (from: string) => setUnticked((current) => {
    const next = new Set(current);
    if (next.has(from)) next.delete(from); else next.add(from);
    return next;
  });
  const toggleGroup = (items: PlannedMove[], on: boolean) => setUnticked((current) => {
    const next = new Set(current);
    for (const item of items) if (on) next.delete(item.from); else next.add(item.from);
    return next;
  });

  const apply = () => run(async () => {
    if (!selected || !chosen.length) return;
    const ok = await confirmAction({
      title: `Move ${chosen.length} file${chosen.length === 1 ? "" : "s"} into "${selected.name}"?`,
      description: "Missing folders are created. Files are moved, never overwritten or deleted, and you can undo this from here afterwards. If the project uses Git, commit afterwards so the team gets the new structure.",
      confirmLabel: `Move ${chosen.length} file${chosen.length === 1 ? "" : "s"}`,
    });
    if (!ok) return;
    const result = await studioApi.applyOrganize(projectPath, selected.id, chosen.map(({ from, to }) => ({ from, to })));
    if (result.skipped.length) toast.warning(`Moved ${result.moved.length}, skipped ${result.skipped.length}`, { description: result.skipped.slice(0, 3).map(([file, why]) => `${file}: ${why}`).join("\n") });
    else toast.success(`Moved ${result.moved.length} file${result.moved.length === 1 ? "" : "s"}`, { description: result.createdFolders.length ? `Created ${result.createdFolders.length} folders.` : undefined });
    await refresh();
  });

  const createFolders = () => run(async () => {
    if (!selected) return;
    const result = await studioApi.createLayoutFolders(projectPath, selected.id);
    toast.success(result.created.length ? `Created ${result.created.length} folder${result.created.length === 1 ? "" : "s"}.` : "All folders already exist.");
    await refresh();
  });

  const undo = () => run(async () => {
    const journal = last.data;
    if (!journal) return;
    const ok = await confirmAction({ title: "Undo the last organize?", description: `Moves ${journal.moves.length} file${journal.moves.length === 1 ? "" : "s"} back where they were and removes folders it created if they're empty.`, confirmLabel: "Undo" });
    if (!ok) return;
    const result = await studioApi.undoOrganize(projectPath);
    toast.success(`Moved ${result.moved.length} file${result.moved.length === 1 ? "" : "s"} back`, { description: result.skipped.length ? `${result.skipped.length} couldn't be restored because they changed since.` : undefined });
    await refresh();
  });

  async function openEditor(layout: FolderLayout, scope: LayoutScope, title: string) {
    try { setEditor({ text: await studioApi.layoutToJson(layout), scope, title }); }
    catch (error) { toast.error(errorText(error)); }
  }
  const duplicate = (entry: LayoutEntry, scope: LayoutScope) => {
    const ids = new Set(entries.map((item) => item.id));
    let id = entry.source === "builtin" ? entry.id : `${entry.id}-copy`;
    // A project or personal copy of a built-in keeps its id and replaces it.
    if (entry.source !== "builtin") for (let n = 2; ids.has(id); n += 1) id = `${entry.id}-copy-${n}`;
    void openEditor({ ...layoutOf(entry), id, name: entry.source === "builtin" ? entry.name : `${entry.name} (copy)` }, scope, entry.source === "builtin" ? `Customize "${entry.name}"` : `Duplicate "${entry.name}"`);
  };
  const newLayout = () => void openEditor({
    id: "my-layout",
    name: "My layout",
    description: "",
    folders: ["Art/Concept", "Art/Reference", "Audio", "Docs"],
    rules: [
      { to: "Art/Reference", match: ["*ref*", "*moodboard*"], kinds: ["image"] },
      { to: "Art/Concept", kinds: ["image", "source"] },
      { to: "Audio", kinds: ["audio"] },
      { to: "Docs", kinds: ["document"] },
    ],
    keep: ["README*", "LICENSE*"],
  }, "project", "New layout");

  async function importText(text: string) {
    try {
      const layout = await studioApi.parseLayout(text);
      await openEditor(layout, "project", "Import layout");
    } catch (error) { toast.error(errorText(error)); }
  }
  async function importFromClipboard() {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) { toast.error("The clipboard is empty."); return; }
      await importText(text);
    } catch { toast.error("Couldn't read the clipboard. Open a file instead."); }
  }

  const remove = (entry: LayoutEntry) => run(async () => {
    if (entry.source === "builtin") return;
    const replaced = entry.overrides ? " The built-in version comes back." : "";
    if (!(await confirmAction({ title: `Delete "${entry.name}"?`, description: `Removes ${entry.file ?? "the layout file"}. No project files are touched.${replaced}`, confirmLabel: "Delete", destructive: true }))) return;
    await studioApi.deleteLayout(projectPath, entry.source, entry.id);
    toast.success("Layout deleted.");
    await refresh();
  });

  async function copyJson(entry: LayoutEntry) {
    try {
      await navigator.clipboard.writeText(await studioApi.layoutToJson(layoutOf(entry)));
      toast.success("Layout copied as JSON. Paste it into Import in any project.");
    } catch (error) { toast.error(errorText(error)); }
  }
  async function exportFile(entry: LayoutEntry) {
    try {
      await saveTextFile({ defaultName: `${entry.id}.json`, contents: await studioApi.layoutToJson(layoutOf(entry)), filterName: "Pipeline OS layout", extensions: ["json"] });
    } catch (error) { toast.error(errorText(error)); }
  }
  async function openFolder(scope: LayoutScope) {
    try { await desktopApi.openPath(await studioApi.layoutsFolder(projectPath, scope)); }
    catch (error) { toast.error(errorText(error)); }
  }

  const grouped = (["project", "user", "builtin"] as const).map((source) => [source, entries.filter((entry) => entry.source === source)] as const);
  const canEdit = selected && selected.source !== "builtin";

  return (
    <>
      <Dialog open={open} onOpenChange={(value) => { if (!value) onClose(); }}>
        {/* A work surface: closes with ✕ or Escape, not a stray click (the confirm prompt counts as outside). */}
        <DialogContent onInteractOutside={(event) => event.preventDefault()} className="flex h-[calc(100vh-4rem)] w-[calc(100vw-4rem)] max-w-6xl flex-col gap-0 p-0 sm:max-w-6xl">
          <DialogHeader className="border-b border-border px-6 pb-4 pt-5 pr-14">
            <DialogTitle className="flex items-center gap-2"><FolderTree size={18} className="text-primary" aria-hidden /> Organize project</DialogTitle>
            <DialogDescription>Pick a folder layout and see exactly which loose files would move. Untick anything to leave it where it is. Engine folders are never touched, and you can undo.</DialogDescription>
          </DialogHeader>

          <div className="grid min-h-0 flex-1 grid-cols-[260px_1fr]">
            {/* Layout library */}
            <aside className="flex min-h-0 flex-col border-r border-border bg-muted/10">
              <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-3">
                {grouped.map(([source, items]) => (
                  <section key={source} aria-label={SOURCE_LABEL[source]}>
                    <h3 className="mb-1.5 flex items-center justify-between px-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                      {SOURCE_LABEL[source]}
                      {source !== "builtin" && isNativeRuntime() ? <button className="rounded p-0.5 hover:text-foreground" title="Open this folder" aria-label={`Open the ${SOURCE_LABEL[source]} folder`} onClick={() => void openFolder(source)}><FolderOpen size={12} /></button> : null}
                    </h3>
                    {items.length ? (
                      <div className="space-y-1" role="radiogroup" aria-label={SOURCE_LABEL[source]}>
                        {items.map((entry) => (
                          <button key={`${entry.source}:${entry.id}`} role="radio" aria-checked={selected?.id === entry.id} onClick={() => { setSelectedId(entry.id); setTab("preview"); }}
                            className={cn("w-full rounded-md border px-2.5 py-2 text-left transition-colors", selected?.id === entry.id ? "border-primary/60 bg-primary/10" : "border-transparent hover:bg-muted/50")}>
                            <span className="block truncate text-sm font-medium">{entry.name}</span>
                            <span className="block truncate text-xs text-muted-foreground">{entry.folders.length} folders · {entry.rules.length} rules{entry.overrides ? " · replaces built-in" : ""}</span>
                          </button>
                        ))}
                      </div>
                    ) : <p className="px-1 text-xs text-muted-foreground">{source === "project" ? "None yet. Customize a layout to share it with the team." : "None yet."}</p>}
                  </section>
                ))}
              </div>
              <div className="flex gap-2 border-t border-border p-3">
                <Button variant="outline" size="sm" className="flex-1" onClick={newLayout}><Plus size={14} /> New</Button>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild><Button variant="outline" size="sm" className="flex-1"><FileUp size={14} /> Import</Button></DropdownMenuTrigger>
                  <DropdownMenuContent align="end" className="w-60">
                    <DropdownMenuLabel>JSON layout or a folder list (.txt)</DropdownMenuLabel>
                    <DropdownMenuItem onClick={() => fileInput.current?.click()}><FileUp size={14} /> Open a file…</DropdownMenuItem>
                    <DropdownMenuItem onClick={() => void importFromClipboard()}><Clipboard size={14} /> Paste from clipboard</DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
                <input ref={fileInput} type="file" accept=".json,.txt,text/plain,application/json" className="hidden" onChange={(event) => { const file = event.target.files?.[0]; event.target.value = ""; if (file) void file.text().then(importText); }} />
              </div>
            </aside>

            {/* Selected layout */}
            <section className="flex min-h-0 flex-col">
              {selected ? (
                <>
                  <div className="flex items-start gap-3 border-b border-border px-6 py-4">
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <h2 className="text-lg font-semibold">{selected.name}</h2>
                        <span className="rounded border border-border px-1.5 py-0.5 text-[11px] text-muted-foreground" title={SOURCE_HINT[selected.source]}>{SOURCE_LABEL[selected.source]}</span>
                      </div>
                      {selected.description ? <p className="mt-0.5 text-sm text-muted-foreground">{selected.description}</p> : null}
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      {canEdit
                        ? <Button variant="outline" size="sm" onClick={() => void openEditor(layoutOf(selected), selected.source as LayoutScope, `Edit "${selected.name}"`)}><Pencil size={14} /> Edit</Button>
                        : <Button variant="outline" size="sm" onClick={() => duplicate(selected, "project")}><Pencil size={14} /> Customize</Button>}
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" aria-label="More layout actions"><MoreHorizontal size={16} /></Button></DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-60">
                          <DropdownMenuItem onClick={() => duplicate(selected, "project")}><Copy size={14} /> Duplicate to this project</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => duplicate(selected, "user")}><Copy size={14} /> Duplicate to my layouts</DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem onClick={() => void copyJson(selected)}><Clipboard size={14} /> Copy as JSON</DropdownMenuItem>
                          <DropdownMenuItem onClick={() => void exportFile(selected)}><FileDown size={14} /> Export .json file…</DropdownMenuItem>
                          {selected.file && isNativeRuntime() ? <DropdownMenuItem onClick={() => void openFolder(selected.source as LayoutScope)}><FolderOpen size={14} /> Show file</DropdownMenuItem> : null}
                          {canEdit ? <><DropdownMenuSeparator /><DropdownMenuItem className="text-destructive" onClick={() => void remove(selected)}><Trash2 size={14} /> Delete layout</DropdownMenuItem></> : null}
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </div>
                  </div>

                  <div className="flex gap-1 border-b border-border px-6" role="tablist">
                    {([["preview", `Preview changes${plan.data ? ` (${moves.length})` : ""}`], ["layout", "Folders & rules"]] as const).map(([id, label]) => (
                      <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)} className={cn("-mb-px border-b-2 px-3 py-2 text-sm transition-colors", tab === id ? "border-primary font-medium text-foreground" : "border-transparent text-muted-foreground hover:text-foreground")}>{label}</button>
                    ))}
                  </div>

                  <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
                    {tab === "layout" ? <LayoutDetails layout={selected} /> : (
                      plan.isLoading ? <p className="text-sm text-muted-foreground">Looking through the project…</p>
                      : plan.isError ? <p className="text-sm text-destructive">{errorText(plan.error)}</p>
                      : plan.data ? (
                        <div className="space-y-4">
                          <div className="flex flex-wrap gap-2 text-xs">
                            <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-2.5 py-1.5"><ArrowRight size={13} className="text-primary" /> {moves.length ? `${moves.length} loose file${moves.length === 1 ? "" : "s"} to sort` : "Nothing to move"}</span>
                            {plan.data.conflicts.length ? <span className="inline-flex items-center gap-1.5 rounded-md border border-warning/40 bg-warning/10 px-2.5 py-1.5"><TriangleAlert size={13} className="text-warning" /> {plan.data.conflicts.length} skipped: a file with that name is already there</span> : null}
                            {plan.data.protectedAreas.length ? <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-2.5 py-1.5" title={plan.data.protectedAreas.join(", ")}><ShieldCheck size={13} className="text-success" /> Engine folders left alone: {plan.data.protectedAreas.slice(0, 4).join(", ")}{plan.data.protected ? ` (${plan.data.protected} matching file${plan.data.protected === 1 ? "" : "s"})` : ""}</span> : null}
                          </div>
                          {moves.length ? groups.map(([folder, items]) => {
                            const allOn = items.every((item) => !unticked.has(item.from));
                            return (
                              <div key={folder} className="overflow-hidden rounded-lg border border-border">
                                <label className="flex cursor-pointer items-center gap-2.5 border-b border-border bg-muted/30 px-3 py-2 text-sm">
                                  <input type="checkbox" className="h-4 w-4 accent-[var(--primary)]" checked={allOn} onChange={(event) => toggleGroup(items, event.target.checked)} />
                                  <FolderTree size={14} className="text-primary" aria-hidden />
                                  <span className="font-medium">{folder}</span>
                                  <span className="text-xs text-muted-foreground">{items.length} file{items.length === 1 ? "" : "s"}</span>
                                </label>
                                <ul className="divide-y divide-border">
                                  {items.map((move) => (
                                    <li key={move.from}>
                                      <label className={cn("flex cursor-pointer items-center gap-2.5 px-3 py-1.5 text-sm hover:bg-muted/30", unticked.has(move.from) && "opacity-50")}>
                                        <input type="checkbox" className="h-4 w-4 shrink-0 accent-[var(--primary)]" checked={!unticked.has(move.from)} onChange={() => toggle(move.from)} />
                                        <span className="min-w-0 flex-1 truncate font-mono text-xs" title={`${move.from} → ${move.to}`}>{move.from}</span>
                                        <span className="hidden max-w-[40%] truncate text-xs text-muted-foreground md:block" title={`Matched ${move.reason}`}>{move.reason}</span>
                                        <span className="w-16 shrink-0 text-right text-xs tabular-nums text-muted-foreground">{formatSize(move.size)}</span>
                                      </label>
                                    </li>
                                  ))}
                                </ul>
                              </div>
                            );
                          }) : (
                            <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border py-10 text-center">
                              <CheckCircle2 size={22} className="text-success" />
                              <p className="text-sm font-medium">This project already fits "{selected.name}"</p>
                              <p className="max-w-md text-xs text-muted-foreground">No loose files match its rules. You can still create its folders so new files have a home.</p>
                            </div>
                          )}
                          {plan.data.conflicts.length ? (
                            <details className="rounded-lg border border-border px-3 py-2 text-sm">
                              <summary className="cursor-pointer text-muted-foreground">Skipped because the destination already has that file name ({plan.data.conflicts.length})</summary>
                              <ul className="mt-2 space-y-1 font-mono text-xs text-muted-foreground">{plan.data.conflicts.map((item) => <li key={item.from}>{item.from} → {item.to}</li>)}</ul>
                            </details>
                          ) : null}
                        </div>
                      ) : null
                    )}
                  </div>
                </>
              ) : <p className="p-6 text-sm text-muted-foreground">{layouts.isLoading ? "Loading layouts…" : layouts.isError ? errorText(layouts.error) : "No layouts."}</p>}
            </section>
          </div>

          <DialogFooter className="items-center gap-2 border-t border-border px-6 py-3 sm:justify-between">
            <div className="min-w-0 text-xs text-muted-foreground">
              {last.data ? (
                <Button variant="ghost" size="sm" disabled={busy} onClick={() => void undo()}><RotateCcw size={14} /> Undo last organize ({last.data.moves.length} file{last.data.moves.length === 1 ? "" : "s"}, {new Date(last.data.at).toLocaleString()})</Button>
              ) : "Moves are recorded on this computer so you can undo them."}
            </div>
            <div className="flex gap-2">
              <Button variant="outline" disabled={!selected || busy} onClick={() => void createFolders()}>Create folders only</Button>
              <Button disabled={!selected || busy || !chosen.length} onClick={() => void apply()}>{chosen.length ? `Move ${chosen.length} file${chosen.length === 1 ? "" : "s"}` : "Nothing to move"}</Button>
            </div>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      {editor ? (
        <LayoutEditor
          initial={editor}
          projectPath={projectPath}
          onClose={() => setEditor(null)}
          onSaved={async (id) => { setEditor(null); setSelectedId(id); setTab("layout"); await refresh(); }}
        />
      ) : null}
    </>
  );
}

/** Every folder and its parents, in tree order ("Art", "Art/Concept", ...). */
function folderTree(folders: string[]): { path: string; name: string; depth: number }[] {
  const all = new Set<string>();
  for (const folder of folders) {
    const parts = folder.split("/");
    for (let i = 1; i <= parts.length; i += 1) all.add(parts.slice(0, i).join("/"));
  }
  return [...all]
    .sort((a, b) => a.split("/").join("\u0000").localeCompare(b.split("/").join("\u0000"), undefined, { numeric: true }))
    .map((path) => ({ path, name: path.split("/").pop() ?? path, depth: path.split("/").length - 1 }));
}

function LayoutDetails({ layout }: { layout: FolderLayout }) {
  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <div>
        <h3 className="mb-2 text-sm font-semibold">Folders</h3>
        <div className="rounded-lg border border-border bg-muted/10 p-3 font-mono text-xs">
          {folderTree(layout.folders).map(({ path, name, depth }) => (
            <div key={path} style={{ paddingLeft: depth * 16 }} className="flex items-center gap-1.5 py-0.5"><FolderTree size={12} className="text-primary" aria-hidden />{name}</div>
          ))}
        </div>
      </div>
      <div>
        <h3 className="mb-2 text-sm font-semibold">Where files go</h3>
        <p className="mb-2 text-xs text-muted-foreground">The first rule a file matches wins. Files already inside these folders stay put.</p>
        <ol className="space-y-1.5">
          {layout.rules.map((rule, index) => (
            <li key={index} className="flex items-baseline gap-2 rounded-md border border-border px-2.5 py-1.5 text-xs">
              <span className="w-4 shrink-0 text-right tabular-nums text-muted-foreground">{index + 1}</span>
              <span className="min-w-0 flex-1"><span className="font-mono">{ruleText(rule)}</span> <ArrowRight size={11} className="mx-1 inline text-muted-foreground" aria-hidden /> <span className="font-mono text-primary">{rule.to}</span></span>
            </li>
          ))}
          {!layout.rules.length ? <li className="text-xs text-muted-foreground">No rules: this layout only creates folders.</li> : null}
        </ol>
        {layout.keep?.length ? <p className="mt-3 text-xs text-muted-foreground">Never moved: <span className="font-mono">{layout.keep.join(", ")}</span></p> : null}
      </div>
    </div>
  );
}

const FORMAT_HELP = `Plain JSON, one file per layout:
  "folders": folders to create
  "rules": [{ "to": folder, "match": ["*.psd", "*concept*"], "kinds": ["image"] }]
     kinds: image, source, model, scene, cache, animation, video, audio, document
  "keep": file globs that never move
Or paste a folder list, one per line:  Art/Source <- *.psd source`;

function LayoutEditor({ initial, projectPath, onClose, onSaved }: {
  initial: { text: string; scope: LayoutScope; title: string };
  projectPath: string;
  onClose: () => void;
  onSaved: (id: string) => Promise<void>;
}) {
  const [text, setText] = useState(initial.text);
  const [scope, setScope] = useState<LayoutScope>(initial.scope);
  const [parsed, setParsed] = useState<FolderLayout | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // Validate as the user types (debounced) with the same parser that saves.
  useEffect(() => {
    const handle = window.setTimeout(() => {
      studioApi.parseLayout(text)
        .then((layout) => { setParsed(layout); setError(null); })
        .catch((reason) => { setParsed(null); setError(errorText(reason)); });
    }, 250);
    return () => window.clearTimeout(handle);
  }, [text]);

  async function save() {
    if (!parsed) return;
    setSaving(true);
    try {
      const file = await studioApi.saveLayout(projectPath, scope, parsed);
      toast.success(`Saved "${parsed.name}"`, { description: file });
      await onSaved(parsed.id);
    } catch (reason) { toast.error(errorText(reason)); } finally { setSaving(false); }
  }

  return (
    <Dialog open onOpenChange={(value) => { if (!value) onClose(); }}>
      <DialogContent className="flex max-h-[90vh] flex-col sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{initial.title}</DialogTitle>
          <DialogDescription>Layouts are plain JSON files. Edit here, or open the file in any text editor later.</DialogDescription>
        </DialogHeader>
        <Textarea aria-label="Layout JSON" spellCheck={false} value={text} onChange={(event) => setText(event.target.value)} className="min-h-[340px] flex-1 font-mono text-xs leading-relaxed" />
        <div className="flex flex-wrap items-start justify-between gap-3">
          <pre className="whitespace-pre-wrap text-[11px] leading-relaxed text-muted-foreground">{FORMAT_HELP}</pre>
          <div className="min-w-56 text-sm">
            {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>
              : parsed ? <p className="flex items-center gap-1.5 text-xs text-success"><CheckCircle2 size={14} /> Valid: {parsed.folders.length} folders, {parsed.rules.length} rules</p> : null}
          </div>
        </div>
        <DialogFooter className="gap-2 sm:justify-between">
          <div className="flex items-center gap-1 rounded-md border border-border p-0.5 text-xs" role="radiogroup" aria-label="Save to">
            {(["project", "user"] as const).map((value) => (
              <button key={value} role="radio" aria-checked={scope === value} onClick={() => setScope(value)} title={SOURCE_HINT[value]} className={cn("rounded px-2.5 py-1 transition-colors", scope === value ? "bg-secondary font-medium text-foreground" : "text-muted-foreground hover:text-foreground")}>
                {value === "project" ? "Save to this project (shared)" : "Save to my layouts"}
              </button>
            ))}
          </div>
          <div className="flex gap-2">
            <Button variant="ghost" onClick={onClose}>Cancel</Button>
            <Button disabled={!parsed || saving} onClick={() => void save()}>Save layout</Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

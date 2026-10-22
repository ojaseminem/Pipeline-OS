// Conflict helpers: open a conflicted file in a 3-way merge tool (Rider, VS
// Code, Visual Studio, UnityYAMLMerge for scenes and prefabs, …), mark it
// resolved, and auto-merge Pipeline OS's own TOML files key by key, asking
// only about values both sides changed differently.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, GitMerge, Sparkles, Wrench } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { scmApi, type TomlMergeReport } from "../../bridge";
import { attempt } from "../../lib/scm";

/** Menu items for a conflicted file (rendered inside an existing dropdown). */
export function MergeToolItems({ root, path, onResolved }: { root: string; path: string; onResolved: () => void }) {
  const tools = useQuery({ queryKey: ["merge-tools", path.split(".").pop()], queryFn: () => scmApi.mergeTools(path), staleTime: 5 * 60_000, retry: false });
  const mergeable = (tools.data?.tools ?? []).filter((tool) => tool.supportsMerge);
  return (
    <>
      <DropdownMenuSeparator />
      <DropdownMenuLabel className="text-xs">3-way merge</DropdownMenuLabel>
      {mergeable.map((tool) => (
        <DropdownMenuItem key={tool.id} onClick={() => void attempt(`Opening ${tool.name}`, () => scmApi.launchMergeTool(root, path, tool.id), `Merging in ${tool.name}. Save there, then mark it resolved.`)}>
          <Wrench size={13} /> {tool.name}{tools.data?.recommended === tool.id ? <span className="ml-auto text-[10px] text-primary">recommended</span> : null}
        </DropdownMenuItem>
      ))}
      {tools.data && !mergeable.length ? <DropdownMenuItem disabled>No merge tools found</DropdownMenuItem> : null}
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={() => void attempt("Marking resolved", () => scmApi.markResolved(root, path), `Resolved ${path}`).then((done) => { if (done) onResolved(); })}><Check size={13} /> Mark resolved</DropdownMenuItem>
    </>
  );
}

/** Auto-merges conflicted `.pipelineos` TOML files; asks about the rest. */
export function PipelineFilesMerge({ root, conflicted, onDone }: { root: string; conflicted: string[]; onDone: () => void }) {
  const queryClient = useQueryClient();
  const [reports, setReports] = useState<TomlMergeReport[] | null>(null);
  const [choices, setChoices] = useState<Record<string, Record<string, "ours" | "theirs" | "base">>>({});
  const [busy, setBusy] = useState(false);
  const pipelineFiles = conflicted.filter((path) => /(^|\/)\.pipelineos\/.*\.toml$/.test(path.replace(/\\/g, "/")));
  if (!pipelineFiles.length) return null;

  async function run() {
    setBusy(true);
    const result = await attempt("Auto-merging Pipeline OS files", () => scmApi.tomlAutoMerge(root));
    setBusy(false);
    if (!result) return;
    const resolved = result.filter((report) => report.autoResolved).length;
    if (resolved) toast.success(`Merged ${resolved} Pipeline OS file${resolved === 1 ? "" : "s"} automatically`);
    const remaining = result.filter((report) => !report.autoResolved && report.conflicts.length);
    if (remaining.length) {
      setChoices(Object.fromEntries(remaining.map((report) => [report.path, Object.fromEntries(report.conflicts.map((conflict) => [conflict.keyPath, "ours" as const]))])));
      setReports(remaining);
    }
    for (const report of result.filter((item) => item.error)) toast.error(`Couldn't merge ${report.path}`, { description: report.error ?? undefined });
    void queryClient.invalidateQueries({ queryKey: ["git-merge-status", root] });
    void queryClient.invalidateQueries({ queryKey: ["git-status", root] });
    onDone();
  }

  async function apply() {
    if (!reports) return;
    setBusy(true);
    for (const report of reports) await attempt(`Resolving ${report.path}`, () => scmApi.tomlResolve(root, report.path, choices[report.path] ?? {}));
    setBusy(false);
    setReports(null);
    toast.success("Pipeline OS files resolved");
    void queryClient.invalidateQueries({ queryKey: ["git-merge-status", root] });
    void queryClient.invalidateQueries({ queryKey: ["git-status", root] });
    onDone();
  }

  const show = (value: string | null) => value == null ? <span className="italic text-muted-foreground">removed</span> : <code className="break-all">{value}</code>;

  return (
    <>
      <Button size="sm" variant="outline" className="h-7" disabled={busy} onClick={() => void run()} title="Merges tasks, assets, issues and project settings key by key"><Sparkles size={13} /> Auto-merge {pipelineFiles.length} Pipeline OS file{pipelineFiles.length === 1 ? "" : "s"}</Button>
      <Dialog open={Boolean(reports)} onOpenChange={(open) => { if (!open) setReports(null); }}>
        <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2"><GitMerge size={18} /> Choose which edits to keep</DialogTitle>
            <DialogDescription>Everything else was merged automatically. These values were changed differently on both sides.</DialogDescription>
          </DialogHeader>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto">
            {(reports ?? []).map((report) => (
              <section key={report.path} className="space-y-2">
                <h3 className="font-mono text-xs text-muted-foreground">{report.path}</h3>
                {report.conflicts.map((conflict) => {
                  const pick = choices[report.path]?.[conflict.keyPath] ?? "ours";
                  const option = (side: "ours" | "theirs", label: string, value: string | null) => (
                    <button role="radio" aria-checked={pick === side} onClick={() => setChoices((current) => ({ ...current, [report.path]: { ...current[report.path], [conflict.keyPath]: side } }))}
                      className={cn("flex-1 rounded-md border p-2 text-left text-xs", pick === side ? "border-primary bg-primary/10" : "border-border hover:border-primary/40")}>
                      <span className="mb-1 block font-medium">{label}</span>{show(value)}
                    </button>
                  );
                  return (
                    <div key={conflict.keyPath} className="space-y-1 rounded-lg border border-border p-2">
                      <div className="flex items-center gap-2 text-xs"><code className="font-semibold">{conflict.keyPath}</code>{conflict.base != null ? <span className="text-muted-foreground">was {conflict.base}</span> : null}</div>
                      <div role="radiogroup" aria-label={conflict.keyPath} className="flex gap-2">{option("ours", "Keep mine", conflict.ours)}{option("theirs", "Take theirs", conflict.theirs)}</div>
                    </div>
                  );
                })}
              </section>
            ))}
          </div>
          <DialogFooter><Button variant="outline" onClick={() => setReports(null)}>Decide later</Button><Button disabled={busy} onClick={() => void apply()}><Check size={14} /> Apply choices</Button></DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

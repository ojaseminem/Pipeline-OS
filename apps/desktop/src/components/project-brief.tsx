// The project brief on the Overview tab: a compact preview card that opens a
// large reader with the full brief, its Markdown source, copy, and save.
import { useQuery } from "@tanstack/react-query";
import { BookOpenText, Clipboard, Code2, Eye, FileDown, Maximize2, RefreshCw } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { studioApi, studioAvailable } from "../bridge";
import { Markdown } from "./markdown";

export function ProjectBriefCard({ projectPath, projectName }: { projectPath: string; projectName: string }) {
  const available = studioAvailable();
  const brief = useQuery({ queryKey: ["brief", projectPath], queryFn: () => studioApi.brief(projectPath), enabled: available });
  const [open, setOpen] = useState(false);
  const [raw, setRaw] = useState(false);
  if (!available) return null;

  const markdown = brief.data?.markdown ?? "";
  // The card shows the body under the title; the reader shows everything.
  const preview = markdown.replace(/^#\s.*\n+/, "");

  async function save() {
    try {
      const result = await studioApi.brief(projectPath, true);
      toast.success("Brief saved", { description: result.writtenTo ?? undefined });
    } catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }
  function copy() {
    void navigator.clipboard?.writeText(markdown);
    toast.success("Brief copied. Paste it into any AI chat.");
  }

  return (
    <>
      <Card className="group relative overflow-hidden transition-colors hover:border-primary/40">
        <button className="absolute inset-0 z-10 rounded-[inherit] focus-visible:outline-2 focus-visible:outline-ring" aria-label="Open the project brief" onClick={() => setOpen(true)} />
        <CardContent className="flex h-full flex-col gap-3 p-5">
          <div className="flex items-center gap-2">
            <BookOpenText size={18} className="shrink-0 text-primary" aria-hidden />
            <div className="min-w-0 flex-1">
              <h2 className="text-base font-semibold">Project brief</h2>
              <p className="text-xs text-muted-foreground">What AI agents and new teammates read first</p>
            </div>
            <Maximize2 size={15} className="shrink-0 text-muted-foreground transition-colors group-hover:text-primary" aria-hidden />
          </div>
          <div className="relative max-h-44 min-h-28 flex-1 overflow-hidden rounded-md border border-border bg-muted/10 px-4 py-3 text-sm">
            {brief.isLoading ? <p className="text-muted-foreground">Building brief…</p>
              : brief.isError ? <p className="text-destructive">{String(brief.error)}</p>
              : <Markdown source={preview} />}
            <div className="pointer-events-none absolute inset-x-0 bottom-0 h-16 bg-gradient-to-t from-card to-transparent" aria-hidden />
          </div>
          <span className="text-xs font-medium text-primary">Read the full brief</span>
        </CardContent>
      </Card>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="flex h-[calc(100vh-4rem)] w-[calc(100vw-4rem)] max-w-5xl flex-col gap-3 p-0 sm:max-w-5xl">
          <DialogHeader className="border-b border-border px-6 pb-3 pt-5 pr-14">
            <div className="flex flex-wrap items-center gap-2">
              <div className="min-w-0 flex-1">
                <DialogTitle>Project brief</DialogTitle>
                <DialogDescription>Generated from {projectName}'s files. Save it to <code>.pipelineos/context.md</code> so AI agents read it automatically.</DialogDescription>
              </div>
              <Button variant="ghost" size="icon" aria-pressed={raw} aria-label={raw ? "Show preview" : "Show Markdown source"} title={raw ? "Preview" : "Markdown source"} onClick={() => setRaw((value) => !value)}>{raw ? <Eye size={15} /> : <Code2 size={15} />}</Button>
              <Button variant="ghost" size="icon" aria-label="Refresh brief" title="Refresh" onClick={() => void brief.refetch()}><RefreshCw size={15} className={brief.isFetching ? "animate-spin" : ""} /></Button>
              <Button variant="outline" size="sm" disabled={!markdown} onClick={copy}><Clipboard size={14} /> Copy</Button>
              <Button variant="outline" size="sm" onClick={() => void save()}><FileDown size={14} /> Save to context.md</Button>
            </div>
          </DialogHeader>
          <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
            {raw
              ? <pre className="whitespace-pre-wrap rounded-md border border-border bg-muted/20 p-4 font-mono text-xs leading-relaxed">{markdown}</pre>
              : <div className="mx-auto max-w-3xl"><Markdown source={markdown} /></div>}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

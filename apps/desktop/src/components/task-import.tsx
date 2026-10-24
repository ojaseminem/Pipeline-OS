// Import tasks from pasted TOML or a .toml file: shows exactly what each task
// will run before anything is saved, then adds them to the pipeline or keeps
// them as project presets.
import { useEffect, useRef, useState } from "react";
import { FileUp, ShieldAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { studioApi, type TaskDefinition } from "../bridge";
import { KIND_ICONS } from "../lib/task-meta";

const EXAMPLE = `[[task]]
id = "build-and-zip"
name = "Build and zip (Windows)"
kind = "build"
program = "python"
args = ["Tools/build_and_zip.py", "--target", "win64"]`;

export function ImportTasksDialog({ open, onOpenChange, onAddTasks, onSavePresets }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAddTasks: (tasks: TaskDefinition[]) => Promise<boolean>;
  onSavePresets: (presets: TaskDefinition[]) => Promise<boolean>;
}) {
  const [text, setText] = useState("");
  const [parsed, setParsed] = useState<TaskDefinition[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  useEffect(() => { if (!open) { setText(""); setParsed(null); setError(null); } }, [open]);

  // Validate as the user types (debounced): the backend parses and checks ids,
  // programs and working directories, so the preview is exactly what saves.
  useEffect(() => {
    if (!text.trim()) { setParsed(null); setError(null); return; }
    const handle = window.setTimeout(() => {
      studioApi.parseTasksToml(text)
        .then((tasks) => { setParsed(tasks); setError(null); })
        .catch((reason) => { setParsed(null); setError(reason instanceof Error ? reason.message : String(reason)); });
    }, 250);
    return () => window.clearTimeout(handle);
  }, [text]);

  async function readFile(file: File | undefined) {
    if (file) setText(await file.text());
  }

  async function finish(action: (tasks: TaskDefinition[]) => Promise<boolean>) {
    if (!parsed?.length) return;
    setBusy(true);
    try { if (await action(parsed)) onOpenChange(false); } finally { setBusy(false); }
  }

  const count = parsed?.length ?? 0;
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>Import tasks</DialogTitle>
          <DialogDescription>Paste tasks copied from another project (<code>[[task]]</code> or <code>[[preset]]</code> tables, or a whole <code>tasks.toml</code>), or open a <code>.toml</code> file.</DialogDescription>
        </DialogHeader>
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs text-muted-foreground">Same format as <code>.pipelineos/tasks.toml</code></span>
          <Button variant="outline" size="sm" onClick={() => fileInput.current?.click()}><FileUp size={14} /> Open file…</Button>
          <input ref={fileInput} type="file" accept=".toml,.txt,text/plain" className="hidden" onChange={(event) => { void readFile(event.target.files?.[0]); event.target.value = ""; }} />
        </div>
        <Textarea aria-label="Task TOML" spellCheck={false} value={text} onChange={(event) => setText(event.target.value)} placeholder={EXAMPLE} className="min-h-40 font-mono text-xs" />
        <div className="-mx-1 min-h-0 flex-1 overflow-y-auto px-1">
          {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
          {parsed?.length ? (
            <div className="space-y-2">
              <p className="flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs"><ShieldAlert size={14} className="mt-0.5 shrink-0 text-warning" aria-hidden /> Running a task runs exactly these commands on your machine. Only import tasks from people you trust.</p>
              <ul className="divide-y divide-border rounded-lg border border-border">
                {parsed.map((task) => {
                  const Icon = KIND_ICONS[task.kind];
                  return (
                    <li key={task.id} className="flex items-start gap-3 px-3 py-2">
                      <Icon size={15} className="mt-0.5 shrink-0 text-muted-foreground" aria-hidden />
                      <div className="min-w-0 flex-1">
                        <div className="flex items-baseline gap-2 text-sm font-medium">{task.name}<span className="font-mono text-xs font-normal text-muted-foreground">{task.id}</span></div>
                        <code className="block whitespace-pre-wrap break-all text-[11px] text-muted-foreground">{[task.program, ...task.args].join(" ")}{task.cwd ? `   (in ${task.cwd})` : ""}</code>
                        {task.env && Object.keys(task.env).length ? <code className="block text-[11px] text-muted-foreground">{Object.entries(task.env).map(([key, value]) => `${key}=${value}`).join("  ")}</code> : null}
                      </div>
                    </li>
                  );
                })}
              </ul>
            </div>
          ) : null}
        </div>
        <DialogFooter className="gap-2 sm:justify-between">
          <Button variant="outline" disabled={!count || busy} onClick={() => void finish(onSavePresets)}>Save as project preset{count === 1 ? "" : "s"}</Button>
          <Button disabled={!count || busy} onClick={() => void finish(onAddTasks)}>{count ? `Add ${count} task${count === 1 ? "" : "s"} to the pipeline` : "Paste tasks to import"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

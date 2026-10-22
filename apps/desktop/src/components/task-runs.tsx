// Live pipeline task runs: one subscription to the backend's task events,
// shared by Home ("In progress"), the project Pipeline tab, the sidebar
// indicator, and the log viewer.
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Ban, CheckCircle2, Clipboard, Clock3, Loader2, Play, Square, TerminalSquare, XCircle } from "lucide-react";
import { KIND_ICONS, formatDuration } from "../lib/task-meta";
import { TaskRunsContext, useTaskRuns, type TaskRunsContextValue } from "../lib/task-runs-context";
import { toast } from "sonner";
import { getPrefs } from "../lib/prefs";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { onTaskEvents, studioApi, studioAvailable, type RunStatus, type TaskRun } from "../bridge";

const MAX_LOG_LINES = 2_000;

export function TaskRunsProvider({ children }: { children: ReactNode }) {
  const [runs, setRuns] = useState<TaskRun[]>([]);
  const [logs, setLogs] = useState<Record<string, string[]>>({});
  const [viewing, setViewing] = useState<string | null>(null);
  const previous = useRef<Map<string, RunStatus>>(new Map());

  const refresh = useCallback(() => {
    if (!studioAvailable()) return;
    studioApi.runs().then(setRuns).catch(() => undefined);
  }, []);

  useEffect(() => {
    refresh();
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void onTaskEvents(
      (run) => {
        setRuns((current) => {
          const index = current.findIndex((item) => item.id === run.id);
          if (index < 0) return [run, ...current];
          const next = current.slice();
          next[index] = run;
          return next;
        });
      },
      (batch) => {
        setLogs((current) => {
          const existing = current[batch.runId] ?? [];
          const merged = existing.concat(batch.lines.map((line) => (line.stream === "stderr" ? `\u0000${line.text}` : line.text)));
          return { ...current, [batch.runId]: merged.length > MAX_LOG_LINES ? merged.slice(-MAX_LOG_LINES) : merged };
        });
      },
    ).then((off) => { if (disposed) off(); else unlisten = off; });
    return () => { disposed = true; unlisten?.(); };
  }, [refresh]);

  // Announce finished runs once, with a shortcut to the log.
  useEffect(() => {
    for (const run of runs) {
      const before = previous.current.get(run.id);
      if (before === "running" && run.status !== "running" && getPrefs().taskToasts) {
        const detail = `${run.projectName} · ${formatDuration(run.durationMs)}`;
        const action = { label: "View log", onClick: () => setViewing(run.id) };
        if (run.status === "succeeded") toast.success(`${run.taskName} finished`, { description: detail, action });
        else if (run.status === "failed") toast.error(`${run.taskName} failed`, { description: run.lastLine ?? detail, action });
        else if (run.status === "cancelled") toast.message(`${run.taskName} cancelled`, { description: detail });
      }
      previous.current.set(run.id, run.status);
    }
  }, [runs]);

  const start = useCallback(async (path: string, taskId: string) => {
    try {
      const run = await studioApi.runTask(path, taskId);
      setRuns((current) => [run, ...current.filter((item) => item.id !== run.id)]);
      previous.current.set(run.id, "running");
      return run;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (message.startsWith("TASKS_UNTRUSTED")) throw error;
      toast.error("Could not start the task", { description: message });
      return null;
    }
  }, []);

  const cancel = useCallback(async (runId: string) => {
    try { await studioApi.cancelRun(runId); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }, []);

  const openLog = useCallback((runId: string) => setViewing(runId), []);

  const value = useMemo<TaskRunsContextValue>(() => ({
    runs,
    active: runs.filter((run) => run.status === "running" || run.status === "queued"),
    logs,
    start,
    cancel,
    openLog,
    refresh,
  }), [runs, logs, start, cancel, openLog, refresh]);

  return (
    <TaskRunsContext.Provider value={value}>
      {children}
      <RunLogDialog runId={viewing} onClose={() => setViewing(null)} />
    </TaskRunsContext.Provider>
  );
}

export function RunStatusIcon({ status, size = 15 }: { status: RunStatus; size?: number }) {
  switch (status) {
    case "running": return <Loader2 size={size} className="animate-spin text-primary" aria-label="Running" />;
    case "queued": return <Clock3 size={size} className="text-muted-foreground" aria-label="Queued" />;
    case "succeeded": return <CheckCircle2 size={size} className="text-success" aria-label="Succeeded" />;
    case "failed": return <XCircle size={size} className="text-destructive" aria-label="Failed" />;
    case "cancelled": return <Ban size={size} className="text-muted-foreground" aria-label="Cancelled" />;
  }
}

/** Thin progress bar; indeterminate shimmer when the tool reports no %. */
export function RunProgress({ run }: { run: TaskRun }) {
  const running = run.status === "running";
  const value = run.progress ?? (run.status === "succeeded" ? 100 : null);
  return (
    <div className="relative h-1.5 w-full overflow-hidden rounded-full bg-secondary" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={value ?? undefined} aria-label={`${run.taskName} progress`}>
      {value != null ? (
        <div className={cn("h-full rounded-full transition-[width] duration-300", run.status === "failed" ? "bg-destructive" : run.status === "cancelled" ? "bg-muted-foreground" : "bg-primary")} style={{ width: `${value}%` }} />
      ) : running ? (
        <div className="pos-indeterminate absolute inset-y-0 w-1/3 rounded-full bg-primary/70" />
      ) : null}
    </div>
  );
}

export function RunRow({ run, showProject = true }: { run: TaskRun; showProject?: boolean }) {
  const { cancel, openLog } = useTaskRuns();
  const Icon = KIND_ICONS[run.kind] ?? TerminalSquare;
  const running = run.status === "running";
  return (
    <div className="group flex items-center gap-3 rounded-lg px-2 py-2 hover:bg-muted/40">
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-secondary text-muted-foreground"><Icon size={16} aria-hidden /></span>
      <button className="min-w-0 flex-1 text-left" onClick={() => openLog(run.id)} title="View log">
        <span className="flex items-center gap-2 text-sm">
          <RunStatusIcon status={run.status} size={14} />
          <span className="truncate font-medium">{run.taskName}</span>
          {showProject ? <span className="truncate text-xs text-muted-foreground">· {run.projectName}</span> : null}
          <span className="ml-auto shrink-0 text-xs tabular-nums text-muted-foreground">
            {running ? (run.progress != null ? `${Math.round(run.progress)}%` : "") : formatDuration(run.durationMs)}
          </span>
        </span>
        {running ? <span className="mt-1.5 block"><RunProgress run={run} /></span> : null}
        {run.lastLine ? <span className="mt-1 block truncate font-mono text-[11px] text-muted-foreground">{run.lastLine}</span> : null}
      </button>
      {running ? <Button variant="ghost" size="icon" className="h-7 w-7 shrink-0" aria-label={`Cancel ${run.taskName}`} title="Cancel" onClick={() => void cancel(run.id)}><Square size={13} /></Button> : null}
    </div>
  );
}

function RunLogDialog({ runId, onClose }: { runId: string | null; onClose: () => void }) {
  const { runs, logs, cancel, start } = useTaskRuns();
  const run = runs.find((item) => item.id === runId) ?? null;
  const [history, setHistory] = useState<string[]>([]);
  const scroller = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);

  useEffect(() => {
    setHistory([]);
    setFollow(true);
    if (runId) studioApi.runLog(runId).then(setHistory).catch(() => undefined);
  }, [runId]);

  const live = runId ? logs[runId] ?? [] : [];
  // Prefer the live stream once it has more than the fetched snapshot.
  const lines = live.length >= history.length ? live : history;

  useEffect(() => {
    if (follow && scroller.current) scroller.current.scrollTop = scroller.current.scrollHeight;
  }, [lines.length, follow]);

  return (
    <Dialog open={!!run} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="flex max-h-[85vh] flex-col sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">{run ? <RunStatusIcon status={run.status} /> : null}{run?.taskName}<span className="text-sm font-normal text-muted-foreground">· {run?.projectName}</span></DialogTitle>
          <DialogDescription className="truncate font-mono text-xs" title={run?.commandLine}>{run?.commandLine}</DialogDescription>
        </DialogHeader>
        {run?.status === "running" ? <RunProgress run={run} /> : null}
        <div
          ref={scroller}
          onScroll={(event) => {
            const el = event.currentTarget;
            setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 24);
          }}
          className="min-h-64 flex-1 overflow-auto rounded-md border border-border bg-black/40 p-3 font-mono text-xs leading-relaxed"
          role="log"
          aria-live="polite"
          aria-label="Task output"
        >
          {lines.length ? lines.map((line, index) => {
            const stderr = line.startsWith("\u0000");
            const text = stderr ? line.slice(1) : line;
            return <div key={index} className={cn("whitespace-pre-wrap break-all", stderr || /\berror\b/i.test(text) ? "text-destructive" : /\bwarn(ing)?\b/i.test(text) ? "text-warning" : "text-foreground/85")}>{text || " "}</div>;
          }) : <div className="text-muted-foreground">{run?.status === "running" ? "Waiting for output…" : "No output was captured for this run in the current session."}</div>}
        </div>
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span>{run ? `${run.status[0].toUpperCase()}${run.status.slice(1)}` : ""}{run?.exitCode != null ? ` · exit code ${run.exitCode}` : ""}{run?.durationMs != null ? ` · ${formatDuration(run.durationMs)}` : ""}</span>
          <span className="ml-auto flex gap-2">
            <Button variant="outline" size="sm" onClick={() => { void navigator.clipboard?.writeText(lines.map((line) => (line.startsWith("\u0000") ? line.slice(1) : line)).join("\n")); toast.success("Log copied."); }}><Clipboard size={14} /> Copy log</Button>
            {run?.status === "running" ? <Button variant="outline" size="sm" onClick={() => void cancel(run.id)}><Square size={13} /> Cancel</Button>
              : run ? <Button size="sm" onClick={() => void start(run.projectPath, run.taskId).catch((error) => toast.error(error instanceof Error ? error.message : String(error)))}><Play size={13} /> Run again</Button> : null}
          </span>
        </div>
      </DialogContent>
    </Dialog>
  );
}

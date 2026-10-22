// CI checks for a branch: rollup, every check run and status, re-run failed
// GitHub Actions runs (manually or automatically once), and the failing log
// tail right here, so you rarely need to open the browser.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, ExternalLink, RotateCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { openExternal, scmApi, type CheckItem } from "../../bridge";
import { usePrefs } from "../../lib/prefs";
import { formatLastOpened } from "../../lib/format";
import { RollupIcon } from "./shared";
import { attempt, conclusionRollup, errorText } from "../../lib/scm";

function JobLogs({ root, runId }: { root: string; runId: number }) {
  const logs = useQuery({ queryKey: ["job-logs", root, runId], queryFn: () => scmApi.failedJobLogs(root, runId), retry: false, staleTime: Infinity });
  if (logs.isLoading) return <p className="px-3 py-2 text-xs text-muted-foreground">Loading the failing log…</p>;
  if (logs.isError) return <p className="px-3 py-2 text-xs text-destructive">{errorText(logs.error)}</p>;
  return (
    <div className="space-y-2 px-3 py-2">
      {(logs.data ?? []).map((job) => (
        <div key={job.jobId} className="space-y-1">
          <p className="text-xs font-medium">{job.name}{job.failedStep ? <span className="text-muted-foreground"> · failed at “{job.failedStep}”</span> : null}</p>
          <pre className="max-h-64 overflow-auto rounded-md border border-border bg-muted/30 p-2 font-mono text-[11px] leading-relaxed">{job.logTail || "(no log output)"}</pre>
        </div>
      ))}
      {logs.data && !logs.data.length ? <p className="text-xs text-muted-foreground">No failed jobs in this run.</p> : null}
    </div>
  );
}

export function BranchChecks({ root, branch, compact = false }: { root: string; branch?: string; compact?: boolean }) {
  const queryClient = useQueryClient();
  const [prefs, setPrefs] = usePrefs();
  const key = ["branch-checks", root, branch ?? "HEAD"];
  const checks = useQuery({
    queryKey: key,
    queryFn: () => scmApi.checksForBranch(root, branch),
    retry: false,
    refetchInterval: (query) => (query.state.data?.rollup === "pending" ? 20_000 : 120_000),
  });
  const [expanded, setExpanded] = useState<string | null>(null);
  const retriedSha = useRef<string | null>(null);
  const data = checks.data;

  // Auto-retry: re-run each failed workflow once per commit, so a flaky
  // runner or network blip doesn't need a manual click.
  useEffect(() => {
    if (!prefs.autoRetryChecks || !data || data.rollup !== "failure" || retriedSha.current === data.sha) return;
    retriedSha.current = data.sha;
    scmApi.autoRetryChecks(root, branch, 2).then((result) => {
      if (result.retried.length) {
        toast.message(`Re-running ${result.retried.length} failed check${result.retried.length === 1 ? "" : "s"}`, { description: result.retried.map((run) => run.name).join(", ") });
        void queryClient.invalidateQueries({ queryKey: key });
      }
    }).catch(() => undefined);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data?.sha, data?.rollup, prefs.autoRetryChecks]);

  if (checks.isLoading) return <p className="text-sm text-muted-foreground">Loading checks…</p>;
  if (checks.isError) return <p className="text-sm text-muted-foreground">{errorText(checks.error)}</p>;
  if (!data || !data.total) return <p className="text-sm text-muted-foreground">No checks have run on {branch ?? "this branch"} yet.</p>;

  const failedRuns = [...new Set(data.items.filter((item) => conclusionRollup(item.status, item.conclusion) === "failure" && item.runId).map((item) => item.runId as number))];

  const row = (item: CheckItem) => {
    const rollup = conclusionRollup(item.status, item.conclusion);
    const id = `${item.kind}:${item.name}:${item.jobId ?? item.runId ?? ""}`;
    const open = expanded === id;
    return (
      <li key={id} className="border-t border-border first:border-t-0">
        <div className="flex items-center gap-2 px-3 py-2 text-sm">
          {rollup === "failure" && item.runId ? <button aria-label={open ? "Hide log" : "Show log"} onClick={() => setExpanded(open ? null : id)}>{open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</button> : <span className="w-3.5" />}
          <RollupIcon rollup={rollup} />
          <span className="min-w-0 flex-1 truncate" title={item.description ?? item.name}>{item.name}{item.app ? <span className="text-muted-foreground"> · {item.app}</span> : null}</span>
          <span className="shrink-0 text-xs text-muted-foreground">{item.completedAt ? formatLastOpened(item.completedAt) : item.status === "in_progress" ? "running" : item.status}</span>
          {item.url ? <Button variant="ghost" size="icon" className="h-6 w-6" aria-label={`Open ${item.name}`} onClick={() => void openExternal(item.url!)}><ExternalLink size={12} /></Button> : null}
        </div>
        {open && item.runId ? <JobLogs root={root} runId={item.runId} /> : null}
      </li>
    );
  };

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <RollupIcon rollup={data.rollup} size={16} />
        <span className="font-medium">{data.rollup === "success" ? "All checks passed" : data.rollup === "failure" ? `${data.failed} failing` : data.rollup === "pending" ? `${data.pending} running` : "Checks finished"}</span>
        <span className="text-xs text-muted-foreground">{data.passed}/{data.total} passed · {data.sha.slice(0, 7)}</span>
        <span className="ml-auto flex items-center gap-1.5">
          {!compact ? <label className="flex items-center gap-1.5 text-xs text-muted-foreground" title="Re-runs each failed GitHub Actions workflow once per commit"><input type="checkbox" className="size-3.5 accent-[var(--primary)]" checked={prefs.autoRetryChecks} onChange={(event) => setPrefs({ autoRetryChecks: event.target.checked })} /> Auto-retry failures</label> : null}
          {failedRuns.length ? <Button variant="outline" size="sm" className="h-7" onClick={() => void Promise.all(failedRuns.map((runId) => attempt("Re-running checks", () => scmApi.rerunFailed(root, runId)))).then(() => { toast.success("Re-running failed jobs"); void queryClient.invalidateQueries({ queryKey: key }); })}><RotateCw size={13} /> Re-run failed</Button> : null}
          <Button variant="ghost" size="icon" className="h-7 w-7" aria-label="Refresh checks" onClick={() => void checks.refetch()}><RotateCw size={13} className={checks.isFetching ? "animate-spin" : ""} /></Button>
        </span>
      </div>
      <ul className="rounded-lg border border-border">{[...data.items].sort((a, b) => ["failure", "pending", "neutral", "success"].indexOf(conclusionRollup(a.status, a.conclusion)) - ["failure", "pending", "neutral", "success"].indexOf(conclusionRollup(b.status, b.conclusion))).map(row)}</ul>
    </div>
  );
}

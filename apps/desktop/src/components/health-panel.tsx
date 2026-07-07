import { useEffect, useState } from "react";
import { CircleAlert, Eye, EyeOff, Wrench } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { loadDismissedHealth, saveDismissedHealth } from "../lib/local-store";
import { desktopApi, type HealthIssue } from "../bridge";

type FixAction = {
  /** Short verb-first label shown on the Fix button. */
  label: string;
  /** If set, shown as a window.confirm() before running (for anything that
   *  writes files or invokes an external installer). */
  confirmMessage?: string;
  run: (projectPath: string) => Promise<string>;
};

/** Fix actions for health issue codes that can be resolved with one click.
 *  Codes not listed here fall back to the plain-text remediation note —
 *  there's no safe automatic fix (e.g. a missing drive, uncommitted changes
 *  that are the user's call, or a history rewrite). */
const FIX_ACTIONS: Record<string, FixAction> = {
  APP_NOT_INSTALLED: { label: "Rescan applications", run: async () => { await desktopApi.scanApps([]); return "Applications rescanned."; } },
  APP_INSTALL_STALE: { label: "Rescan applications", run: async () => { await desktopApi.scanApps([]); return "Applications rescanned."; } },
  APP_CHECK_FAILED: { label: "Rescan applications", run: async () => { await desktopApi.scanApps([]); return "Applications rescanned."; } },
  PROJECT_CONFIG_INVALID: {
    label: "Repair project.toml",
    confirmMessage: "Regenerate .pipelineos/project.toml from this project's files? An existing broken file is renamed aside, not deleted.",
    run: async (projectPath) => { await desktopApi.repairProjectConfig(projectPath, true); return "Project metadata repaired."; },
  },
  GIT_LFS_NOT_INSTALLED: { label: "Install Git LFS", run: async () => desktopApi.installGitLfs() },
  GIT_LFS_NOT_INITIALIZED: {
    label: "Track with Git LFS",
    confirmMessage: "Set up Git LFS for this project and track its large files? This stages changes to .gitattributes — review and commit them afterward.",
    run: async (projectPath) => { const result = await desktopApi.gitLfsTrackLargeFiles(projectPath, true); return result.stdout || "Large files are now tracked with Git LFS."; },
  },
  LARGE_FILE_NOT_TRACKED: {
    label: "Track with Git LFS",
    confirmMessage: "Set up Git LFS for this project and track its large files? This stages changes to .gitattributes — review and commit them afterward.",
    run: async (projectPath) => { const result = await desktopApi.gitLfsTrackLargeFiles(projectPath, true); return result.stdout || "Large files are now tracked with Git LFS."; },
  },
};

/** Renders a project's health issues with per-issue dismiss/hide, persisted per
 *  project. Hidden issues can be revealed again from here. Issues with a known
 *  fix show a one-click "Fix" button; others show their remediation note as
 *  information only — call `onFixed` to refresh the health check afterward. */
export function HealthPanel({ projectPath, issues, emptyText, onFixed }: { projectPath: string; issues: HealthIssue[]; emptyText?: string; onFixed?: () => void }) {
  const [dismissed, setDismissed] = useState<string[]>(() => loadDismissedHealth(projectPath));
  const [showHidden, setShowHidden] = useState(false);
  const [fixing, setFixing] = useState<string | null>(null);

  useEffect(() => setDismissed(loadDismissedHealth(projectPath)), [projectPath]);

  function toggle(code: string) {
    const next = dismissed.includes(code) ? dismissed.filter((value) => value !== code) : [...dismissed, code];
    setDismissed(next);
    saveDismissedHealth(projectPath, next);
  }

  async function runFix(code: string, action: FixAction) {
    if (action.confirmMessage && !window.confirm(action.confirmMessage)) return;
    setFixing(code);
    try {
      const message = await action.run(projectPath);
      toast.success(message);
      onFixed?.();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setFixing(null);
    }
  }

  const hiddenCount = issues.filter((issue) => dismissed.includes(issue.code)).length;
  const visible = issues.filter((issue) => showHidden || !dismissed.includes(issue.code));

  if (issues.length === 0) {
    return <p className="text-sm text-muted-foreground">{emptyText ?? "No health issues. Everything looks good."}</p>;
  }

  return (
    <div className="space-y-2">
      {hiddenCount > 0 ? (
        <button onClick={() => setShowHidden((value) => !value)} className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground">
          {showHidden ? <EyeOff size={13} /> : <Eye size={13} />}
          {showHidden ? "Hide dismissed" : `Show ${hiddenCount} dismissed`}
        </button>
      ) : null}
      {visible.map((issue) => {
        const isHidden = dismissed.includes(issue.code);
        const fix = FIX_ACTIONS[issue.code];
        return (
          <div key={issue.code} className={cn("flex items-start gap-2.5 rounded-lg border border-border p-3", isHidden && "opacity-60")}>
            <CircleAlert size={16} className={cn("mt-0.5 shrink-0", issue.severity === "error" ? "text-destructive" : "text-primary")} />
            <div className="min-w-0 flex-1">
              <h4 className="font-medium">{issue.title}</h4>
              <p className="text-sm text-muted-foreground">{issue.detail}</p>
              {issue.remediation ? <p className="mt-1 text-xs text-muted-foreground"><span className="text-foreground">{fix ? "Or fix it yourself:" : "Fix:"}</span> {issue.remediation}</p> : null}
              <small className="text-[10px] uppercase tracking-wide text-muted-foreground">{issue.code}</small>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              {fix ? (
                <Button variant="outline" size="sm" disabled={fixing === issue.code} onClick={() => void runFix(issue.code, fix)}>
                  <Wrench size={13} /> {fixing === issue.code ? "Fixing…" : fix.label}
                </Button>
              ) : null}
              <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => toggle(issue.code)}>
                {isHidden ? "Unhide" : "Dismiss"}
              </Button>
            </div>
          </div>
        );
      })}
    </div>
  );
}

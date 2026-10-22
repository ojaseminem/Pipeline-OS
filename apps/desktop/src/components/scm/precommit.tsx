// Pre-commit checks shown above the commit button: conflict markers, secrets,
// oversized files, Unity .meta pairing, generated folders, risky file names.
// Errors hold the commit until fixed (one click where possible) or overridden.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, ChevronDown, ChevronRight, Info, ShieldCheck, Wand2, XCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { scmApi, type CheckFinding } from "../../bridge";
import { attempt } from "../../lib/scm";

const FIX_LABEL: Record<string, string> = { trackWithLfs: "Track with Git LFS", addGitignore: "Add to .gitignore", untrackPaths: "Stop tracking" };

export function PrecommitChecks({ root, paths, statusKey, onBlockingChange }: { root: string; paths: string[]; statusKey: string; onBlockingChange: (blocking: number) => void }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const sorted = [...paths].sort();
  const checks = useQuery({ queryKey: ["precommit", root, statusKey, sorted.join("\n")], queryFn: () => scmApi.checks(root, sorted.length ? sorted : undefined), enabled: paths.length > 0, retry: false, staleTime: 30_000 });
  const findings = (checks.data ?? []).filter((finding) => finding.severity !== "info");
  const errors = findings.filter((finding) => finding.severity === "error").length;
  useEffect(() => { onBlockingChange(errors); }, [errors, onBlockingChange]);

  if (!paths.length) return null;
  if (checks.isLoading) return <p className="flex items-center gap-1.5 px-1 text-xs text-muted-foreground"><ShieldCheck size={13} /> Checking changes…</p>;
  if (!findings.length) return <p className="flex items-center gap-1.5 px-1 text-xs text-success"><ShieldCheck size={13} /> Pre-commit checks passed</p>;

  async function fix(finding: CheckFinding) {
    if (!finding.fix) return;
    const done = await attempt(finding.title, () => scmApi.fix(root, finding.fix!), `Fixed: ${finding.title}`);
    if (done) {
      void queryClient.invalidateQueries({ queryKey: ["precommit", root] });
      void queryClient.invalidateQueries({ queryKey: ["git-status", root] });
    }
  }

  return (
    <div className={cn("rounded-md border text-xs", errors ? "border-destructive/40 bg-destructive/5" : "border-warning/40 bg-warning/5")}>
      <button className="flex w-full items-center gap-1.5 px-2 py-1.5 text-left font-medium" onClick={() => setOpen(!open)} aria-expanded={open}>
        {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        {errors ? <XCircle size={13} className="text-destructive" /> : <AlertTriangle size={13} className="text-warning" />}
        {errors ? `${errors} problem${errors === 1 ? "" : "s"} to fix before committing` : `${findings.length} warning${findings.length === 1 ? "" : "s"}`}
      </button>
      {open ? (
        <ul className="space-y-1.5 border-t border-border/60 px-2 py-2">
          {findings.map((finding) => (
            <li key={finding.id} className="space-y-1">
              <div className="flex items-start gap-1.5">
                {finding.severity === "error" ? <XCircle size={12} className="mt-0.5 shrink-0 text-destructive" /> : finding.severity === "warning" ? <AlertTriangle size={12} className="mt-0.5 shrink-0 text-warning" /> : <Info size={12} className="mt-0.5 shrink-0" />}
                <span className="min-w-0 flex-1"><span className="font-medium">{finding.title}</span><span className="block text-muted-foreground">{finding.detail}</span></span>
                {finding.fix ? <Button variant="outline" size="sm" className="h-6 shrink-0 px-2 text-[11px]" onClick={() => void fix(finding)}><Wand2 size={11} /> {FIX_LABEL[finding.fix.kind] ?? "Fix"}</Button> : null}
              </div>
              {(finding.locations.length ? finding.locations : finding.paths).slice(0, 6).map((location) => <code key={location} className="ml-4 block truncate text-[11px] text-muted-foreground" title={location}>{location}</code>)}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

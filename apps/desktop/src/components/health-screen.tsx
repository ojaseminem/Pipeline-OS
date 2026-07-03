import { useEffect, useState } from "react";
import { Activity, Folder, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { HealthPanel } from "./health-panel";
import { desktopApi, isNativeRuntime, type HealthIssue, type RegisteredProject } from "../bridge";
import { formatLastOpened } from "../lib/format";

/** Workspace-wide health: a resizable project list on the left, the selected
 *  project's full issue detail on the right — mirrors the Source Control
 *  tab's list/detail split so the two feel consistent. */
export function HealthScreen({ projects, onOpenProject }: { projects: RegisteredProject[]; onOpenProject: (project: { path: string; name: string }) => void }) {
  const [results, setResults] = useState<Record<string, HealthIssue[]>>({});
  const [checkedAt, setCheckedAt] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [checkingAll, setCheckingAll] = useState(false);

  // Pre-populate from cached health so the screen is never blank on open, then
  // scan any project that has no cached result yet (so first open populates and
  // caches without the user having to press Re-check).
  useEffect(() => {
    if (!isNativeRuntime()) return;
    let active = true;
    desktopApi.healthOverview().then(async (overview) => {
      if (!active) return;
      const cachedResults: Record<string, HealthIssue[]> = {};
      const cachedAt: Record<string, string> = {};
      for (const entry of overview) {
        if (entry.checkedAt) { cachedResults[entry.path] = entry.issues; cachedAt[entry.path] = entry.checkedAt; }
      }
      setResults(cachedResults);
      setCheckedAt(cachedAt);
      for (const entry of overview) {
        if (active && !entry.checkedAt) await runOne(entry.path, false);
      }
    }).catch(() => undefined);
    return () => { active = false; };
  }, []);

  async function runOne(path: string, select = true) {
    if (!isNativeRuntime()) return;
    setLoading(path);
    try {
      const issues = await desktopApi.projectHealth(path);
      setResults((current) => ({ ...current, [path]: issues }));
      setCheckedAt((current) => ({ ...current, [path]: new Date().toISOString() }));
      if (select) setSelected(path);
    } finally {
      setLoading(null);
    }
  }

  async function checkAll() {
    setCheckingAll(true);
    try {
      for (const project of projects) {
        await runOne(project.path, false);
      }
    } finally {
      setCheckingAll(false);
    }
  }

  const counts = (issues: HealthIssue[]) => ({
    errors: issues.filter((issue) => issue.severity === "error").length,
    warnings: issues.filter((issue) => issue.severity !== "error").length,
  });

  const selectedProject = projects.find((project) => project.path === selected) ?? projects[0] ?? null;
  const selectedIssues = selectedProject ? results[selectedProject.path] : undefined;

  return (
    <section className="space-y-5">
      <div className="flex items-center justify-between">
        <div><div className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Workspace</div><h1 className="text-2xl font-semibold">Health</h1></div>
        <Button variant="outline" disabled={checkingAll || !projects.length || !isNativeRuntime()} onClick={() => void checkAll()}>
          {checkingAll ? <><RefreshCw size={15} className="animate-spin" /> Checking all…</> : <><Activity size={15} /> Check all projects</>}
        </Button>
      </div>

      {projects.length === 0 ? (
        <Card><CardContent className="p-6"><p className="text-sm text-muted-foreground">No projects registered yet. Import a project to track its health.</p></CardContent></Card>
      ) : (
        <ResizablePanelGroup direction="horizontal" className="min-h-[320px] gap-4" style={{ height: "calc(100vh - 230px)" }}>
          <ResizablePanel defaultSize={36} minSize={24} className="flex min-w-0 flex-col">
            <Card className="flex h-full flex-col overflow-hidden"><CardContent className="flex min-h-0 flex-1 flex-col p-0">
              <div className="flex-1 overflow-y-auto">
                {projects.map((project) => {
                  const issues = results[project.path];
                  const tally = issues ? counts(issues) : null;
                  const isSelected = selectedProject?.path === project.path;
                  return (
                    <button key={project.path} onClick={() => setSelected(project.path)} className={`flex w-full items-center gap-3 border-t border-border px-3 py-2.5 text-left first:border-t-0 hover:bg-muted/40 ${isSelected ? "bg-muted/60" : ""}`}>
                      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-secondary text-primary"><Folder size={17} /></span>
                      <span className="min-w-0 flex-1">
                        <strong className="block truncate text-sm font-medium">{project.name}</strong>
                        <small className="block truncate text-xs text-muted-foreground">{checkedAt[project.path] ? `Checked ${formatLastOpened(checkedAt[project.path])}` : "Not checked"}</small>
                      </span>
                      {tally ? (
                        tally.errors === 0 && tally.warnings === 0
                          ? <Badge variant="secondary" className="shrink-0 text-primary">Healthy</Badge>
                          : <span className="flex shrink-0 items-center gap-1">
                              {tally.errors ? <Badge variant="outline" className="border-destructive/50 text-destructive">{tally.errors}</Badge> : null}
                              {tally.warnings ? <Badge variant="outline">{tally.warnings}</Badge> : null}
                            </span>
                      ) : loading === project.path ? <RefreshCw size={13} className="shrink-0 animate-spin text-muted-foreground" /> : <span className="shrink-0 text-xs text-muted-foreground">—</span>}
                    </button>
                  );
                })}
              </div>
            </CardContent></Card>
          </ResizablePanel>
          <ResizableHandle withHandle />
          <ResizablePanel defaultSize={64} minSize={35} className="flex min-w-0 flex-col">
            <Card className="flex h-full flex-col overflow-hidden"><CardContent className="flex min-h-0 flex-1 flex-col p-4">
              {selectedProject ? (
                <>
                  <div className="mb-3 flex shrink-0 items-center justify-between gap-2">
                    <div className="min-w-0">
                      <h2 className="truncate text-base font-semibold">{selectedProject.name}</h2>
                      <p className="truncate text-xs text-muted-foreground">{checkedAt[selectedProject.path] ? `Checked ${formatLastOpened(checkedAt[selectedProject.path])}` : selectedProject.path}</p>
                    </div>
                    <span className="flex shrink-0 items-center gap-1.5">
                      <Button variant="outline" size="sm" disabled={loading === selectedProject.path || !isNativeRuntime()} onClick={() => void runOne(selectedProject.path)}>
                        {loading === selectedProject.path ? <RefreshCw size={14} className="animate-spin" /> : <RefreshCw size={14} />} {selectedIssues ? "Re-check" : "Run checks"}
                      </Button>
                      <Button variant="ghost" size="sm" onClick={() => onOpenProject({ path: selectedProject.path, name: selectedProject.name })}>Open</Button>
                    </span>
                  </div>
                  <div className="flex-1 overflow-y-auto">
                    {selectedIssues ? <HealthPanel projectPath={selectedProject.path} issues={selectedIssues} onFixed={() => void runOne(selectedProject.path, false)} /> : <p className="text-sm text-muted-foreground">{loading === selectedProject.path ? "Checking…" : "Not checked yet — press Run checks."}</p>}
                  </div>
                </>
              ) : <p className="text-sm text-muted-foreground">Select a project to see its health.</p>}
            </CardContent></Card>
          </ResizablePanel>
        </ResizablePanelGroup>
      )}
    </section>
  );
}

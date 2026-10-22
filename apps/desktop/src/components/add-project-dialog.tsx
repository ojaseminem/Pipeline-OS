import { useState, type FormEvent } from "react";
import { FolderPlus, HardDrive, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { PathInput } from "./path-input";
import { DiscoveredProjects } from "./discovered-projects";
import { platformExamples } from "../lib/platform";
import { useDiscovery } from "../lib/discovery";

/** One place to bring projects in: pick a folder, add what Pipeline OS found
 *  on this computer (all at once or a few), or deep-scan drives (Windows).
 *  Dragging a folder onto the window works too. */
export function AddProjectDialog({ open, onOpenChange, onImport, onScan, scanning, scanLabel, onCancelScan, driveSummary, canScanDrives, onAdded }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImport: (path: string, name: string) => Promise<boolean>;
  onScan: () => void;
  scanning: boolean;
  scanLabel: string;
  onCancelScan: () => void;
  driveSummary: string;
  canScanDrives: boolean;
  onAdded: () => void;
}) {
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const found = useDiscovery().projects.length > 0;

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!path.trim()) return;
    setBusy(true);
    try {
      if (await onImport(path.trim(), name.trim())) { setPath(""); setName(""); onOpenChange(false); }
    } finally { setBusy(false); }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Add a project</DialogTitle>
          <DialogDescription>Any Unity, Unreal, Godot, Blender, or creative folder works. Existing <code>.pipelineos</code> settings (for example from a teammate's commit) are used as-is; otherwise they're created. Tip: you can also drag a folder onto the window.</DialogDescription>
        </DialogHeader>
        {found ? <div className="rounded-lg border border-primary/30 bg-primary/5 p-3"><DiscoveredProjects compact onAdded={onAdded} /></div> : null}
        <form className="space-y-3" onSubmit={(event) => void submit(event)}>
          <label className="block space-y-1 text-sm"><span className="font-medium">Project folder</span>
            <PathInput ariaLabel="Project path" required directory placeholder={platformExamples().projectFolder} value={path} onChange={setPath} className="w-full" />
          </label>
          <label className="block space-y-1 text-sm"><span className="font-medium">Display name <span className="font-normal text-muted-foreground">(optional)</span></span>
            <Input aria-label="Project name" placeholder="Defaults to the folder name" value={name} onChange={(event) => setName(event.target.value)} />
          </label>
          <div className="flex justify-end"><Button type="submit" disabled={busy || !path.trim()}><FolderPlus size={15} /> {busy ? "Adding…" : "Add project"}</Button></div>
        </form>
        {!found ? <div className="border-t border-border pt-4"><DiscoveredProjects compact onAdded={onAdded} /></div> : null}
        {canScanDrives ? <div className="space-y-2 border-t border-border pt-4">
          <div className="text-sm font-medium">Deep scan drives</div>
          <p className="text-xs text-muted-foreground">Scans {driveSummary} for folders that already have Pipeline OS settings, e.g. after reinstalling or on a second drive.</p>
          {scanning ? (
            <div className="flex items-center justify-between gap-3 rounded-lg border bg-secondary/40 px-3 py-2 text-sm">
              <span className="flex min-w-0 items-center gap-2"><RefreshCw size={14} className="shrink-0 animate-spin" /><span className="truncate">{scanLabel || "Scanning…"}</span></span>
              <Button variant="outline" size="sm" onClick={onCancelScan}>Cancel</Button>
            </div>
          ) : <Button variant="outline" onClick={onScan}><HardDrive size={15} /> Scan drives</Button>}
        </div> : null}
      </DialogContent>
    </Dialog>
  );
}

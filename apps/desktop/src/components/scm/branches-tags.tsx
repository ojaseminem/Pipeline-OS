// Branches and tags: rename, delete (guarded: never the current or default
// branch, never unmerged work without asking, and remote deletion only when
// your GitHub role and branch protection allow it), and release tags.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Cloud, GitBranch, MoreHorizontal, Pencil, Tag, Trash2, Upload } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { desktopApi, scmApi, type BranchInfo } from "../../bridge";
import { confirmAction } from "../../lib/confirm";
import { formatLastOpened } from "../../lib/format";
import { attempt, errorText } from "../../lib/scm";

export function BranchesAndTags({ root, current }: { root: string; current: string | undefined }) {
  const queryClient = useQueryClient();
  const branches = useQuery({ queryKey: ["git-branches", root], queryFn: () => desktopApi.gitBranches(root), retry: false });
  const tags = useQuery({ queryKey: ["git-tags", root], queryFn: () => scmApi.tags(root), retry: false });
  const [renaming, setRenaming] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const [tagName, setTagName] = useState("");
  const [tagMessage, setTagMessage] = useState("");
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["git-branches", root] });
    void queryClient.invalidateQueries({ queryKey: ["git-tags", root] });
  };
  const local = (branches.data ?? []).filter((branch) => !branch.remote);
  const remote = (branches.data ?? []).filter((branch) => branch.remote);

  async function deleteLocal(branch: BranchInfo) {
    if (!(await confirmAction({ title: `Delete local branch “${branch.name}”?`, description: "Only your local copy is removed. Pipeline OS refuses if it has commits that exist nowhere else.", confirmLabel: "Delete branch", destructive: true }))) return;
    try {
      await scmApi.deleteBranch(root, branch.name, false, false);
      toast.success(`Deleted ${branch.name}`);
    } catch (error) {
      const message = errorText(error);
      if (/unmerged|unpushed|not merged|exist nowhere|only on this/i.test(message) && await confirmAction({ title: "This branch has work that isn't anywhere else", description: `${message}\n\nDelete it anyway? Those commits will be lost.`, confirmLabel: "Delete anyway", destructive: true })) {
        await attempt("Deleting the branch", () => scmApi.deleteBranch(root, branch.name, false, true), `Deleted ${branch.name}`);
      } else {
        toast.error("Couldn't delete the branch", { description: message });
      }
    }
    refresh();
  }

  async function deleteRemote(branch: BranchInfo) {
    const decision = await scmApi.canDeleteRemote(root, branch.name).catch((error) => ({ allowed: false, reason: errorText(error) }));
    if (!decision.allowed) { toast.error(`You can't delete ${branch.remote}/${branch.name}`, { description: decision.reason }); return; }
    if (!(await confirmAction({ title: `Delete “${branch.name}” on ${branch.remote ?? "the remote"}?`, description: `${decision.reason}\n\nThis removes it for everyone.`, confirmLabel: "Delete for everyone", destructive: true }))) return;
    await attempt("Deleting the remote branch", () => scmApi.deleteBranch(root, branch.name, true, false), `Deleted ${branch.name} on ${branch.remote ?? "the remote"}`);
    refresh();
  }

  async function rename(oldName: string) {
    const target = newName.trim();
    if (!target || target === oldName) { setRenaming(null); return; }
    const done = await attempt("Renaming", () => scmApi.renameBranch(root, oldName, target), `Renamed to ${target}`);
    if (done) { setRenaming(null); refresh(); }
  }

  async function createTag() {
    const name = tagName.trim();
    if (!name) return;
    const done = await attempt("Creating the tag", () => scmApi.createTag(root, name, tagMessage.trim() || undefined), `Tagged ${name}`);
    if (done) {
      setTagName(""); setTagMessage(""); refresh();
      toast.message(`Push ${name} to share it?`, { action: { label: "Push", onClick: () => void attempt("Pushing the tag", () => scmApi.pushTag(root, name), `Pushed ${name}`) } });
    }
  }

  return (
    <div className="grid gap-4 xl:grid-cols-2">
      <Card><CardContent className="space-y-2 p-4">
        <h3 className="flex items-center gap-2 text-sm font-semibold"><GitBranch size={15} className="text-primary" /> Branches</h3>
        {branches.isError ? <p className="text-sm text-destructive">{errorText(branches.error)}</p> : null}
        <ul className="divide-y divide-border rounded-lg border border-border">
          {[...local, ...remote].map((branch) => (
            <li key={`${branch.remote ?? ""}/${branch.name}`} className="flex items-center gap-2 px-3 py-2 text-sm">
              {branch.remote ? <Cloud size={13} className="shrink-0 text-muted-foreground" /> : <GitBranch size={13} className="shrink-0 text-muted-foreground" />}
              {renaming === branch.name && !branch.remote ? (
                <Input autoFocus aria-label="New branch name" value={newName} onChange={(event) => setNewName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void rename(branch.name); if (event.key === "Escape") setRenaming(null); }} onBlur={() => void rename(branch.name)} className="h-7" />
              ) : <span className="min-w-0 flex-1 truncate">{branch.name}</span>}
              {branch.remote ? <Badge variant="outline" className="text-[10px]">{branch.remote}</Badge> : null}
              {branch.name === current && !branch.remote ? <Badge variant="secondary" className="text-[10px]">current</Badge> : null}
              <DropdownMenu>
                <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-7 w-7" aria-label={`Actions for ${branch.name}`}><MoreHorizontal size={14} /></Button></DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {!branch.remote ? <DropdownMenuItem onClick={() => { setRenaming(branch.name); setNewName(branch.name); }}><Pencil size={13} /> Rename</DropdownMenuItem> : null}
                  <DropdownMenuSeparator />
                  {branch.remote
                    ? <DropdownMenuItem variant="destructive" onClick={() => void deleteRemote(branch)}><Trash2 size={13} /> Delete on {branch.remote}…</DropdownMenuItem>
                    : <DropdownMenuItem variant="destructive" disabled={branch.name === current} onClick={() => void deleteLocal(branch)}><Trash2 size={13} /> Delete local branch…</DropdownMenuItem>}
                </DropdownMenuContent>
              </DropdownMenu>
            </li>
          ))}
        </ul>
        <p className="text-xs text-muted-foreground">The current and default branches can't be deleted. Deleting a branch on GitHub needs write access (admin or maintain for protected branches).</p>
      </CardContent></Card>
      <Card><CardContent className="space-y-2 p-4">
        <h3 className="flex items-center gap-2 text-sm font-semibold"><Tag size={15} className="text-primary" /> Tags</h3>
        <div className="flex flex-wrap gap-2">
          <Input aria-label="Tag name" value={tagName} onChange={(event) => setTagName(event.target.value)} placeholder="v1.2.0" className="h-8 w-32" />
          <Input aria-label="Tag message" value={tagMessage} onChange={(event) => setTagMessage(event.target.value)} placeholder="Release notes (optional)" className="h-8 min-w-40 flex-1" />
          <Button size="sm" disabled={!tagName.trim()} onClick={() => void createTag()}><Tag size={13} /> Tag {current ? "HEAD" : ""}</Button>
        </div>
        <ul className="divide-y divide-border rounded-lg border border-border">
          {(tags.data ?? []).map((tag) => (
            <li key={tag.name} className="flex items-center gap-2 px-3 py-2 text-sm">
              <Tag size={13} className="shrink-0 text-muted-foreground" />
              <span className="font-medium">{tag.name}</span>
              <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{tag.subject} · {tag.target} · {formatLastOpened(tag.date)}</span>
              <Button variant="ghost" size="icon" className="h-7 w-7" aria-label={`Push ${tag.name}`} title="Push tag" onClick={() => void attempt("Pushing the tag", () => scmApi.pushTag(root, tag.name), `Pushed ${tag.name}`)}><Upload size={13} /></Button>
            </li>
          ))}
          {tags.data && !tags.data.length ? <li className="px-3 py-4 text-center text-sm text-muted-foreground">No tags yet. Tag releases and milestone builds.</li> : null}
        </ul>
      </CardContent></Card>
    </div>
  );
}

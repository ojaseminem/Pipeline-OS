// Pull requests without leaving Pipeline OS: the current branch's PR up top
// (or one click to open it), every open PR with review, checks, and merge
// state, and safe merging that refuses drafts, conflicts, and unknown state.
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ChevronDown, ChevronRight, CircleDot, ExternalLink, GitMerge, GitPullRequest, GitPullRequestDraft, MessageSquare, Plus, UserPlus, XCircle } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { openExternal, scmApi, type PullRequest } from "../../bridge";
import { confirmAction } from "../../lib/confirm";
import { formatLastOpened } from "../../lib/format";
import { BranchChecks } from "./branch-checks";
import { GithubConnect } from "./github-connect";
import { useGithubStatus } from "../../lib/scm";
import { RollupIcon } from "./shared";
import { attempt, errorText } from "../../lib/scm";

const REVIEW_LABEL = { approved: "Approved", changes_requested: "Changes requested", review_required: "Review required" } as const;

function mergeBlocker(pr: PullRequest): string | null {
  if (pr.state !== "open") return `This pull request is ${pr.state}.`;
  if (pr.draft) return "Drafts can't be merged. Mark it ready for review first.";
  if (pr.mergeable === false || pr.mergeableState === "dirty") return "It has merge conflicts with the base branch.";
  if (pr.mergeable === null) return "GitHub is still working out whether it can merge. Try again in a moment.";
  if (pr.mergeableState === "blocked") return "Branch protection blocks merging (required reviews or checks).";
  if (pr.mergeableState === "behind") return "The branch is behind the base branch. Update it first.";
  return null;
}

function CreatePrDialog({ root, open, onOpenChange, branch, defaultBranch, onCreated }: { root: string; open: boolean; onOpenChange: (open: boolean) => void; branch: string; defaultBranch: string; onCreated: () => void }) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [base, setBase] = useState(defaultBranch);
  const [draft, setDraft] = useState(false);
  const [busy, setBusy] = useState(false);
  const suggested = branch.replace(/^(feature|feat|fix|bugfix|chore)[/-]/i, "").replace(/[-_/]+/g, " ").replace(/^\w/, (c) => c.toUpperCase());

  async function submit() {
    setBusy(true);
    const pr = await attempt("Opening the pull request", () => scmApi.createPr(root, { title: title.trim() || suggested, body, base: base.trim() || undefined, draft }), (value) => `Opened #${value.number}`);
    setBusy(false);
    if (pr) { onOpenChange(false); setTitle(""); setBody(""); onCreated(); }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Open a pull request</DialogTitle>
          <DialogDescription>From <code>{branch}</code> into <code>{base || defaultBranch}</code>. The branch must be published first.</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <label className="block space-y-1 text-sm"><span className="font-medium">Title</span><Input autoFocus value={title} placeholder={suggested} onChange={(event) => setTitle(event.target.value)} /></label>
          <label className="block space-y-1 text-sm"><span className="font-medium">Description <span className="font-normal text-muted-foreground">(Markdown)</span></span><Textarea rows={7} value={body} onChange={(event) => setBody(event.target.value)} placeholder={"What changed and why.\n\nHow to test it:\n- "} /></label>
          <div className="flex flex-wrap items-center gap-4 text-sm">
            <label className="flex items-center gap-2">Into <Input aria-label="Base branch" value={base} onChange={(event) => setBase(event.target.value)} className="h-8 w-40" /></label>
            <label className="flex items-center gap-2"><input type="checkbox" className="size-4 accent-[var(--primary)]" checked={draft} onChange={(event) => setDraft(event.target.checked)} /> Open as draft</label>
          </div>
        </div>
        <DialogFooter><Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button><Button disabled={busy} onClick={() => void submit()}><GitPullRequest size={14} /> {busy ? "Opening…" : draft ? "Open draft" : "Open pull request"}</Button></DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function PrRow({ root, pr, highlight, methods, onChanged }: { root: string; pr: PullRequest; highlight?: boolean; methods: Array<"merge" | "squash" | "rebase">; onChanged: () => void }) {
  const [open, setOpen] = useState(Boolean(highlight));
  const [comment, setComment] = useState("");
  const [reviewers, setReviewers] = useState("");
  const blocker = mergeBlocker(pr);
  const Icon = pr.draft ? GitPullRequestDraft : GitPullRequest;

  async function merge(method: "merge" | "squash" | "rebase") {
    const verb = method === "squash" ? "Squash and merge" : method === "rebase" ? "Rebase and merge" : "Merge";
    if (!(await confirmAction({ title: `${verb} #${pr.number}?`, description: `“${pr.title}” goes into ${pr.base}.`, confirmLabel: verb }))) return;
    const result = await attempt("Merging", () => scmApi.mergePr(root, pr.number, method), (value) => value.message || `Merged #${pr.number}`);
    if (result) onChanged();
  }

  return (
    <li className={cn("rounded-lg border", highlight ? "border-primary/40 bg-primary/5" : "border-border")}>
      <div className="flex items-start gap-2 p-3">
        <button aria-label={open ? "Collapse" : "Expand"} className="mt-0.5" onClick={() => setOpen(!open)}>{open ? <ChevronDown size={15} /> : <ChevronRight size={15} />}</button>
        <Icon size={16} className={cn("mt-0.5 shrink-0", pr.draft ? "text-muted-foreground" : "text-success")} />
        <div className="min-w-0 flex-1">
          <button className="text-left text-sm font-medium hover:text-primary" onClick={() => setOpen(!open)}>{pr.title} <span className="font-normal text-muted-foreground">#{pr.number}</span></button>
          <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
            <span>{pr.author}</span><span>·</span><code>{pr.head}</code><span>→</span><code>{pr.base}</code><span>·</span><span>{formatLastOpened(pr.updatedAt)}</span>
            {pr.reviewDecision ? <Badge variant="outline" className={cn("gap-1", pr.reviewDecision === "approved" ? "text-success" : pr.reviewDecision === "changes_requested" ? "text-destructive" : "")}>{pr.reviewDecision === "approved" ? <CheckCircle2 size={11} /> : pr.reviewDecision === "changes_requested" ? <XCircle size={11} /> : <CircleDot size={11} />}{REVIEW_LABEL[pr.reviewDecision]}</Badge> : null}
            {pr.checks ? <Badge variant="outline" className="gap-1"><RollupIcon rollup={pr.checks} size={11} /> Checks</Badge> : null}
            {pr.mergeable === false ? <Badge variant="outline" className="text-destructive">Conflicts</Badge> : null}
            {pr.changedFiles ? <span className="tabular-nums"><span className="text-success">+{pr.additions}</span> <span className="text-destructive">−{pr.deletions}</span> · {pr.changedFiles} files</span> : null}
          </div>
        </div>
        <Button variant="ghost" size="icon" className="h-7 w-7 shrink-0" aria-label="Open on GitHub" onClick={() => void openExternal(pr.url)}><ExternalLink size={13} /></Button>
      </div>
      {open ? (
        <div className="space-y-3 border-t border-border p-3">
          <BranchChecks root={root} branch={pr.headSha || pr.head} compact />
          {pr.requestedReviewers.length ? <p className="text-xs text-muted-foreground">Waiting on {pr.requestedReviewers.join(", ")}</p> : null}
          <div className="flex flex-wrap items-center gap-2">
            {pr.state === "open" ? (
              blocker ? <p className="flex-1 text-xs text-warning">{blocker}</p> : (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild><Button size="sm"><GitMerge size={14} /> Merge <ChevronDown size={12} /></Button></DropdownMenuTrigger>
                  <DropdownMenuContent align="start">
                    {methods.map((method) => <DropdownMenuItem key={method} onClick={() => void merge(method)}>{method === "squash" ? "Squash and merge" : method === "rebase" ? "Rebase and merge" : "Create a merge commit"}</DropdownMenuItem>)}
                  </DropdownMenuContent>
                </DropdownMenu>
              )
            ) : null}
            {pr.state === "open" ? <Button variant="outline" size="sm" onClick={() => void attempt("Updating", () => scmApi.updatePr(root, pr.number, { draft: !pr.draft }), pr.draft ? "Marked ready for review" : "Converted to draft").then(onChanged)}>{pr.draft ? "Ready for review" : "Convert to draft"}</Button> : null}
            {pr.state === "open" ? <Button variant="ghost" size="sm" className="text-destructive hover:text-destructive" onClick={() => void (async () => { if (await confirmAction({ title: `Close #${pr.number} without merging?`, confirmLabel: "Close pull request", destructive: true })) await attempt("Closing", () => scmApi.closePr(root, pr.number), `Closed #${pr.number}`).then(onChanged); })()}>Close</Button> : null}
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="flex gap-1.5"><Input aria-label="Reviewers" value={reviewers} onChange={(event) => setReviewers(event.target.value)} placeholder="Reviewers: alice, team:art" className="h-8" /><Button variant="outline" size="sm" disabled={!reviewers.trim()} onClick={() => void attempt("Requesting review", () => scmApi.requestReviewers(root, pr.number, reviewers.split(",").map((value) => value.trim()).filter(Boolean)), "Review requested").then((value) => { if (value) { setReviewers(""); onChanged(); } })}><UserPlus size={13} /></Button></div>
            <div className="flex gap-1.5"><Input aria-label="Comment" value={comment} onChange={(event) => setComment(event.target.value)} placeholder="Leave a comment" className="h-8" /><Button variant="outline" size="sm" disabled={!comment.trim()} onClick={() => void attempt("Commenting", () => scmApi.commentPr(root, pr.number, comment.trim()), "Comment posted").then((value) => { if (value) setComment(""); })}><MessageSquare size={13} /></Button></div>
          </div>
        </div>
      ) : null}
    </li>
  );
}

export function PullRequests({ root, branch, published }: { root: string; branch: string | undefined; published: boolean }) {
  const queryClient = useQueryClient();
  const status = useGithubStatus(root);
  const connected = Boolean(status.data?.viewer);
  const prs = useQuery({ queryKey: ["github-prs", root], queryFn: () => scmApi.prs(root), enabled: Boolean(status.data?.repo), retry: false, refetchInterval: 60_000 });
  const [creating, setCreating] = useState(false);
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["github-prs", root] });
    void queryClient.invalidateQueries({ queryKey: ["branch-checks", root] });
  };
  const info = status.data?.repoInfo;
  const methods = ([info?.allowSquashMerge !== false ? "squash" : null, info?.allowMergeCommit !== false ? "merge" : null, info?.allowRebaseMerge ? "rebase" : null].filter(Boolean) as Array<"merge" | "squash" | "rebase">);
  const defaultBranch = info?.defaultBranch ?? "main";
  const mine = (prs.data ?? []).find((pr) => pr.head === branch);
  const others = (prs.data ?? []).filter((pr) => pr !== mine);

  return (
    <div className="space-y-4">
      <GithubConnect root={root} />
      {status.data?.repo ? (
        <>
          {branch && branch !== defaultBranch && !mine ? (
            <div className="flex flex-wrap items-center gap-3 rounded-lg border border-dashed border-border p-3 text-sm">
              <GitPullRequest size={16} className="text-primary" />
              <span className="flex-1">{published ? <>Ready to propose <code>{branch}</code>?</> : <>Publish <code>{branch}</code> to open a pull request for it.</>}</span>
              <Button size="sm" disabled={!connected || !published} onClick={() => setCreating(true)}><Plus size={14} /> New pull request</Button>
            </div>
          ) : null}
          {prs.isError ? <p className="text-sm text-destructive">{errorText(prs.error)}</p> : null}
          {prs.isLoading ? <p className="text-sm text-muted-foreground">Loading pull requests…</p> : null}
          <ul className="space-y-2">
            {mine ? <PrRow key={mine.number} root={root} pr={mine} highlight methods={methods} onChanged={refresh} /> : null}
            {others.map((pr) => <PrRow key={pr.number} root={root} pr={pr} methods={methods} onChanged={refresh} />)}
          </ul>
          {prs.data && !prs.data.length ? <p className="text-sm text-muted-foreground">No open pull requests.</p> : null}
          {branch ? <CreatePrDialog root={root} open={creating} onOpenChange={setCreating} branch={branch} defaultBranch={defaultBranch} onCreated={refresh} /> : null}
        </>
      ) : null}
    </div>
  );
}

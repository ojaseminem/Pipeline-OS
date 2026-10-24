// When a branch was last updated (its latest commit), shown next to branch
// names so stale branches are easy to spot when switching or merging.
import type { BranchInfo } from "../../bridge";
import { timeAgo } from "../../lib/format";

export function BranchUpdated({ branch }: { branch: BranchInfo }) {
  if (!branch.updatedAt) return null;
  const exact = new Date(branch.updatedAt * 1000).toLocaleString();
  return (
    <span className="shrink-0 text-xs tabular-nums text-muted-foreground" title={`Last commit ${exact}`}>
      {timeAgo(branch.updatedAt)}
    </span>
  );
}

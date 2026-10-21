import type { ReviewStatus } from "../bridge";

export const REVIEW_META: Record<ReviewStatus, { label: string; className: string }> = {
  none: { label: "No status", className: "bg-secondary text-muted-foreground" },
  wip: { label: "WIP", className: "bg-[color:oklch(0.62_0.12_250/0.18)] text-[color:oklch(0.75_0.12_250)]" },
  review: { label: "In review", className: "bg-warning/15 text-warning" },
  approved: { label: "Approved", className: "bg-success/15 text-success" },
  rejected: { label: "Changes needed", className: "bg-destructive/15 text-destructive" },
};

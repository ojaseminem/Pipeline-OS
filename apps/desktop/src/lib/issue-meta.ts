import { Bug, CircleDot, Lightbulb, Palette, PenTool, SignalHigh, SignalLow, SignalMedium, TriangleAlert } from "lucide-react";
import type { IssueKind, IssuePriority, IssueStatus } from "../bridge";

export const STATUS_COLUMNS: Array<{ id: IssueStatus; label: string }> = [
  { id: "backlog", label: "Backlog" },
  { id: "todo", label: "To do" },
  { id: "in-progress", label: "In progress" },
  { id: "review", label: "Review" },
  { id: "done", label: "Done" },
];

export const KIND_META: Record<IssueKind, { label: string; icon: typeof Bug; className: string }> = {
  task: { label: "Task", icon: CircleDot, className: "text-muted-foreground" },
  bug: { label: "Bug", icon: Bug, className: "text-destructive" },
  feature: { label: "Feature", icon: Lightbulb, className: "text-primary" },
  art: { label: "Art", icon: Palette, className: "text-success" },
  design: { label: "Design", icon: PenTool, className: "text-warning" },
};

export const PRIORITY_META: Record<IssuePriority, { label: string; icon: typeof Bug | null; className: string }> = {
  none: { label: "No priority", icon: null, className: "" },
  low: { label: "Low", icon: SignalLow, className: "text-muted-foreground" },
  medium: { label: "Medium", icon: SignalMedium, className: "text-foreground" },
  high: { label: "High", icon: SignalHigh, className: "text-warning" },
  urgent: { label: "Urgent", icon: TriangleAlert, className: "text-destructive" },
};

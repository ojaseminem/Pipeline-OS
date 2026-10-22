// Small shared pieces for the source control screens.
import { CheckCircle2, CircleDashed, CircleSlash, Loader2, XCircle } from "lucide-react";
import { cn } from "@/lib/utils";
import type { CheckRollup } from "../../bridge";



export function RollupIcon({ rollup, size = 14, className }: { rollup: CheckRollup | null | undefined; size?: number; className?: string }) {
  if (rollup === "success") return <CheckCircle2 size={size} className={cn("text-success", className)} aria-label="Checks passed" />;
  if (rollup === "failure") return <XCircle size={size} className={cn("text-destructive", className)} aria-label="Checks failed" />;
  if (rollup === "pending") return <Loader2 size={size} className={cn("animate-spin text-warning", className)} aria-label="Checks running" />;
  if (rollup === "neutral") return <CircleSlash size={size} className={cn("text-muted-foreground", className)} aria-label="Checks neutral" />;
  return <CircleDashed size={size} className={cn("text-muted-foreground", className)} aria-label="No checks" />;
}


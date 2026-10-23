import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/** A friendly empty state: what's missing, why it matters, and the fix. */
export function EmptyState({ icon: Icon, title, description, action, className, compact }: {
  icon: LucideIcon;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  className?: string;
  compact?: boolean;
}) {
  return (
    <div className={cn("flex flex-col items-center justify-center text-center", compact ? "gap-1.5 px-4 py-6" : "gap-2 px-6 py-10", className)}>
      <span className={cn("flex items-center justify-center rounded-xl bg-secondary text-primary", compact ? "h-9 w-9" : "h-11 w-11")}><Icon size={compact ? 18 : 22} aria-hidden /></span>
      <p className={cn("font-medium", compact ? "text-sm" : "text-base")}>{title}</p>
      {description ? <p className="max-w-md text-sm text-muted-foreground">{description}</p> : null}
      {action ? <div className="mt-2 flex flex-wrap items-center justify-center gap-2">{action}</div> : null}
    </div>
  );
}

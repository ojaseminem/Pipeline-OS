import { Film, Flame, Hammer, Package, TerminalSquare, TestTube2 } from "lucide-react";
import type { TaskKind } from "../bridge";

export function formatDuration(ms: number | null | undefined): string {
  if (ms == null) return "";
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export const KIND_ICONS: Record<TaskKind, typeof Hammer> = { build: Hammer, test: TestTube2, render: Film, export: Package, cook: Flame, script: TerminalSquare };

export function cliRunCommand(path: string, taskId: string): string {
  return `vantadeck tasks run "${path}" ${taskId} --yes`;
}

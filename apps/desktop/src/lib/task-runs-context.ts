import { createContext, useContext } from "react";
import type { TaskRun } from "../bridge";

export type TaskRunsContextValue = {
  runs: TaskRun[];
  active: TaskRun[];
  logs: Record<string, string[]>;
  start: (path: string, taskId: string) => Promise<TaskRun | null>;
  cancel: (runId: string) => Promise<void>;
  openLog: (runId: string) => void;
  refresh: () => void;
};

export const TaskRunsContext = createContext<TaskRunsContextValue | null>(null);

export function useTaskRuns(): TaskRunsContextValue {
  const value = useContext(TaskRunsContext);
  if (!value) throw new Error("useTaskRuns must be used inside <TaskRunsProvider>");
  return value;
}

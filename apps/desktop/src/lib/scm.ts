// Source control helpers shared by the scm components.
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { scmApi, type CheckRollup, type GithubStatus } from "../bridge";

export function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Runs an action with a success toast, or an error toast with the reason. */
export async function attempt<T>(label: string, action: () => Promise<T>, success?: string | ((value: T) => string)): Promise<T | undefined> {
  try {
    const value = await action();
    if (success) toast.success(typeof success === "function" ? success(value) : success);
    return value;
  } catch (error) {
    toast.error(`${label} failed`, { description: errorText(error) });
    return undefined;
  }
}

export function conclusionRollup(status: string, conclusion: string | null): CheckRollup {
  if (status !== "completed") return "pending";
  if (conclusion === "success") return "success";
  if (conclusion === "failure" || conclusion === "timed_out" || conclusion === "cancelled" || conclusion === "action_required" || conclusion === "startup_failure" || conclusion === "error") return "failure";
  return "neutral";
}

export function useGithubStatus(root: string, enabled = true) {
  return useQuery({ queryKey: ["github-status", root], queryFn: () => scmApi.githubStatus(root), enabled, staleTime: 5 * 60_000, retry: false });
}

export function roleOf(status: GithubStatus | undefined): string | null {
  const permissions = status?.repoInfo?.permissions;
  if (!permissions) return null;
  if (permissions.admin) return "Admin";
  if (permissions.maintain) return "Maintainer";
  if (permissions.push) return "Write";
  if (permissions.triage) return "Triage";
  return "Read";
}

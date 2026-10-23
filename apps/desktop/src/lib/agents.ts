import type { AgentId } from "../bridge";

/** AI coding agents Pipeline OS can start in a project terminal. */
export const AGENTS: Array<{ id: AgentId; name: string; hint: string }> = [
  { id: "claude", name: "Claude Code", hint: "claude" },
  { id: "codex", name: "Codex CLI", hint: "codex" },
  { id: "gemini", name: "Gemini CLI", hint: "gemini" },
  { id: "aider", name: "Aider", hint: "aider" },
];

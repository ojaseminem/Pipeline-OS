// GitHub connection for a repository: who you're signed in as, your role in
// the repository, and a token prompt (kept in the OS keychain) when needed.
import { useQueryClient } from "@tanstack/react-query";
import { ExternalLink, Github, KeyRound, LogOut, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { openExternal, scmApi } from "../../bridge";
import { attempt, roleOf, useGithubStatus } from "../../lib/scm";



const TOKEN_URL = "https://github.com/settings/tokens/new?scopes=repo,workflow,read:org&description=Pipeline%20OS";

export function GithubConnect({ root, compact = false }: { root: string; compact?: boolean }) {
  const queryClient = useQueryClient();
  const status = useGithubStatus(root);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const data = status.data;
  const host = data?.repo?.host ?? "github.com";
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["github-status", root] });

  async function connect() {
    if (!token.trim()) return;
    setBusy(true);
    const viewer = await attempt("Connecting GitHub", () => scmApi.githubSetToken(host, token.trim()), (value) => `Connected as ${value.login}`);
    setBusy(false);
    if (viewer) { setToken(""); await refresh(); void queryClient.invalidateQueries({ queryKey: ["github-prs", root] }); }
  }

  if (status.isLoading) return <p className="text-sm text-muted-foreground">Checking GitHub…</p>;
  if (!data?.repo) {
    return <p className="text-sm text-muted-foreground">{data?.remoteUrl ? `This repository's remote (${data.remoteUrl}) isn't on GitHub. Pull requests and checks work with GitHub and GitHub Enterprise.` : "Publish this repository to GitHub to open pull requests and see checks here."}</p>;
  }
  if (data.viewer) {
    const role = roleOf(data);
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm">
        {data.viewer.avatarUrl ? <img src={data.viewer.avatarUrl} alt="" className="size-6 rounded-full" /> : <Github size={18} />}
        <span>Signed in as <strong>{data.viewer.login}</strong></span>
        {role ? <Badge variant="secondary" className="gap-1"><ShieldCheck size={11} /> {role}</Badge> : null}
        <span className="text-xs text-muted-foreground">on {data.repo.owner}/{data.repo.repo}{data.tokenSource === "ghCli" ? " · via GitHub CLI" : data.tokenSource === "env" ? " · via environment" : ""}</span>
        {!compact ? (
          <span className="ml-auto flex gap-1.5">
            <Button variant="ghost" size="sm" onClick={() => void openExternal(data.repo!.htmlUrl)}><ExternalLink size={13} /> Open on GitHub</Button>
            {data.tokenSource === "keychain" ? <Button variant="ghost" size="sm" onClick={() => void attempt("Signing out", () => scmApi.githubClearToken(host), "Signed out of GitHub").then(refresh)}><LogOut size={13} /> Sign out</Button> : null}
          </span>
        ) : null}
      </div>
    );
  }
  return (
    <div className="space-y-2">
      <p className="text-sm">Connect GitHub to open and merge pull requests, see checks, and re-run failed builds for <strong>{data.repo.owner}/{data.repo.repo}</strong>.</p>
      {data.error ? <p className="text-xs text-destructive">{data.error}</p> : null}
      <div className="flex flex-wrap gap-2">
        <Input type="password" aria-label="GitHub token" placeholder="Paste a GitHub token (ghp_… or github_pat_…)" value={token} onChange={(event) => setToken(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void connect(); }} className="min-w-64 flex-1" />
        <Button disabled={busy || !token.trim()} onClick={() => void connect()}><KeyRound size={14} /> {busy ? "Checking…" : "Connect"}</Button>
      </div>
      <p className="text-xs text-muted-foreground">
        <button className="text-primary hover:underline" onClick={() => void openExternal(host === "github.com" ? TOKEN_URL : `https://${host}/settings/tokens/new?scopes=repo,workflow`)}>Create a token</button> with the <code>repo</code> and <code>workflow</code> scopes. It's stored in your OS keychain, never in the project. Already use the GitHub CLI? Run <code>gh auth login</code> and Pipeline OS picks it up automatically.
      </p>
    </div>
  );
}

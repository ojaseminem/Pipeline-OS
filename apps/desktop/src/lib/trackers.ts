import type { TrackerKind } from "../bridge";

export type TrackerMeta = {
  kind: TrackerKind;
  name: string;
  needsUrl: boolean;
  urlPlaceholder: string;
  projectLabel: string;
  projectPlaceholder: string;
  tokenLabel: string;
  tokenPlaceholder: string;
  tokenOptional?: boolean;
  tokenHelpUrl: string;
};

export const TRACKERS: TrackerMeta[] = [
  { kind: "github", name: "GitHub", needsUrl: false, urlPlaceholder: "https://github.example.com/api/v3", projectLabel: "Repository", projectPlaceholder: "owner/repo", tokenLabel: "Personal access token", tokenPlaceholder: "github_pat_…", tokenOptional: true, tokenHelpUrl: "https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens" },
  { kind: "gitea", name: "Gitea / Forgejo", needsUrl: true, urlPlaceholder: "https://git.studio.local", projectLabel: "Repository", projectPlaceholder: "owner/repo", tokenLabel: "Access token", tokenPlaceholder: "token from Settings → Applications", tokenOptional: true, tokenHelpUrl: "https://forgejo.org/docs/latest/user/api-usage/" },
  { kind: "jira", name: "Jira", needsUrl: true, urlPlaceholder: "https://studio.atlassian.net", projectLabel: "Project key", projectPlaceholder: "VOID", tokenLabel: "API token", tokenPlaceholder: "Cloud: you@studio.com:api-token · Data Center: personal access token", tokenHelpUrl: "https://support.atlassian.com/atlassian-account/docs/manage-api-tokens-for-your-atlassian-account/" },
  { kind: "linear", name: "Linear", needsUrl: false, urlPlaceholder: "", projectLabel: "Team key", projectPlaceholder: "ENG", tokenLabel: "API key", tokenPlaceholder: "lin_api_…", tokenHelpUrl: "https://linear.app/docs/api-and-webhooks" },
  { kind: "plane", name: "Plane", needsUrl: true, urlPlaceholder: "https://plane.studio.local", projectLabel: "Workspace / project id", projectPlaceholder: "studio/2f9c…", tokenLabel: "API key", tokenPlaceholder: "plane_api_…", tokenHelpUrl: "https://developers.plane.so/api-reference/introduction" },
  { kind: "clickup", name: "ClickUp", needsUrl: false, urlPlaceholder: "", projectLabel: "List id", projectPlaceholder: "901234567", tokenLabel: "API token", tokenPlaceholder: "pk_…", tokenHelpUrl: "https://developer.clickup.com/docs/authentication" },
];

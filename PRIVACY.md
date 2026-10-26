# Privacy

Pipeline OS is local-first. It needs no account and has no telemetry, analytics or
advertising. Projects, preferences, activity, run history and caches stay on your
computer. Pipeline OS does not sell or share usage data.

## When the network is used

Only these features contact a server, and only the one named:

| Feature | Destination | When |
| --- | --- | --- |
| Update check | GitHub Releases for this repository | At launch when *Automatically check for updates* is on (the default; turn it off in Settings), or when you click *Check for updates* |
| Git fetch / pull / push | Your project's Git remote | When you sync, using your existing Git credentials |
| Pull requests and CI checks | GitHub API | When you open those panels in a GitHub-hosted project (an optional token adds private repos and actions) |
| Tracker sync | The tracker you connect (GitHub Issues, Jira, Linear, Plane, Gitea/Forgejo, ClickUp) | When you sync the board |
| Links | Your browser | When you open a link |

Everything else, including app detection, projects, launching, pipeline tasks, the
board, the asset library and the MCP server for local AI apps, works offline.

## Credentials

Tokens for GitHub and trackers are stored in your operating system's keychain, never
in Pipeline OS's database or in project files. Git uses the credentials already
configured for it.

## AI apps

The built-in MCP server only runs when an AI app you configured starts it on your
computer. Pipeline OS sends nothing to an AI provider itself; what an AI app does with
the data it reads is governed by that app.

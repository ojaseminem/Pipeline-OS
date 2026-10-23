# ADR 0001: Keep Tauri for the desktop shell

- **Status:** Accepted (October 2026)
- **Context:** The product must feel fast and polished for years, ship to
  Windows first (then macOS/Linux), and share one Rust core between the
  desktop app, the CLI and the MCP server. We reconsidered the shell.

## Options

| Option | For | Against |
| --- | --- | --- |
| **Tauri 2 (current)** | Rust core called in-process; ~10 MB installer; low idle memory; capability-based IPC permissions; signed auto-updater; sidecars (we ship the CLI this way) | Rendering differs per OS webview (WebView2 on Windows is Chromium, so the primary target is consistent; WebKitGTK on Linux lags) |
| Electron | One Chromium everywhere; huge ecosystem | ~150 MB installs and 200–400 MB RAM for a launcher meant to sit in the tray; Rust core becomes a separate process or N-API addon |
| Qt (C++/PySide) | Native widgets; familiar to DCC developers (Maya/Houdini use Qt) | Full UI rewrite; slower UI iteration; licensing considerations; harder to hire for |
| Flutter | Consistent rendering, good performance | Full UI rewrite; Rust interop via FFI; desktop ecosystem thinner |
| egui / Slint / Iced (pure Rust) | One language, tiny binaries | Accessibility, text input, theming and polish are far behind the web stack today |

## Decision

Keep Tauri. The things that make an app *feel* excellent (instant navigation,
calm motion, keyboard flow, density, typography) are frontend work that the
web stack is best at. Tauri keeps the footprint and security model that a
background launcher needs. The core stays UI-agnostic (`crates/*`), so a future
shell change would only replace `apps/desktop`.

## Consequences

- Invest in frontend architecture (per-screen modules, a router, generated IPC
  types), performance budgets, and visual regression tests on WebView2.
- Long operations stream from Rust with batched events (already done for
  pipeline tasks) so the webview never floods.
- Revisit if WebView2 regressions or Linux WebKitGTK gaps block a release.

# Headless Mode

The Pipeline OS command line (`vantadeck`) uses the same application service as the
desktop app and opens the same database, so it sees your registered projects and
detected apps. Every command accepts `--json` for a stable envelope that scripts and
AI agents can parse.

```text
# Apps
vantadeck --json scan apps --root "D:/Creative Apps"
vantadeck --json apps list
vantadeck --json apps override blender --version 4.2.3 --path "D:/Portable/Blender/blender.exe"

# Projects
vantadeck --json discover
vantadeck --json project import D:/Projects/MyGame --name MyGame
vantadeck --json project list --query MyGame --limit 100
vantadeck --json project show D:/Projects/MyGame
vantadeck --json project health D:/Projects/MyGame
vantadeck --json project pin D:/Projects/MyGame
vantadeck --json project launch D:/Projects/MyGame editor

# Source control
vantadeck --json project vcs D:/Projects/MyGame status
vantadeck --json project vcs D:/Projects/MyGame sync --yes
vantadeck --json project vcs D:/Projects/MyGame commit --message "Update assets" --yes
vantadeck --json project vcs D:/Projects/MyGame push --yes
vantadeck --json project vcs D:/Projects/MyGame switch --branch develop --yes

# Pipeline tasks
vantadeck --json tasks templates
vantadeck --json tasks list D:/Projects/MyGame
vantadeck --json tasks plan D:/Projects/MyGame build-windows
vantadeck --json tasks run D:/Projects/MyGame build-windows --yes
vantadeck tasks trust D:/Projects/MyGame

# Issue board
vantadeck --json issues list D:/Projects/MyGame
vantadeck --json issues add D:/Projects/MyGame --title "Catwalk collision" --kind bug --priority high
vantadeck --json issues move D:/Projects/MyGame <id> in-progress
vantadeck --json issues sync D:/Projects/MyGame

# Assets
vantadeck --json assets list D:/Projects/MyGame

# Folder layouts and organizing (see LAYOUTS.md)
vantadeck --json organize layouts D:/Projects/MyGame
vantadeck organize show game-art > our-layout.json
vantadeck --json organize import our-layout.json --project D:/Projects/MyGame
vantadeck --json organize plan D:/Projects/MyGame game-art
vantadeck --json organize apply D:/Projects/MyGame game-art --yes
vantadeck --json organize apply D:/Projects/MyGame game-art --folders-only --yes
vantadeck --json organize undo D:/Projects/MyGame --yes

# AI
vantadeck context D:/Projects/MyGame --write
vantadeck --json ai status
vantadeck ai setup --client claude-code --yes
vantadeck mcp

# Tools Hub metadata
vantadeck --json tools cache <index-url> --file reviewed-index.json
vantadeck --json tools list <index-url>
vantadeck --json tools verify tool.zip --sha256 <digest>
```

## Output and consent

JSON uses a stable envelope with `schemaVersion`, `command`, `success`, `data`,
`warnings`, and `errors` ([schema](../schemas/cli-envelope.schema.json)).

Commands that change files, run tasks, or touch a remote require `--yes`.
Non-interactive execution never infers consent: a missing confirmation exits with code 2
and, in JSON mode, the error code `CONFIRMATION_REQUIRED`. `tasks plan` prints the exact
command line a task would run without running it.

`vantadeck mcp` serves the same capabilities to AI apps over the Model Context Protocol
(stdio). AI agents can only run a project's tasks after you trust them on this machine
with `vantadeck tasks trust` or from the app; any change to `tasks.toml` revokes that
trust. See [AI_INTEGRATION.md](AI_INTEGRATION.md).

## Data location

The CLI uses the installed app's database under the operating system's data directory
(`com.vantadeck.launcher`). `--channel dev` points it at the PipelineOS Dev build's data
instead. Tests and managed deployments can set `VANTADECK_DATABASE_PATH` to an explicit
SQLite file without changing project metadata.

`tools cache` validates a user-supplied index file and stores it for offline use; it
does not perform a network request. `tools verify` never executes an artifact.

# Headless Mode

The `vantadeck` CLI is automation-safe and uses the same application contracts as the
desktop app.

```text
vantadeck --json apps list
vantadeck --json scan apps --root "D:/Creative Apps"
vantadeck --json apps override blender --version 4.2.3 --path "D:/Portable/Blender/blender.exe"
vantadeck --json project import D:/Projects/MyGame --name MyGame
vantadeck --json project show D:/Projects/MyGame
vantadeck --json project health D:/Projects/MyGame
vantadeck --json project vcs D:/Projects/MyGame status
vantadeck --json project vcs D:/Projects/MyGame sync --yes
vantadeck --json project vcs D:/Projects/MyGame commit --message "Update assets" --yes
vantadeck --json project vcs D:/Projects/MyGame push --yes
vantadeck --json project list --query MyGame --limit 100
vantadeck --json project pin D:/Projects/MyGame
vantadeck --json project launch D:/Projects/MyGame editor
vantadeck --json project vcs D:/Projects/MyGame switch --branch develop --yes
vantadeck --json tools cache https://tools.vantadeck.org/v1/index.json --file reviewed-index.json
vantadeck --json tools list https://tools.vantadeck.org/v1/index.json
vantadeck --json tools verify tool.zip --sha256 <digest>
```

JSON uses a stable envelope with `schemaVersion`, `command`, `success`, `data`,
`warnings`, and `errors`. Future destructive or remote-mutating commands require
`--yes`; non-interactive execution never infers consent. Missing confirmation returns
exit code 2 and error code `CONFIRMATION_REQUIRED` in JSON mode.

Machine-local state is stored under the operating system's Vantadeck data directory.
Tests and managed deployments can set `VANTADECK_DATABASE_PATH` to an explicit SQLite
file without changing project metadata.

`tools cache` validates a user-supplied index file and stores it for offline use; it
does not perform a network request. `tools verify` never executes an artifact.

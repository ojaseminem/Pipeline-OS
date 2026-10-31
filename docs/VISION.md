# Why Pipeline OS exists

## The problem

A game, film or VFX project is never just code. It is one specific engine version,
a handful of creative apps, gigabytes of source art and exports, references, build
scripts someone wrote once, and a lot of knowledge that lives in one person's head.

That shows up the same way on almost every small team:

- **"Which version opens this?"** Open a project in the wrong Unity or Unreal version
  and it upgrades itself, or breaks.
- **Files end up everywhere.** Concept art in Downloads, `final_v3_REAL.psd` in the
  project root, audio in a folder called `misc`. New people can't find anything, and
  nobody knows what is safe to move.
- **Builds are rituals.** A script on one machine, a checklist in a doc, a person who
  "knows how to do the Android build".
- **Getting someone started takes a day.** Install the right apps, find the right
  versions, learn the folder conventions, learn the build steps.

Big studios solve this with pipeline engineers, asset-management servers and in-house
launchers. Everyone else (indie and mid-size game teams, solo developers, students,
freelancers, small animation and VFX studios) has nothing between "a folder" and "a
pipeline department".

**Pipeline OS is the pipeline department for teams that don't have one.**

## What it does: the project knows how to run itself

Pipeline OS turns each project into something that explains itself. Everything it
needs is plain text inside the project (`.pipelineos/`), shared through the version
control the team already uses:

| Step | What Pipeline OS does | Lives in |
| --- | --- | --- |
| **Open** | Finds every installed creative app and version, and opens the project in the one it needs. | `project.toml` |
| **Organize** | Gives the project a folder layout everyone follows, and safely sorts an existing mess into it, with a preview and undo. | `layouts/*.json` |
| **Build** | Turns builds, cooks, renders, exports and tests into buttons that run the same way for everyone, including CI and AI agents. | `tasks.toml`, `presets.toml` |
| **Track** | Keeps the board, asset reviews and the project brief next to the work, and handles Git for large binary projects. | `issues/`, `assets.toml`, `context.md` |

A new teammate clones the repo, opens it in Pipeline OS, and the right engine
version, the folder structure, the build buttons and the open work are all there.

## Why it's different

- **Local-first, no server.** No account, no subscription, no telemetry, and it works
  offline. It suits a solo artist and a 30-person team equally.
- **Plain, editable files.** Layouts are JSON, tasks are TOML, the brief is Markdown.
  Edit them in any text editor, review them in a pull request, share them by copy and
  paste. Nothing is locked inside the app.
- **Engine-aware and careful.** It never moves files an engine manages (Unity
  `Assets/`, Unreal `Content/`, Godot imports), always shows a preview before changing
  anything, and asks you to review commands before they run.
- **Ready for AI.** The same project description lets Claude, Cursor, Codex and other
  agents help with a project safely: they can read it, propose layouts and tasks, and
  preview changes, while running and moving things stays with you.

## Principles

1. **Preview before change, undo when possible.** Nothing moves, runs or syncs
   without the user seeing what will happen.
2. **The project folder is the source of truth.** Anything a team shares is a plain
   file in the project, not a row in a database.
3. **Easy to share.** Every preset (layouts, tasks) can be copied, pasted, exported
   and imported as text.
4. **Small teams first.** Every feature must work for one person with no setup, and
   scale to a team through Git.
5. **No surprises.** No telemetry, no background network use beyond the update check.

## What it is not

Not a cloud asset manager, a render farm, a license server, or a replacement for the
engine or creative apps. See [FUTURE_SCOPE.md](FUTURE_SCOPE.md).

## Where we're focusing next

In order of how much they help with the core problem:

1. **Organize, further.** A health check that spots loose files and suggests a layout;
   naming conventions (`T_Rock_Albedo`, `SM_Crate`) as part of a layout; engine-safe
   moves inside Unity `Assets/` that carry `.meta` files along.
2. **Set up this project.** One screen for a newly cloned project: the engine version
   it needs, missing apps, the folder layout, build tasks and Git LFS, each with a fix.
3. **Studio packs.** One shareable file with a team's layouts and task presets, so a
   studio standard can be imported into every project in one step.
4. **Multi-step pipelines.** Chain tasks (test, build, package, upload) without writing
   a script.
5. **Art in version control.** View, review and compare versions of art files through
   Git.
6. **Perforce** in the app, for teams that use it.

The detailed list is in [ROADMAP.md](../ROADMAP.md).

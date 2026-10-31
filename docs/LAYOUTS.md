# Folder layouts

A folder layout describes the folders a project should have, and simple rules for
sorting loose files into them. Pipeline OS uses layouts to create a project's folder
structure and to organize an existing project (Assets tab, **Organize project**, or
`vantadeck organize`).

## Where layouts live

| Source | Location | Shared with |
| --- | --- | --- |
| This project | `.pipelineos/layouts/<id>.json` | Everyone, through version control |
| My layouts | `<app data>/layouts/<id>.json` | Every project on this computer |
| Built in | Shipped with Pipeline OS ([source](../crates/studio/src/layouts)) | Everyone |

A project or personal layout with the same `id` as a built-in one replaces it, so
"Customize" on a built-in layout saves your version under the same id.

Built-in layouts: **Game team** (`game-art`), **Studio pipeline**
(`studio-pipeline`), **Film / VFX / Animation** (`film-vfx`) and **Client or freelance
project** (`client-project`).

## The format

Layouts are plain JSON, so they can be edited in any text editor, reviewed in a pull
request, and shared by copy and paste:

```json
{
  "format": "pipelineos-layout",
  "version": 1,
  "id": "our-studio",
  "name": "Our studio",
  "description": "How we lay out every game project.",
  "folders": ["Art/Concept", "Art/Source", "Audio", "Docs"],
  "rules": [
    { "to": "Art/Concept", "match": ["*concept*", "*sketch*"] },
    { "to": "Art/Source", "match": ["*.psd", "*.kra"], "kinds": ["source"] },
    { "to": "Audio", "kinds": ["audio"] },
    { "to": "Docs", "match": ["*.pdf", "*.docx"] }
  ],
  "keep": ["README*", "LICENSE*"]
}
```

| Field | Meaning |
| --- | --- |
| `id` | File name and identifier: lowercase letters, digits and dashes. |
| `folders` | Folders to create, relative to the project root, with `/`. |
| `rules` | Where files go. The **first** rule a file matches wins. |
| `rules[].to` | Destination folder. |
| `rules[].match` | File-name globs, case-insensitive (`*.psd`, `*concept*`, `*_v[0-9]*`). A glob containing `/` matches the whole project path. |
| `rules[].kinds` | Any of `image`, `source`, `model`, `scene`, `cache`, `animation`, `video`, `audio`, `document`. |
| `keep` | Globs for files that are never moved. |

A rule needs `match`, `kinds`, or both (both means the file must satisfy each). Rules
that only name kinds apply to recognized creative files; a glob that names an
extension, like `*.zip`, can pick up other files too.

### Plain-text layouts

A folder list in a `.txt` file (or pasted text) imports too. Each line is a folder;
after `<-`, globs and kinds for files that belong there. The first `#` line names it:

```text
# Our studio
Art/Concept   <- *concept* *sketch*
Art/Source    <- *.psd *.kra source
Audio         <- audio
Docs
```

## How organizing stays safe

- **Preview first.** You see every file that would move and why, and can untick any
  of them.
- **Only loose files.** Files already inside one of the layout's top-level folders stay
  where they are, as do hidden and git-ignored files and build or cache folders.
- **Engine folders are never touched.** Unity `Assets/`, `Packages/` and
  `ProjectSettings/`; Unreal `Content/`, `Source/`, `Config/` and `Plugins/`; and in
  Godot projects, scenes, scripts and any file with a `.import` beside it. Moving
  these outside the editor would break references.
- **Never overwrites.** If a file with the same name is already at the destination,
  that move is skipped and listed.
- **Undo.** The last organize is recorded on your computer and can be undone; folders
  it created are removed again if they are empty.

After organizing a project that uses Git, commit the moves so the team gets the new
structure. Git recognizes moved files as renames.

# Application Manifest Specification v1

Application manifests live in `manifests/apps`, validate against
`schemas/app-manifest.schema.json`, and use stable kebab-case IDs. They declare
platforms, executable names, discovery hints, file types, and structured launch
argument arrays.

Arguments may contain documented placeholders such as `{file}`, `{projectFile}`, and
`{projectRoot}`. Shell metacharacters, command chaining, redirection, substitutions,
and remote scripts are rejected. A manifest cannot grant permissions or install code.

Contributions require fixtures or detection evidence that contains no private paths.

# Add a command to goh in five minutes

goh completes a command from a JSON spec: its subcommands, options and arguments, each with a description. 191
ship inside the binary (`crates/goh-core/specs/`). This page takes you from a command with no spec to a pull request
that bundles one.

## 1. Start from its `--help` (1 minute)

```bash
goh spec gen mytool --print > mytool.json
```

`goh spec gen` reads `mytool --help` and keeps the options (and whether they take a value) and subcommands it
recognises in the usual layouts (clap, cobra, argparse, getopt). It leaves out what it does not understand; it never
invents. Without `--print` it writes `~/.config/goh/specs/mytool.json` for you alone. It refuses to replace a
bundled spec unless you pass `--force`.

## 2. Make it good (3 minutes)

Open `mytool.json`. The format is [docs/schemas/completion.schema.json](schemas/completion.schema.json); a short
example is [docs/examples/git.completion.json](examples/git.completion.json).

```json
{
  "schemaVersion": 1,
  "name": "mytool",
  "description": "One line on what the tool does",
  "subcommands": [
    { "name": "deploy", "description": "Deploy the current project",
      "options": [
        { "name": "--env", "description": "Target environment", "takesValue": true,
          "args": [{ "name": "env", "suggestions": [{ "name": "staging" }, { "name": "production" }] }] }
      ],
      "args": [{ "name": "path", "template": "folders", "isOptional": true }] }
  ],
  "options": [
    { "name": "--version", "description": "Print the version", "takesValue": false }
  ]
}
```

What makes a spec feel right:

- **A short description on every subcommand and option.** It is what the popup shows next to the name.
- **Arguments say what they are.** `"template": "filepaths"` or `"folders"` lists paths; `"extensions": ["py"]`
  narrows them by type; `"provider": "git-branches"` (or `docker-containers`, `kube-pods`, `package-scripts`, ...,
  the full list is in the schema) asks goh for live values. Use `"suggestions"` only for a fixed set, never for
  values that depend on the user's project.
- **`"takesValue": true`** on an option that eats the next word, so goh does not offer a subcommand there.
- **Drop noise:** `-h`/`--help` repeated on every subcommand, deprecated flags, internal commands.
- `"isVariadic": true` for an argument that repeats (`git add a b c`); `"runsCommand": true` for one that runs the
  rest of the line as a command (like `sudo`).

## 3. Try it (1 minute)

```bash
cp mytool.json ~/.config/goh/specs/
goh reload
```

Type `mytool ` and look at what the menu offers. A spec in `~/.config/goh/specs/` replaces a bundled one of the same
name, so you can improve an existing spec the same way. Delete your copy when you are done.

## 4. Send it

```bash
cp mytool.json crates/goh-core/specs/
cargo test -p goh-core      # every bundled spec must parse and not be empty
```

Cargo embeds every JSON file in `crates/goh-core/specs/`; nothing else needs registering. In the same pull request,
add the command to the `unreleased` line under `## Changes` in
[docs/features/bundled-specs.md](features/bundled-specs.md) (CI asks for it). Title the pull request
`feat(spec): add mytool` ([Conventional Commits](../CONTRIBUTING.md)).

A spec written from your own knowledge of the tool, or from its `--help`, is fine. Do not copy specs from other
projects whose license does not allow it; Fig specs are imported only through `tools/import-specs`.

Looking for something to add? Issues labelled
[`good first issue`](https://github.com/caophuoclong/goh/labels/good%20first%20issue) list commands with no spec yet.

# 06 — Completion Engine, Schema & IPC Technical Design

## Objective

From **line prefix + cursor + shell + cwd** produce a safe, ranked list of suggestions and a precise edit range. The engine does **not** execute user command text. It is independent from UI, OS and shell implementation.

## Pipeline

```mermaid
flowchart LR
  A[Editor snapshot] --> B[Normalize & tokenize]
  B --> C[Cursor-aware AST]
  C --> D[Resolve command/spec node]
  D --> E[Collect static candidates]
  D --> F[Filesystem provider]
  D --> G[Optional bounded generator]
  E --> H[Rank/dedupe]
  F --> H
  G --> H
  H --> I[Validate replacement range]
  I --> J[CompletionResponse]
```

## Parsing contract

- Parse **prefix at cursor** and know suffix after cursor so replacement doesn't erase unexpected content. Preserve raw input and a normalized representation separately.
- Start with pragmatic grammar for quotes, whitespace, escapes, `--flag=value`, short flags, subcommands, path separators and incomplete tokens. Add pipelines, command substitution and redirection cautiously. Unrecognized structure -> conservative/no suggestions.
- Character indices: define **UTF-8 byte offsets** in Rust protocol; adapters must convert from their own cursor-index convention (UTF-16 in Windows APIs where applicable) and validate boundaries to avoid slicing multibyte Unicode.
- A candidate carries `replace_start_byte`, `replace_end_byte` plus `insert_text`; display text is not insertion text.
- Do not perform glob expansion, variable substitution, subprocess invocation or shell evaluation during parsing.

## Completion spec v1

A safe, declarative JSON file with `schemaVersion=1`, `name`, description, recursive `subcommands`, `options`, `args` and `suggestions`. See [JSON Schema](../schemas/completion.schema.json) and [example](../examples/git.completion.json). This is **our own normalized format**, inspired by the public Fig completion-spec concept, not a claim of drop-in Fig runtime compatibility.

### Example

```json
{
  "schemaVersion": 1,
  "name": "tool",
  "description": "Example CLI",
  "subcommands": [{ "name": "run", "description": "Run a task", "options": [
    { "name": "--verbose", "description": "Enable detailed output", "takesValue": false },
    { "name": "--target", "description": "Target name", "takesValue": true,
      "args": [{ "name": "target", "suggestions": [{"name": "dev"}, {"name": "prod"}] }] }
  ] }]
}
```

Schema v1 intentionally excludes executable JS/TS code and remote URLs. Fig community specs may contain dynamic generators and TypeScript functions; **importer** must transform supported static structures only, report unsupported constructs, and attribute license.

## Candidate data model

| Field | Type | Meaning |
|---|---|---|
| `id` | opaque string | Stable within response for acceptance |
| `label` | string | Shown in UI |
| `description` | string? | Short accessible description |
| `kind` | enum | command/subcommand/option/argument/file/directory |
| `insert_text` | string | Shell-ready (escaped appropriately by adapter/engine contract) |
| `replace_start_byte` / `replace_end_byte` | int | Valid UTF-8 byte span in snapshot |
| `match_ranges` | `[[start_byte, end_byte], …]` | UTF-8 byte ranges of `label` that matched the typed token; ascending, non-overlapping, on character boundaries. Empty for no typed text. Display only; never used for editing |
| `score` | number | Deterministic rank; not security signal |
| `generation` | uint64 | Discard stale responses |
| `source` | enum | bundled/local/filesystem/built-in generator |

## Resolution and ranking

1. Detect command root, follow matching subcommand tree and option arguments; respect option's `takesValue`.
2. Combine providers appropriate to active argument (static choices, directory, file, dynamic if enabled).
3. Filter by prefix: exact prefix > case-folded prefix > fuzzy (P1); prioritize context match; stable alphabetical tie-break.
4. Deduplicate by `(kind, insert_text, range)`; cap visible results (e.g. 8–12) and total response (e.g. 50).
5. Empty/unknown path returns no candidates without error; no arbitrary command execution.

## Dynamic generators: deliberately limited

- MVP can ship read-only **built-in providers** (e.g. git branch names) behind an explicit allow switch; never allow raw commands embedded in downloaded specs to execute.
- Provider API is internal Rust trait: `(cwd, argv, deadline, cancellation, policy) → suggestions`.
- Spawn by argv array, no shell interpolation; restricted environment, no stdin access, per-provider timeout default 100 ms, memory/CPU limits where supported, max output size, no network (policy/enforcement platform-dependent).
- Remote/sandboxed script generators are P2 and demand separate threat model and permission design.

## IPC wire example (illustrative, not codegen schema)

Request:

```json
{
  "v": 1,
  "type": "complete",
  "session_id": "opaque-session-uuid",
  "generation": 42,
  "shell": "zsh",
  "cwd": "/home/dev/project",
  "buffer": "git ch",
  "cursor_byte": 6,
  "editor_state": "editing"
}
```

Response:

```json
{
  "v": 1,
  "type": "results",
  "session_id": "opaque-session-uuid",
  "generation": 42,
  "suggestions": [{
    "id": "git:checkout",
    "label": "checkout",
    "description": "Switch branches or restore paths",
    "kind": "subcommand",
    "insert_text": "checkout ",
    "replace_start_byte": 4,
    "replace_end_byte": 6,
    "match_ranges": [[0, 2]],
    "score": 100,
    "source": "bundled"
  }]
}
```

Accept request and reply (the adapter, not the UI, sends these):

```json
{ "v": 1, "type": "accept", "session_id": "opaque-session-uuid", "generation": 42,
  "suggestion_id": "git:checkout", "buffer_hash": "sha256:…" }
```

```json
{ "v": 1, "type": "edit", "session_id": "opaque-session-uuid", "generation": 42,
  "replace_start_byte": 4, "replace_end_byte": 6, "insert_text": "checkout ", "cursor_byte": 13 }
```

If `generation` or `buffer_hash` no longer matches, the daemon replies `{"type":"stale"}` and the adapter changes nothing. Shape of the messages: [`schemas/ipc.schema.json`](../schemas/ipc.schema.json), sample: [`examples/completion-response.json`](../examples/completion-response.json).

### Behaviour the prototype fixes as reference

The interactive prototype ([17](17-design-system-handoff.md)) is the behavioural reference for ranking and insertion: exact prefix first, then in-order subsequence matches (accent- and case-folded, so `tai` finds `Tài liệu/`), stable list order as tie-break; paths with spaces are inserted escaped; subcommands and options get a trailing space, directories end in `/` with none; accepting a subcommand opens the next context immediately. The Rust engine must reproduce these cases as golden tests.

**Privacy:** the buffer is sent **only** over authenticated same-user IPC and never persisted. Request size cap and `session_id` prevent cross-session mixups. Buffer hash/generation re-check must happen at accept time; daemon must not authorize edits based solely on an old response ID.

## Schema/version strategy

- Protocol `v` increments on breaking changes; client/daemon negotiate or disable gracefully.
- Spec `schemaVersion` independent from IPC `v`.
- On schema validation failure: skip spec, report file path plus error code (not spec contents with secrets).
- Distribution: bundled specs with signed release; local user specs are trusted data only, never executable.

## Acceptance tests

- `git ch`, `git commit -`, `git -C dir status`; `--flag=value`; paths with spaces, Unicode, hidden folders.
- Quoting incomplete (`'`, `"`), backslash, escaped space, multi-command `&&`, pipeline, cursor in middle, pasting large blocks.
- Prefix ambiguity and local override precedence.
- Stale generation response ignored, UTF-8 range validation, overlong socket messages rejected.


## Validity (as-you-type suggestions)

An explicit Tab always completes (an unknown command gets file names). Suggestions that appear **without being asked**
(the shell adapter's as-you-type mode) must not fire for every command or for nonsense, so the engine also reports
whether the line is a **recognised command in progress** (`Completion::valid`, request flag `live`):

- the first word has a completion spec (bundled or user-installed); unknown commands are not valid;
- every completed word before the one being typed is *placed* by the spec: it is a subcommand of the current node, or
  the value of an option that takes one, or fits an argument slot of the current node (a variadic argument accepts any
  number of words). A word with no place (`git foo c`, `git checkout main extra c`) makes the line invalid;
- an unterminated quote is not valid, and neither is a command word still being typed (`gi`);
- an exact, plain command name at end of line (`docker`, `git`, `npm`) is valid before its first space: offer the
  next word with a zero-width replacement at the cursor and prepend the missing space to its insertion. Do not
  apply this transition inside a longer word, to quoted/escaped names, or when text follows the cursor;
- the word being typed only needs to have candidates; with none, nothing is shown.

Options are deliberately **not** checked (an unknown `--flag` does not invalidate the line), because specs list only
some of a tool's options; the cost is that a mistyped flag still shows suggestions. Alternatives considered: asking the
shell whether the command exists (`whence`/`command -v`) would make `ls`, `cat`... "valid" but with nothing to suggest
and ignore which words are meaningful; dry-running or parsing `--help` output is slow, runs user commands and breaks
the "no execution" rule. Validity is therefore only as good as the specs: with the minimal bundled `git` spec,
`git commit file` counts as invalid because the spec gives `commit` no arguments.

## Matching rules for what is typed

- Exact prefix, then case- and accent-folded prefix, then (only for **two or more** typed characters) in-order scattered
  matches. A single typed letter matches prefixes only: `c` should not list `fetch` because it contains a `c`.
- Options are offered for an empty word or one starting with `-`, never for a plain word such as a remote name.
- Suggestions that appear **unasked** (as-you-type mode, `live` requests) additionally drop the scattered matches
  whenever any prefix match exists (`Completion::prefix_matches_only`); asked-for completion (Tab) keeps them.

## Which specs are served

The daemon serves the specs in `$GOH_SPEC_DIR` (default `~/.config/goh/specs`) **plus the ones bundled in the binary**
(`crates/goh-core/specs/`: 40 commands), so the common commands work with no setup. The original `git` and
`npm` specs are retained; 36 commands, including expanded Docker and Node, are converted from pinned MIT-licensed Fig declarations by
`tools/import-specs/`. Cargo discovers and embeds the checked-in JSON; building and running goh requires no
upstream checkout, Node or network. `tools/import-specs/report.json` records source hashes and omitted features,
and `third_party/fig-autocomplete/LICENSE` preserves the upstream license. This is a static subset, including
selected AWS S3/configure/STS and gcloud auth/config/run trees; other delegated specs and executable generators
are reported as omissions. A spec of yours for the same command **replaces** the bundled one (never merged), the first file wins if two
files define one command, and an unusable file is skipped with a warning. `goh specs` lists each command, its source
and what it replaces for the **next daemon start**, keeping tab-separated rows on stdout and an explanatory notice on
stderr. `goh-status` labels the same configuration explicitly. These listings read the current files; the running daemon
keeps its startup snapshot and may differ. Restart `gohd` after changing specs.

Bundled specs are static and do not invent project-specific values: Git branch names and npm script names are not
suggested until a provider reads them from the current project. Their argument slots remain valid for user-entered values.

## Fixed runtime values (Docker, frequent directories)

Argument specs may name one of four fixed `provider` values: `docker-containers`,
`docker-all-containers`, `docker-images`, or `frecent-dirs` (used by `cd`, offered only once a word is typed, as directories). `frecent-dirs` runs
`zoxide query --list` when zoxide is installed and has entries, otherwise reads goh's own history
(`$XDG_DATA_HOME/goh/dirs`, default `~/.local/share/goh/dirs`, `GOH_VISITS` overrides), which the shell
adapters feed with `goh visit <dir>` on every directory change; frecency weights visits by age. The daemon implements only read-only `docker ps`/`docker image ls`
queries with fixed argument arrays, null stdin/stderr, a 120 ms read deadline, 512 KiB output cap,
process cleanup and a two-second cache (including failures). No shell or spec-supplied script is executed.
Names are validated, JSON rows parsed, and the engine supplies matching, escaping and edit spans.
Explicit Docker host/context/config overrides suppress providers; the daemon's inherited environment is
otherwise used. Built-in provider candidates use the existing IPC source `built-in generator`.

Empty words at leaf commands discover flags even when positional arguments exist. `isCommand` marks an
opaque delegated command and variadic following words, preventing parent options being offered inside
`docker exec <container> <program> ...`. A `--` separator stops option completion. Metadata for other
project-specific arguments still needs its own provider; free-form values cannot be inferred from a static spec.

## Project values and the rest (read-only)

Besides the Docker/zoxide providers above, the daemon fills argument slots from plain files in the working
directory (nothing is run): `git-branches`, `git-remotes`, `git-tags` (newest version first), `git-stashes`,
`package-scripts`, `make-targets`, `just-recipes` (the `#` comment above a recipe is its description),
`taskfile-tasks`, `compose-services`, `cargo-bins`, `cargo-examples`, `cargo-features` and `workspace-packages`
(Cargo workspace members and npm/pnpm workspaces; used by `cargo -p` and `pnpm --filter`). The two that need git's
index, `git-changed-files` (modified and untracked, for `git add`, `restore`, `diff`) and `git-staged-files` (for
`git reset`, `restore --staged`), run `git ls-files -m -o --exclude-standard -z` / `git diff --cached --name-only
--relative -z` with fixed arguments, `core.fsmonitor=false`, no optional locks, a 120 ms deadline and the same
cache as Docker; outside a repository or when git is missing they are empty. An argument may name several
providers (`"providers": [...]`) and files may be narrowed with `"extensions": ["py"]` (directories always stay).

## Slips, variables, risk notes

- **Slips.** When nothing matches a word of four or more characters, subcommands, options and spec values one slip
  away (a swapped, dropped, added or wrong letter; two slips from six characters) are offered instead, ranked
  below any real match. File names are never corrected.
- **Variables.** `$NA`, `${NA` and `export NA` / `unset NA` complete from the names (never values) of the shell's
  exported variables, which the adapter sends with the rest of its context.
- **Risk notes.** Suggestions that can do damage (`git push --force`, `git reset --hard`, `rm -r`, `kubectl delete`,
  `terraform apply`, ...) get `⚠ what it does` at the front of their description, and for kubectl/helm the current
  kube context (`· context: prod`), for Terraform the workspace (`.terraform/environment`). Nothing is blocked.
- **Snippets.** `snippets.json` next to the config file: `name`, `template`, optional `description`. `{{cursor}}`
  marks where the cursor lands (`Candidate.cursor_offset`). A snippet is offered while typing the first word (two
  characters while typing, one on Tab), ahead of everything else.

## Your command record (opt-in)

With `privacy.recordCommands` (or `GOH_RECORD=on`) the adapters hand every successful command line to `goh record`,
which appends `epoch TAB cwd TAB line` to `~/.local/share/goh/runs` (`GOH_RUNS`; mode 0600; at most ~4000 lines
kept). `runs::looks_secret` leaves out a line containing a secret-like word, a `user:pass@host` URL or a long
opaque token; a line starting with a space is also never recorded. From it the daemon offers:

- **History lines**: at the end of a line of three or more characters, the lines you ran that start with it, by use,
  recency and folder (`Kind::History`, after the spec's suggestions; at most three).
- **Prediction**: at an empty prompt, what followed the last command run in this folder (within 30 minutes) at
  least twice before, as the full line when the same line repeated, else just the command (`git commit `). The zsh
  adapter shows it as ghost text; Right or Tab takes it (`display.predict` / `GOH_PREDICT=off` turns it off).
- **Option values**: `kubectl -n <TAB>` lists the values earlier runs gave `-n`, first. Options named like a
  secret (`token`, `pass`, `key`, ...) and values that look like one are never recalled.

`goh forget` deletes the record along with what was learned from your picks.

## Specs for programs that have none (opt-in)

`goh spec gen <command>` writes a spec from `--help`. With `completion.autoSpecs` the daemon does it once, in the
background, for the first complete word of a line that is a program on your `PATH` with no spec (never `sudo`,
`kill`, `reboot` and similar), saves it to `~/.config/goh/specs/<command>.json`, never over an existing file, and
serves it at once.
